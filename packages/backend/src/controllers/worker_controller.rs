use std::{marker::PhantomData, sync::Arc};

use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::post,
};
use macworp_core::{
    entities::{LogStream, RunStatus},
    requests::{
        WorkerAnnounceRequest, WorkerClaimRequest, WorkerFinishRequest, WorkerLogsRequest,
        WorkerOutcome, WorkerProgressRequest,
    },
    responses::{WorkerAnnounceResponse, WorkerClaimResponse, WorkerHeartbeatResponse},
    routing::WorkerRouting,
};
use sea_orm::EntityTrait;
use thiserror::Error;

use crate::{
    authentication::middleware::{RequireRunToken, RequireWorker},
    filesystems::is_filesystem::IsFilesystem,
    models::{ProjectTable, RunError, RunLogTable, RunTable},
    state::State as AppState,
};

/// Errors of the worker endpoints
#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    Authentication(#[from] crate::authentication::error::Error),
    /// The run was closed meanwhile (finished, cancelled, deleted)
    #[error("Run already finished")]
    AlreadyFinished,
    /// The shared storage marker could not be read or created
    #[error("Shared storage marker unavailable")]
    Storage,
    #[error("Database error: {0}")]
    Database(#[from] sea_orm::DbErr),
}

impl From<RunError> for Error {
    fn from(error: RunError) -> Self {
        match error {
            RunError::Database(e) => Self::Database(e),
            RunError::InvalidToken => {
                Self::Authentication(crate::authentication::error::Error::Unauthorized)
            }
            _ => Self::AlreadyFinished,
        }
    }
}

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        match self {
            Error::Authentication(e) => e.into_response(),
            Error::AlreadyFinished => {
                tracing::warn!("{self}");
                (StatusCode::CONFLICT, self.to_string()).into_response()
            }
            Error::Storage | Error::Database(_) => {
                tracing::error!("{self}");
                StatusCode::INTERNAL_SERVER_ERROR.into_response()
            }
        }
    }
}

/// Endpoints remote workers poll: claim a run, keep its lease alive, stream logs and progress,
/// report the result. Generic over the filesystem to resolve the project directory.
pub struct WorkerController<F: IsFilesystem> {
    filesystem: PhantomData<F>,
}

