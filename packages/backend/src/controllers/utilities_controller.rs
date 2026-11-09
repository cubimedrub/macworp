use std::sync::Arc;

use axum::{Json, Router, extract::State as AxumState, routing::get};
use macworp_core::{responses::EnginesResponse, routing::UtilitiesRouting};

use crate::{authentication::middleware::RequireUser, state::State};

pub struct UtilitiesController;

impl UtilitiesController {
    pub fn routes() -> Router<Arc<State>> {
        Router::new()
            .route(UtilitiesRouting::VERSION_PATH, get(Self::version))
            .route(UtilitiesRouting::HEALTHCHECK_PATH, get(Self::healthcheck))
            .route(UtilitiesRouting::ENGINES_PATH, get(Self::engines))
    }

    pub async fn version() -> String {
        env!("CARGO_PKG_VERSION").to_string()
    }

    pub async fn healthcheck() -> String {
        "OK".to_string()
    }

    /// Engines runs are rejected for, and how many workers are online
    pub async fn engines(
        AxumState(state): AxumState<Arc<State>>,
        _user: RequireUser,
    ) -> Json<EnginesResponse> {
        let ttl = state.workers().ttl(state.config());
        Json(EnginesResponse {
            unavailable: state.unavailable_engines(),
            workers_online: state.workers().online(std::time::Instant::now(), ttl),
        })
    }
}
