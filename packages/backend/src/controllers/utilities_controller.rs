use std::sync::Arc;

use axum::{Router, routing::get};
use sqlx::Database;

use crate::state::State;

pub struct UtilitiesController;

impl UtilitiesController {
    pub fn path() -> &'static str {
        "/utilities"
    }

    pub fn routes<T: Database>() -> Router<Arc<State<T>>> {
        Router::new()
            .route("/version", get(Self::version))
            .route("/healthcheck", get(Self::healthcheck))
    }

    pub async fn version() -> String {
        env!("CARGO_PKG_VERSION").to_string()
    }

    pub async fn healthcheck() -> String {
        "OK".to_string()
    }
}