impl<F: IsFilesystem + 'static> WorkerController<F> {
    pub fn routes(_state: Arc<AppState>) -> Router<Arc<AppState>> {
        Router::new()
            .route(WorkerRouting::ANNOUNCE_PATH, post(Self::announce))
            .route(WorkerRouting::CLAIM_PATH, post(Self::claim))
            .route(WorkerRouting::HEARTBEAT_PATH, post(Self::heartbeat))
            .route(WorkerRouting::LOGS_PATH, post(Self::logs))
            .route(WorkerRouting::PROGRESS_PATH, post(Self::progress))
            .route(WorkerRouting::FINISH_PATH, post(Self::finish))
    }

    /// Records which engines the worker can run and that it is alive. Workers call it at start
    /// and well within the returned ttl.
    async fn announce(
        State(state): State<Arc<AppState>>,
        RequireWorker { username }: RequireWorker,
        Json(request): Json<WorkerAnnounceRequest>,
    ) -> Result<Json<WorkerAnnounceResponse>, Error> {
        let worker = format!("{username}/{}", request.worker_name);
        tracing::debug!(
            "Worker {worker} announced {} {:?}: {:?}",
            request.runtime,
            request.runtime_version,
            request.engines
        );
        state
            .workers()
            .announce(&worker, request.engines, std::time::Instant::now());
        let storage = F::storage_probe(&state).await.map_err(|e| {
            tracing::error!("Cannot prepare the storage marker: {e}");
            Error::Storage
        })?;
        Ok(Json(WorkerAnnounceResponse {
            ttl_seconds: state.config().worker_lease_seconds(),
            storage,
        }))
    }

    /// Leases the oldest queued run of an engine the worker offers. Runs whose project is gone or has no directory are failed
    /// and skipped. 204 if nothing is queued.
    async fn claim(
        State(state): State<Arc<AppState>>,
        RequireWorker { username }: RequireWorker,
        Json(request): Json<WorkerClaimRequest>,
    ) -> Result<Response, Error> {
        let worker = format!("{username}/{}", request.worker_name);
        let lease_seconds = i64::from(state.config().worker_lease_seconds());
        loop {
            let Some((run, token)) =
                RunTable::claim_next_leased(state.db(), &worker, lease_seconds, &request.engines)
                    .await?
            else {
                return Ok(StatusCode::NO_CONTENT.into_response());
            };
            let project_dir = match ProjectTable::find_by_id(run.project_id)
                .one(state.db())
                .await?
            {
                Some(project) => F::project_path(&state, &project)
                    .ok()
                    .and_then(|path| std::path::absolute(path).ok()),
                None => None,
            };
            let Some(project_dir) = project_dir else {
                RunTable::finish(
                    state.db(),
                    run.id,
                    RunStatus::Failed,
                    None,
                    Some("The project has no directory or was deleted".into()),
                )
                .await
                .or_else(ignore_closed)?;
                continue;
            };
            return Ok(Json(WorkerClaimResponse {
                run_id: run.id,
                project_dir,
                configuration: run.configuration,
                token,
                lease_seconds: lease_seconds as u32,
            })
            .into_response());
        }
    }

    /// Extends the lease and tells the worker whether to cancel
    async fn heartbeat(
        State(state): State<Arc<AppState>>,
        RequireRunToken(run): RequireRunToken,
    ) -> Result<Json<WorkerHeartbeatResponse>, Error> {
        RunTable::extend_lease(
            state.db(),
            run.id,
            i64::from(state.config().worker_lease_seconds()),
        )
        .await?;
        Ok(Json(WorkerHeartbeatResponse {
            cancel_requested: run.cancel_requested,
        }))
    }

    /// Appends log lines
    async fn logs(
        State(state): State<Arc<AppState>>,
        RequireRunToken(run): RequireRunToken,
        Json(request): Json<WorkerLogsRequest>,
    ) -> Result<StatusCode, Error> {
        RunLogTable::append(
            state.db(),
            run.id,
            request.stream,
            &request.lines,
            state.config().max_log_bytes_per_run(),
        )
        .await?;
        Ok(StatusCode::NO_CONTENT)
    }

    /// Stores progress; an engine error report also lands in the log
    async fn progress(
        State(state): State<Arc<AppState>>,
        RequireRunToken(run): RequireRunToken,
        Json(request): Json<WorkerProgressRequest>,
    ) -> Result<StatusCode, Error> {
        RunTable::set_progress(
            state.db(),
            run.id,
            request.total,
            request.done,
            request.message,
        )
        .await?;
        if let Some(error) = request.error {
            RunLogTable::append(
                state.db(),
                run.id,
                LogStream::Engine,
                &[error],
                state.config().max_log_bytes_per_run(),
            )
            .await?;
        }
        Ok(StatusCode::NO_CONTENT)
    }

    /// Stores the result; the token is useless afterwards because the run is no longer running
    async fn finish(
        State(state): State<Arc<AppState>>,
        RequireRunToken(run): RequireRunToken,
        Json(request): Json<WorkerFinishRequest>,
    ) -> Result<StatusCode, Error> {
        let (status, exit_code, error) = match request.outcome {
            WorkerOutcome::Succeeded => (RunStatus::Succeeded, Some(0), None),
            WorkerOutcome::Failed { exit_code, error } => (RunStatus::Failed, exit_code, error),
            WorkerOutcome::Cancelled => (RunStatus::Cancelled, None, None),
        };
        RunTable::finish(state.db(), run.id, status, exit_code, error).await?;
        Ok(StatusCode::NO_CONTENT)
    }
}

/// A run closed in the meantime is fine when failing it, anything else is an error
fn ignore_closed(error: RunError) -> Result<(), RunError> {
    match error {
        RunError::AlreadyFinished | RunError::NotFound => Ok(()),
        other => Err(other),
    }
}
