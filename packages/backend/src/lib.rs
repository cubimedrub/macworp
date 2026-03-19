/// Authentication
pub mod authentication;
/// Backend configuration
pub mod configuration;
/// Controllers providing the web API endpoints
mod controllers;
/// Backend errors
pub mod error;
/// Models abstracting the database access
pub mod models;
/// State of the backend, e.g. connection pool
mod state;

#[cfg(feature = "standalone")]
/// Errors which my occure during standalone operations
pub mod standalone_error;

use std::pin::Pin;
use std::sync::Arc;

use axum::Router;
use macworp_core::routing::{IsRouting, LoginRouting, UsersRouting, UtilitiesRouting};
use tokio::net::TcpListener;

use crate::{configuration::Configuration, controllers::*, error::Error, state::State};

pub struct Backend;

impl Backend {
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
                UsersController::routes(state.clone()),
            )
            .with_state(state);

        axum::serve(tcp_listener, router)
            .with_graceful_shutdown(shutdown_signal)
            .await
            .map_err(Error::BackendStart)
    }
}

#[cfg(feature = "standalone")]
impl Backend {
    /// Database and starts the backend on the provides TCP port.
    ///
    /// # Arguments
    /// * `tcp_listener` - The TCP listener to accept incoming connections on
    /// * `shutdown_signal` - A signal to gracefully shutdown the backend, e.g. when receiving a SIGINT
    /// * `config` - The backend configuration, e.g. database URL, interface and
    pub async fn start_standalone(
        tcp_listener: TcpListener,
        shutdown_signal: Pin<Box<dyn Future<Output = ()> + Send + 'static>>,
        config: Configuration,
    ) -> Result<(), Error> {
        let state = State::new_for_standalone(config).await?;
        Self::internal_start(tcp_listener, shutdown_signal, state).await
    }
}

#[cfg(feature = "integration")]
impl Backend {
    /// Migrates the database and starts the backend on the provides TCP port.
    ///
    /// # Arguments
    /// * `tcp_listener` - The TCP listener to accept incoming connections on
    /// * `shutdown_signal` - A signal to gracefully shutdown the backend
    /// * `config` - The backend configuration to use
    pub async fn start_integration(
        tcp_listener: TcpListener,
        shutdown_signal: Pin<Box<dyn Future<Output = ()> + Send + 'static>>,
        config: Configuration,
    ) -> Result<(), Error> {
        use macworp_database_migration::{Migrator, MigratorTrait};

        use crate::models::{ActiveUser, User, UserTable};

        let state = State::new_for_integration(config).await?;

        Migrator::up(state.db(), None)
            .await
            .map_err(Error::DatabaseMigration)?;

        match UserTable::find_by_username("desktop")
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
                    .set_username("desktop".to_string())
                    .set_firstname("desktop".to_string())
                    .set_lastname("desktop".to_string())
                    .set_email("".to_string())
                    .set_password(state.config().worker().password().to_string());
                let user = new_user.insert(state.db()).await.map_err(|err| {
                    Error::Database("creating new user on integrated start".to_string(), err)
                })?;
                user.into()
            }
        };

        Self::internal_start(tcp_listener, shutdown_signal, state).await
    }
}
