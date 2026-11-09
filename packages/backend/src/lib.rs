/// Authentication
pub mod authentication;
/// Role checks on shared resources
pub mod authorization;
/// Backend configuration
pub mod configuration;
/// Controllers providing the web API endpoints
mod controllers;
/// Backend errors
pub mod error;
/// Defines how project content is loaded from different filesystems
pub mod filesystems;
/// Models abstracting the database access
pub mod models;
/// State of the backend, e.g. connection pool
mod state;
/// Fixtures for the HTTP-level tests
#[cfg(all(test, feature = "integration"))]
mod test_support;
mod worker_registry;

#[cfg(feature = "standalone")]
/// Errors which my occure during standalone operations
pub mod standalone_error;

use std::pin::Pin;
use std::sync::Arc;

use axum::{Extension, Router};
#[cfg(feature = "standalone")]
use http::Method;
use macworp_core::routing::{
    GroupsRouting, IsRouting, LoginRouting, ProjectsRouting, RunsRouting, SharesRouting,
    UsersRouting, UtilitiesRouting, WorkerRouting, WorkflowIntegrationsRouting,
};
use tokio::net::TcpListener;
#[cfg(feature = "standalone")]
use tower_http::cors::AllowOrigin;
use tower_http::cors::CorsLayer;

use crate::{
    configuration::Configuration, controllers::*, error::Error,
    filesystems::is_filesystem::IsFilesystem, state::State,
};

#[cfg(feature = "integration")]
use crate::filesystems::local_filesystem::LocalFilesystem;
#[cfg(feature = "standalone")]
use crate::filesystems::remote_filesystem::RemoteFilesystem;

/// Builds the CORS layer from the configured origins, `None` if cross-origin access is disabled
#[cfg(feature = "standalone")]
fn cors_layer(config: &Configuration) -> Option<CorsLayer> {
    let origins = config.cors_allowed_origins();
    if origins.is_empty() {
        return None;
    }

    let allow_origin = if origins.iter().any(|origin| origin == "*") {
        tracing::warn!("CORS allows any origin, restrict `cors_allowed_origins` in production");
        AllowOrigin::any()
    } else {
        AllowOrigin::list(origins.iter().filter_map(|origin| origin.parse().ok()))
    };

    Some(
        CorsLayer::new()
            .allow_methods([
                Method::GET,
                Method::POST,
                Method::PUT,
                Method::PATCH,
                Method::DELETE,
                Method::OPTIONS,
            ])
            .allow_headers(vec![
                http::header::ACCEPT,
                http::header::CONTENT_TYPE,
                http::header::DNT,
                http::header::AUTHORIZATION,
                http::header::CONTENT_RANGE,
            ])
            .allow_origin(allow_origin),
    )
}

pub struct Backend<F: IsFilesystem + 'static> {
    _filesystem: std::marker::PhantomData<F>,
}

impl<F> Backend<F>
where
    F: IsFilesystem,
{
    /// Periodically requeues (or fails) runs whose worker stopped heartbeating, see
    /// `models::RunTable::requeue_expired`, and fails runs over their queue or running time limit,
    /// see `models::RunTable::expire_timeouts`. Runs until aborted.
    async fn requeue_expired_runs(state: Arc<State>) {
        let period = std::time::Duration::from_secs(
            u64::from(state.config().worker_lease_seconds() / 3).max(1),
        );
        let max_attempts = i32::try_from(state.config().worker_max_attempts()).unwrap_or(i32::MAX);
        loop {
            tokio::time::sleep(period).await;
            match models::RunTable::requeue_expired(state.db(), max_attempts).await {
                Ok(result) if result != models::RequeuedRuns::default() => {
                    tracing::warn!("Worker leases expired: {result:?}");
                }
                Ok(_) => {}
                Err(error) => tracing::error!("Checking worker leases failed: {error}"),
            }
            match models::RunTable::expire_timeouts(
                state.db(),
                state.config().run_timeout(),
                state.config().queued_timeout(),
            )
            .await
            {
                Ok(result) if result != models::ExpiredRuns::default() => {
                    tracing::warn!("Runs hit their time limit: {result:?}");
                }
                Ok(_) => {}
                Err(error) => tracing::error!("Checking run time limits failed: {error}"),
            }
        }
    }

    /// Starts the backend on the provides TCP port.
    ///
    /// # Arguments
    /// * `tcp_listener` - The TCP listener to accept incoming connections on
    /// * `shutdown_signal` - A signal to gracefully shutdown the backend, e.g.
    /// * `state` - The state to use for the backend, e.g. database connection pool and configuration
    pub async fn internal_start(
        tcp_listener: TcpListener,
        shutdown_signal: Pin<Box<dyn Future<Output = ()> + Send + 'static>>,
        state: State,
        cors: Option<CorsLayer>,
    ) -> Result<(), Error> {
        let state = Arc::new(state);

        let router = Router::new()
            .nest(
                UtilitiesRouting::controller_path(),
                UtilitiesController::routes(),
            )
            .nest(
                LoginRouting::controller_path(),
                LoginController::routes(state.clone()),
            )
            .nest(
                UsersRouting::controller_path(),
                UsersController::routes::<F>(state.clone()),
            )
            .nest(
                ProjectsRouting::controller_path(),
                ProjectsController::<F>::routes(state.clone()),
            )
            .nest(
                WorkflowIntegrationsRouting::controller_path(),
                WorkflowIntegrationsController::routes(state.clone()),
            )
            .nest(
                RunsRouting::controller_path(),
                RunsController::routes(state.clone()),
            )
            .nest(
                GroupsRouting::controller_path(),
                GroupsController::routes(state.clone()),
            )
            .nest(
                SharesRouting::controller_path(),
                SharesController::routes(state.clone()),
            )
            .nest(
                WorkerRouting::controller_path(),
                WorkerController::<F>::routes(state.clone()),
            )
            .layer(Extension(state.clone()))
            .with_state(state.clone());
        let router = match cors {
            Some(cors) => router.layer(cors),
            None => router,
        };

        tracing::info!(
            "Starting MaCWorP backend on {}",
            tcp_listener.local_addr().unwrap()
        );

        let requeue = tokio::spawn(Self::requeue_expired_runs(state));
        let result = axum::serve(tcp_listener, router)
            .with_graceful_shutdown(shutdown_signal)
            .await
            .map_err(Error::BackendStart);
        requeue.abort();
        result
    }
}

