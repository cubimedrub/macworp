use std::sync::Arc;

use axum::{
    Json, Router,
    extract::State as AxumState,
    http::StatusCode,
    routing::{get, post},
};
use macworp_core::{
    routing::RuntimeRouting,
    runtime_status::{AdjustMemoryRequest, RuntimeStatus, SleepStatus},
};

use crate::{authentication::middleware::RequireUser, models::RunTable, state::State};

/// The container runtime of the machine the backend runs on, for the desktop app. Answers 404 when
/// the embedding application provides no [`crate::runtime_control::RuntimeControl`].
pub struct RuntimeController;

impl RuntimeController {
    pub fn routes(state: Arc<State>) -> Router<Arc<State>> {
        Router::new()
            .route(RuntimeRouting::STATUS_PATH, get(Self::status))
            .route(RuntimeRouting::REFRESH_PATH, post(Self::refresh))
            .route(RuntimeRouting::START_PATH, post(Self::start))
            .route(RuntimeRouting::MEMORY_PATH, post(Self::adjust_memory))
            .route(RuntimeRouting::SLEEP_PATH, get(Self::sleep))
            .with_state(state)
    }

    /// The status as last checked.
    async fn status(
        AxumState(state): AxumState<Arc<State>>,
        _user: RequireUser,
    ) -> Result<Json<RuntimeStatus>, (StatusCode, String)> {
        Ok(Json(Self::control(&state)?.status().await))
    }

    /// Whether the computer is kept awake for the running workflows.
    async fn sleep(
        AxumState(state): AxumState<Arc<State>>,
        _user: RequireUser,
    ) -> Result<Json<SleepStatus>, (StatusCode, String)> {
        Ok(Json(Self::control(&state)?.sleep_status()))
    }

    /// Checks the runtime again.
    async fn refresh(
        AxumState(state): AxumState<Arc<State>>,
        _user: RequireUser,
    ) -> Result<Json<RuntimeStatus>, (StatusCode, String)> {
        Ok(Json(Self::control(&state)?.refresh().await))
    }

    /// Starts the stopped container engine.
    async fn start(
        AxumState(state): AxumState<Arc<State>>,
        _user: RequireUser,
    ) -> Result<Json<RuntimeStatus>, (StatusCode, String)> {
        Self::control(&state)?
            .start()
            .await
            .map(Json)
            .map_err(|message| (StatusCode::BAD_GATEWAY, message))
    }

    /// Changes the memory of the container VM. Refused (409) while runs are queued or running:
    /// the restart would stop them.
    async fn adjust_memory(
        AxumState(state): AxumState<Arc<State>>,
        _user: RequireUser,
        Json(request): Json<AdjustMemoryRequest>,
    ) -> Result<Json<RuntimeStatus>, (StatusCode, String)> {
        let control = Self::control(&state)?;
        let active = RunTable::active_count(state.db())
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        if active > 0 {
            return Err((
                StatusCode::CONFLICT,
                "Workflows are running. Wait until they are finished or stop them, then try again."
                    .to_string(),
            ));
        }
        control
            .adjust_memory(request.mebibytes)
            .await
            .map(Json)
            .map_err(|message| (StatusCode::BAD_GATEWAY, message))
    }

    /// The control the embedding application provided.
    ///
    /// # Arguments
    /// * `state` - the application state
    fn control(
        state: &State,
    ) -> Result<&Arc<dyn crate::runtime_control::RuntimeControl>, (StatusCode, String)> {
        state.runtime_control().ok_or((
            StatusCode::NOT_FOUND,
            "This backend does not manage a container runtime".to_string(),
        ))
    }
}
