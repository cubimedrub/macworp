use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{Path, State as AxumState},
    http::StatusCode,
    routing::get,
};
use macworp_core::{
    responses::{EnginesResponse, NfCorePipelinesResponse, NfCoreReleasesResponse},
    routing::UtilitiesRouting,
};

use crate::{authentication::middleware::RequireUser, state::State};

pub struct UtilitiesController;

impl UtilitiesController {
    pub fn routes() -> Router<Arc<State>> {
        Router::new()
            .route(UtilitiesRouting::VERSION_PATH, get(Self::version))
            .route(UtilitiesRouting::HEALTHCHECK_PATH, get(Self::healthcheck))
            .route(UtilitiesRouting::ENGINES_PATH, get(Self::engines))
            .route(
                UtilitiesRouting::NF_CORE_PIPELINES_PATH,
                get(Self::nf_core_pipelines),
            )
            .route(
                UtilitiesRouting::NF_CORE_RELEASES_PATH,
                get(Self::nf_core_releases),
            )
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

    /// Pipelines of the public nf-core index; 503 if the index was never fetched and cannot be
    pub async fn nf_core_pipelines(
        AxumState(state): AxumState<Arc<State>>,
        _user: RequireUser,
    ) -> Result<Json<NfCorePipelinesResponse>, (StatusCode, String)> {
        let served = state
            .nf_core_index()
            .get()
            .await
            .map_err(|e| (StatusCode::SERVICE_UNAVAILABLE, e.to_string()))?;
        Ok(Json(NfCorePipelinesResponse {
            pipelines: served
                .snapshot
                .pipelines
                .iter()
                .map(|p| p.summary())
                .collect(),
            fetched_at: served.snapshot.fetched_at,
            stale: served.stale,
        }))
    }

    /// Releases of one nf-core pipeline (newest first); 404 for an unknown pipeline
    pub async fn nf_core_releases(
        AxumState(state): AxumState<Arc<State>>,
        _user: RequireUser,
        Path(name): Path<String>,
    ) -> Result<Json<NfCoreReleasesResponse>, (StatusCode, String)> {
        let served = state
            .nf_core_index()
            .get()
            .await
            .map_err(|e| (StatusCode::SERVICE_UNAVAILABLE, e.to_string()))?;
        let pipeline = served.snapshot.pipeline(&name).ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                format!("Unknown nf-core pipeline {name}"),
            )
        })?;
        Ok(Json(NfCoreReleasesResponse {
            releases: pipeline.releases.clone(),
            fetched_at: served.snapshot.fetched_at,
            stale: served.stale,
        }))
    }
}
