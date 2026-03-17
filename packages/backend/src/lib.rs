/// Authentication
pub mod authentication;
/// Backend configuration
pub mod configuration;
/// Controllers providing the web API endpoints
mod controllers;
/// Backend errors
pub mod error;
/// Middleware, e.g. authentication
mod middleware;
/// Models abstracting the database access
mod models;
/// State of the backend, e.g. connection pool
mod state;

#[cfg(feature = "standalone")]
/// Errors which my occure during standalone operations
pub mod standalone_error;

use std::pin::Pin;
use std::sync::Arc;

use axum::Router;
use macworp_core::routing::{IsRouting, UtilitiesRouting};
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
    pub async fn start_inegration(
        tcp_listener: TcpListener,
        shutdown_signal: Pin<Box<dyn Future<Output = ()> + Send + 'static>>,
        config: Configuration,
    ) -> Result<(), Error> {
        use macworp_database_migration::{Migrator, MigratorTrait};

        let state = State::new_for_integration(config).await?;

        Migrator::up(state.db(), None)
            .await
            .map_err(Error::DatabaseMigration)?;

        Self::internal_start(tcp_listener, shutdown_signal, state).await
    }
}