#[cfg(feature = "standalone")]
impl Backend<RemoteFilesystem> {
    /// Database and starts the backend on the provides TCP port.
    ///
    /// # Arguments
    /// * `tcp_listener` - The TCP listener to accept incoming connections on
    /// * `shutdown_signal` - A signal to gracefully shutdown the backend, e.g. when receiving a SIGINT
    /// * `config` - The backend configuration, e.g. database URL, interface and
    pub async fn start(
        tcp_listener: TcpListener,
        shutdown_signal: Pin<Box<dyn Future<Output = ()> + Send + 'static>>,
        config: Configuration,
    ) -> Result<(), Error> {
        config.validate()?;
        let state = State::new_for_standalone(config).await?;
        let cors = cors_layer(state.config());
        Self::internal_start(tcp_listener, shutdown_signal, state, cors).await
    }
}

#[cfg(feature = "integration")]
impl Backend<LocalFilesystem> {
    /// Migrates the database and starts the backend on the provides TCP port.
    ///
    /// # Arguments
    /// * `tcp_listener` - The TCP listener to accept incoming connections on
    /// * `shutdown_signal` - A signal to gracefully shutdown the backend
    /// * `config` - The backend configuration to use
    pub async fn start(
        tcp_listener: TcpListener,
        shutdown_signal: Pin<Box<dyn Future<Output = ()> + Send + 'static>>,
        config: Configuration,
        desktop_user_name: String,
    ) -> Result<(), Error> {
        use macworp_database_migration::{Migrator, MigratorTrait};

        use crate::models::{ActiveUser, User, UserTable};

        config.validate()?;
        let state = State::new_for_integration(config).await?;

        Migrator::up(state.db(), None)
            .await
            .map_err(Error::DatabaseMigration)?;

        match UserTable::find_by_username(&desktop_user_name)
            .one(state.db())
            .await
            .map_err(|err| Error::Database("selecting user on integrated start".to_string(), err))?
        {
            Some(user) => {
                use sea_orm::{ActiveModelTrait, ActiveValue::Set};
                let mut user: ActiveUser = user.into();
                user.password = Set(Some(state.config().worker().password().to_string()));
                let user: User = user.update(state.db()).await.map_err(|err| {
                    Error::Database(
                        "updating user password on integrated start".to_string(),
                        err,
                    )
                })?;
                user
            }
            None => {
                let new_user = ActiveUser::builder()
                    .set_username(&desktop_user_name)
                    .set_firstname(&desktop_user_name)
                    .set_lastname(&desktop_user_name)
                    .set_email("".to_string())
                    .set_password(state.config().worker().password().to_string());
                let user = new_user.insert(state.db()).await.map_err(|err| {
                    Error::Database("creating new user on integrated start".to_string(), err)
                })?;
                user.into()
            }
        };

        // The desktop UI talks to the backend in-process, no cross-origin access needed
        Self::internal_start(tcp_listener, shutdown_signal, state, None).await
    }
}
