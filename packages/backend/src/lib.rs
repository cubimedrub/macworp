/// Authentication providers
pub mod auth_providers;
/// Backend configuration
pub mod configuration;
/// Controllers providing the web API endpoints
mod controllers;
/// Middleware, e.g. authentication
mod middleware;
/// Models abstracting the database access
mod models;
/// State of the backend, e.g. connection pool
mod state;

use std::pin::Pin;
use std::sync::Arc;

use axum::Router;
use tokio::net::TcpListener;

use crate::controllers::*;
use crate::state::State;

pub async fn start(
    tcp_listener: TcpListener,
    shutdown_signal: Pin<Box<dyn Future<Output = ()> + Send + 'static>>,
) {
    let state = State::new();
    let state = Arc::new(state);

    let router = Router::new()
        .nest(UtilitiesController::path(), UtilitiesController::routes())
        .with_state(state);

    axum::serve(tcp_listener, router)
        .with_graceful_shutdown(shutdown_signal)
        .await
        .unwrap()
}
