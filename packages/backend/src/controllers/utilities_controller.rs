use std::sync::Arc;

use axum::{Router, routing::get};
use macworp_core::routing::UtilitiesRouting;

use crate::state::State;

pub struct UtilitiesController;

impl UtilitiesController {
    pub fn routes() -> Router<Arc<State>> {
        Router::new()
            .route(UtilitiesRouting::VERSION_PATH, get(Self::version))
            .route(UtilitiesRouting::HEALTHCHECK_PATH, get(Self::healthcheck))
    }

    pub async fn version() -> String {
        env!("CARGO_PKG_VERSION").to_string()
    }

    pub async fn healthcheck() -> String {
        "OK".to_string()
    }
}
