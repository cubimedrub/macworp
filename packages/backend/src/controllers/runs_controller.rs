use std::{collections::HashMap, sync::Arc};

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
};
use macworp_core::{
    entities::{Role, RunStatus},
    requests::{ParameterValue, WorkflowStartRequest},
    responses::{LastRunResponse, RunLogsResponse, RunResponse},
    routing::RunsRouting,
    workflow_configuration::render::{apply_values, render_args, render_parameter_file},
};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, QueryFilter, QueryOrder,
    TransactionError, TransactionTrait,
};
use serde::Deserialize;
use thiserror::Error;

use crate::{
    authentication::middleware::RequireUser,
    authorization::{
        Authorized, Editor, Error as AuthorizationError, MinRole, Resource, Viewer, authorize,
    },
    models::{
        ActiveRun, Project, Run, RunColumn, RunLogTable, RunTable, User, WorkflowIntegration,
        WorkflowIntegrationColumn, WorkflowIntegrationTable,
    },
    state::State as AppState,
};

/// Errors of the runs endpoints, mapped to HTTP status codes by the `IntoResponse` impl
#[derive(Error, Debug)]
pub enum Error {
    /// Database failure, 500
    #[error("{0}")]
    Database(#[from] sea_orm::DbErr),
    /// Missing or invalid credentials, response decided by the authentication error
    #[error("{0}")]
    Authentication(#[from] crate::authentication::error::Error),
    /// No project with the given id, 404
    #[error("Project not found")]
    ProjectNotFound,
    /// No workflow with the given id, 404
    #[error("Workflow not found")]
    WorkflowNotFound,
    /// No run with the given id, 404
    #[error("Run not found")]
    RunNotFound,
    /// The user lacks the role on the project, 403
    #[error("Access is forbidden")]
    Forbidden,
    /// Submitted parameter values do not fit the workflow, or it cannot be rendered, 400
    #[error("Invalid parameters: {0}")]
    InvalidParameters(#[from] macworp_core::workflow_configuration::render::Error),
    /// The container runtime of this instance cannot run the workflow's engine, 409
    #[error("Workflow engine {0} is not available with the detected container runtime")]
    EngineUnavailable(macworp_core::workflow_configuration::engine::Engine),
    /// The remote workflow source is invalid, or cannot be fetched and is not cached, 400/502
    #[error("Workflow source unavailable: {0}")]
    Source(#[from] crate::workflow_repository_cache::Error),
    /// The cache task panicked or was cancelled, 500
    #[error("Source resolution failed")]
    SourceTask,
    /// A local workflow has no uploaded files (or not its main script) yet, 409
    #[error("The workflow has no uploaded files, upload them first")]
    WorkflowNotUploaded,
    /// The project's run limit is reached and no run can be evicted, 409
    #[error("Run limit of {0} reached, all runs are locked or still active")]
    RunLimitReached(u64),
    /// The run is already finished and cannot be cancelled, 409
    #[error("Run already finished")]
    AlreadyFinished,
    /// The snapshot of the reference data could not be made, 500
    #[error("{0}")]
    ReferenceSnapshot(#[from] crate::reference_snapshots::Error),
    /// The run to resume is not resumable (not the newest run of the project, still active,
    /// succeeded, or of another workflow), 409. Holds the reason.
    #[error("Cannot resume this run: {0}")]
    NotResumable(&'static str),
    /// The run to resume lost its snapshot of the reference data, 409. Start a fresh run.
    #[error("The reference data of this run is no longer kept, start a new run instead")]
    ReferenceSnapshotGone,
}

impl From<AuthorizationError> for Error {
    fn from(err: AuthorizationError) -> Self {
        match err {
            AuthorizationError::Authentication(e) => Self::Authentication(e),
            AuthorizationError::Database(e) => Self::Database(e),
            AuthorizationError::InvalidPath | AuthorizationError::NotFound => Self::ProjectNotFound,
            AuthorizationError::Forbidden => Self::Forbidden,
        }
    }
}

impl From<crate::models::RunError> for Error {
    fn from(err: crate::models::RunError) -> Self {
        use crate::models::RunError;
        match err {
            RunError::RunLimitReached(limit) => Self::RunLimitReached(limit),
            RunError::NotFound => Self::RunNotFound,
            RunError::AlreadyFinished => Self::AlreadyFinished,
            RunError::InvalidToken => Self::Forbidden,
            RunError::Database(e) => Self::Database(e),
        }
    }
}

impl IntoResponse for Error {
    fn into_response(self) -> axum::response::Response {
        tracing::error!("{self}");
        match self {
            Error::Authentication(e) => e.into_response(),
            Error::Forbidden => (StatusCode::FORBIDDEN, self.to_string()).into_response(),
            Error::ProjectNotFound | Error::WorkflowNotFound | Error::RunNotFound => {
                (StatusCode::NOT_FOUND, self.to_string()).into_response()
            }
            Error::InvalidParameters(_) => {
                (StatusCode::BAD_REQUEST, self.to_string()).into_response()
            }
            Error::RunLimitReached(_)
            | Error::WorkflowNotUploaded
            | Error::AlreadyFinished
            | Error::NotResumable(_)
            | Error::ReferenceSnapshotGone
            | Error::EngineUnavailable(_) => {
                (StatusCode::CONFLICT, self.to_string()).into_response()
            }
            Error::Source(crate::workflow_repository_cache::Error::Fetch { .. }) => {
                (StatusCode::BAD_GATEWAY, self.to_string()).into_response()
            }
            Error::Source(_) => (StatusCode::BAD_REQUEST, self.to_string()).into_response(),
            Error::SourceTask | Error::Database(_) | Error::ReferenceSnapshot(_) => {
                StatusCode::INTERNAL_SERVER_ERROR.into_response()
            }
        }
    }
}

/// Page size cap of the log endpoint
const MAX_LOG_CHUNKS_PER_REQUEST: u64 = 200;

/// Query of the log endpoint
#[derive(Debug, Deserialize)]
struct LogsQuery {
    /// Return chunks with a larger `seq`
    after: Option<i64>,
    limit: Option<u64>,
}

/// Query of the last run endpoint
#[derive(Debug, Deserialize)]
struct LastQuery {
    /// Only consider runs of this workflow
    workflow_id: Option<i64>,
}

/// Endpoints to start, list, inspect and cancel runs of a project
pub struct RunsController;

impl RunsController {
    /// Routes of the controller, nested under [`RunsRouting`]'s controller path
    pub fn routes(state: Arc<AppState>) -> Router<Arc<AppState>> {
        Router::new()
            .route(RunsRouting::START_PATH, post(Self::start))
            .route(RunsRouting::INDEX_PATH, get(Self::index))
            .route(RunsRouting::GET_PATH, get(Self::get))
            .route(RunsRouting::CANCEL_PATH, post(Self::cancel))
            .route(RunsRouting::LOGS_PATH, get(Self::logs))
            .route(RunsRouting::LAST_PATH, get(Self::last))
            .with_state(state)
    }

    /// Log chunks of a run after the cursor `after` (a chunk `seq`, none = from the start), plus
    /// the run itself for status and progress. At most [`MAX_LOG_CHUNKS_PER_REQUEST`] chunks.
    async fn logs(
        State(state): State<Arc<AppState>>,
        RequireUser(user): RequireUser,
        Path(id): Path<i64>,
        Query(query): Query<LogsQuery>,
    ) -> Result<Json<RunLogsResponse>, Error> {
        let user_id = user.id;
        let run = Self::run_of::<Viewer>(&state, user, id).await?;
        let limit = query
            .limit
            .unwrap_or(MAX_LOG_CHUNKS_PER_REQUEST)
            .clamp(1, MAX_LOG_CHUNKS_PER_REQUEST);
        let chunks = RunLogTable::list_after(state.db(), id, query.after, limit)
            .await?
            .into_iter()
            .map(|chunk| chunk.into_response())
            .collect();
        let Json(run) = Self::respond(&state, user_id, run).await?;
        Ok(Json(RunLogsResponse { chunks, run }))
    }

    /// Workflow and parameter values of the newest run of the project, optionally restricted to
    /// the query parameter `workflow_id`. 404 if there is none.
    async fn last(
        State(state): State<Arc<AppState>>,
        Authorized { model: project, .. }: Authorized<Project, Viewer>,
        Query(query): Query<LastQuery>,
    ) -> Result<Json<LastRunResponse>, Error> {
        let project_id = project.id;
        let run = RunTable::latest_for_project(state.db(), project_id, query.workflow_id)
            .await?
            .ok_or(Error::RunNotFound)?;
        let parameters = match run.parameters {
            serde_json::Value::Object(map) => map
                .into_iter()
                .map(|(name, value)| ParameterValue { name, value })
                .collect(),
            _ => Vec::new(),
        };
        Ok(Json(LastRunResponse {
            // The query excludes runs without a workflow
            workflow_id: run.workflow_id.ok_or(Error::RunNotFound)?,
            parameters,
        }))
    }

    /// Loads a run and checks the user holds at least the role `M` on its project
    async fn run_of<M: MinRole>(state: &AppState, user: User, run_id: i64) -> Result<Run, Error> {
        let run = RunTable::find_by_id(run_id)
            .one(state.db())
            .await?
            .ok_or(Error::RunNotFound)?;
        authorize::<Project, M>(state, user, run.project_id).await?;
        Ok(run)
    }

    /// Names of the workflows the runs reference, by workflow id. Deleted workflows are missing.
    /// Workflows the user cannot see are missing as well.
    async fn workflow_names(
        state: &AppState,
        user_id: i64,
        runs: &[Run],
    ) -> Result<HashMap<i64, String>, Error> {
        let ids: Vec<i64> = runs.iter().filter_map(|r| r.workflow_id).collect();
        if ids.is_empty() {
            return Ok(HashMap::new());
        }
        Ok(WorkflowIntegrationTable::find()
            .filter(WorkflowIntegrationColumn::Id.is_in(ids))
            .filter(WorkflowIntegration::visible_to(user_id, Role::Viewer))
            .all(state.db())
            .await?
            .into_iter()
            .map(|w| (w.id, w.name))
            .collect())
    }

    /// Queues a run of a stored workflow on a project.
    ///
    /// Only dynamic parameter values come from the request, they are validated against the
    /// stored configuration, which is then snapshotted into the run. Makes room under the
    /// project's run limit in the same transaction.
    async fn start(
        State(state): State<Arc<AppState>>,
        Authorized {
            user,
            model: project,
            ..
        }: Authorized<Project, Editor>,
        Json(payload): Json<WorkflowStartRequest>,
    ) -> Result<Json<RunResponse>, Error> {
        // Running needs to see the workflow, which is independent of the role on the project
        let workflow = match authorize::<WorkflowIntegration, Viewer>(
            &state,
            user.clone(),
            payload.workflow_id,
        )
        .await
        {
            Ok(authorized) => authorized.model,
            Err(AuthorizationError::NotFound) => return Err(Error::WorkflowNotFound),
            Err(e) => return Err(e.into()),
        };

        let engine = workflow.configuration.engine();
        if state.unavailable_engines().contains(engine) {
            return Err(Error::EngineUnavailable(engine.clone()));
        }

        let resumed = match payload.resume_from_run {
            Some(run_id) => {
                Some(Self::resumable_run(&state, project.id, workflow.id, run_id).await?)
            }
            None => None,
        };

        // Only dynamic parameter values come from the client, everything else from the stored workflow
        let mut configuration = workflow.configuration.clone();
        state.confine_local_source(workflow.id, &mut configuration);
        // Uploaded files are pinned like remote checkouts: later uploads do not change this run
        state.pin_local_source(&mut configuration);
        // Before the resume parameter is added: that one is managed, not allowed by hand
        configuration
            .validate_engine_parameters()
            .map_err(macworp_core::workflow_configuration::render::Error::from)?;
        configuration
            .validate_images()
            .map_err(macworp_core::workflow_configuration::render::Error::from)?;
        let switches: Vec<(String, bool)> = payload
            .engine_switches
            .iter()
            .map(|switch| (switch.name.clone(), switch.enabled))
            .collect();
        configuration
            .apply_engine_switches(&switches)
            .map_err(macworp_core::workflow_configuration::render::Error::from)?;
        apply_values(&mut configuration, &payload.parameters)?;
        if resumed.is_some() {
            configuration.enable_resume();
        }
        // Remote sources are pinned to the commit they resolve to now: later fetches or moved
        // tags cannot change this run, and the engine reads the checkout without network.
        let cache = state.workflow_repository_cache();
        let (configuration, provenance) = tokio::task::spawn_blocking(move || {
            let mut configuration = configuration;
            cache
                .localize(&mut configuration)
                .map(|provenance| (configuration, provenance))
        })
        .await
        .map_err(|_| Error::SourceTask)??;
        let mut configuration = configuration;
        if let macworp_core::workflow_configuration::source::Source::Local(local) =
            configuration.source_mut()
        {
            let location = state.config().resolve_storage_path(local.location());
            if !state.config().allow_local_paths() && !location.join(local.script()).is_file() {
                return Err(Error::WorkflowNotUploaded);
            }
            // Below the storage root the snapshot keeps a relative path, each worker resolves
            // it against its own mount of the storage.
            *local.location_mut() = state.config().relative_to_storage(&location);
        }
        if let Some(provenance) = &provenance {
            tracing::info!(sha = %provenance.commit, "pinned remote workflow source");
        }
        // Fail now instead of in the worker
        let reference_directory = state.reference_directory().get();
        render_args(&configuration, Some(&reference_directory))?;
        render_parameter_file(&configuration, Some(&reference_directory), None)?;

        let parameters: serde_json::Map<String, serde_json::Value> = payload
            .parameters
            .into_iter()
            .map(|p| (p.name, p.value))
            .collect();

        // Point-in-time view of the reference data: later changes by admins never reach this run.
        // A resumed run keeps the snapshot of the run it continues, the engine's cache refers to
        // its paths. `created_snapshot` is only set for a new one, which must be removed again if
        // the run cannot be stored.
        let (reference_snapshot_name, reference_fingerprint, created_snapshot) = match &resumed {
            Some(previous) => (
                previous.reference_snapshot.clone(),
                previous.reference_fingerprint.clone(),
                None,
            ),
            None => match state.reference_snapshots().cloned() {
                Some(snapshots) => {
                    let snapshot = tokio::task::spawn_blocking(move || snapshots.create())
                        .await
                        .map_err(|_| Error::SourceTask)??;
                    (
                        Some(snapshot.name.clone()),
                        Some(snapshot.fingerprint),
                        Some(snapshot.name),
                    )
                }
                None => (None, None, None),
            },
        };

        let max_runs = state.config().max_runs_per_project();
        let comment = payload.comment;
        let user_id = user.id;
        let run = state
            .db()
            .transaction::<_, Run, Error>(|txn| {
                Box::pin(async move {
                    RunTable::make_room(txn, project.id, max_runs).await?;
                    Ok(ActiveRun {
                        project_id: Set(project.id),
                        workflow_id: Set(Some(workflow.id)),
                        started_by: Set(Some(user_id)),
                        configuration: Set(configuration),
                        provenance: Set(provenance),
                        reference_snapshot: Set(reference_snapshot_name),
                        reference_fingerprint: Set(reference_fingerprint),
                        parameters: Set(serde_json::Value::Object(parameters)),
                        comment: Set(comment),
                        status: Set(RunStatus::Queued),
                        cancel_requested: Set(false),
                        locked: Set(false),
                        created_at: Set(chrono::Utc::now()),
                        ..Default::default()
                    }
                    .insert(txn)
                    .await?)
                })
            })
            .await
            .map_err(|err| match err {
                TransactionError::Connection(err) => Error::Database(err),
                TransactionError::Transaction(err) => err,
            });
        let run = match run {
            Ok(run) => run,
            Err(error) => {
                // No run points to the snapshot, do not wait for the sweep
                if let (Some(name), Some(snapshots)) =
                    (created_snapshot, state.reference_snapshots().cloned())
                {
                    let _ = tokio::task::spawn_blocking(move || snapshots.remove(&name)).await;
                }
                return Err(error);
            }
        };

        Self::respond(&state, user_id, run).await
    }

    /// Loads the run a user wants to resume and checks that it can be.
    ///
    /// Only the newest run of the project can be resumed: the engine's cache lives in the
    /// project folder and belongs to the run that used it last, and the reference snapshot of
    /// older runs is released. The run must have failed or been cancelled and belong to the same
    /// workflow, and its snapshot must still exist.
    ///
    /// # Arguments
    /// * `state` - Application state
    /// * `project_id` - Project the new run starts on
    /// * `workflow_id` - Workflow the new run starts, must be the run's workflow
    /// * `run_id` - Run to continue
    async fn resumable_run(
        state: &AppState,
        project_id: i64,
        workflow_id: i64,
        run_id: i64,
    ) -> Result<Run, Error> {
        let run = RunTable::find_by_id(run_id)
            .filter(RunColumn::ProjectId.eq(project_id))
            .one(state.db())
            .await?
            .ok_or(Error::RunNotFound)?;
        let newest = RunTable::newest_of_project(state.db(), project_id).await?;
        if newest.map(|newest| newest.id) != Some(run.id) {
            return Err(Error::NotResumable("a newer run exists in this project"));
        }
        if !run.can_be_resumed() {
            return Err(Error::NotResumable("it did not fail and was not cancelled"));
        }
        if run.workflow_id != Some(workflow_id) {
            return Err(Error::NotResumable("it ran another workflow"));
        }
        if let (Some(snapshots), Some(name)) =
            (state.reference_snapshots(), &run.reference_snapshot)
            && !snapshots.path_of(name).is_dir()
        {
            return Err(Error::ReferenceSnapshotGone);
        }
        Ok(run)
    }

    /// Runs of a project, newest first
    async fn index(
        State(state): State<Arc<AppState>>,
        Authorized {
            user,
            model: project,
            ..
        }: Authorized<Project, Viewer>,
    ) -> Result<Json<Vec<RunResponse>>, Error> {
        let project_id = project.id;
        let runs = RunTable::find()
            .filter(RunColumn::ProjectId.eq(project_id))
            .order_by_desc(RunColumn::CreatedAt)
            .order_by_desc(RunColumn::Id)
            .all(state.db())
            .await?;
        let names = Self::workflow_names(&state, user.id, &runs).await?;
        Ok(Json(
            runs.into_iter()
                .map(|run| {
                    let name = names
                        .get(&workflow_id_of(&run))
                        .cloned()
                        .unwrap_or_default();
                    run.into_response(name)
                })
                .collect(),
        ))
    }

    /// A single run
    async fn get(
        State(state): State<Arc<AppState>>,
        RequireUser(user): RequireUser,
        Path(id): Path<i64>,
    ) -> Result<Json<RunResponse>, Error> {
        let user_id = user.id;
        let run = Self::run_of::<Viewer>(&state, user, id).await?;
        Self::respond(&state, user_id, run).await
    }

    /// Cancels a queued run or flags a running run for its worker to stop, see [`Run::request_cancel`]
    async fn cancel(
        State(state): State<Arc<AppState>>,
        RequireUser(user): RequireUser,
        Path(id): Path<i64>,
    ) -> Result<Json<RunResponse>, Error> {
        let user_id = user.id;
        let run = Self::run_of::<Editor>(&state, user, id).await?;
        let run = run.request_cancel(state.db()).await?;
        Self::respond(&state, user_id, run).await
    }

    /// Builds the response for a run, looking up its workflow name
    async fn respond(state: &AppState, user_id: i64, run: Run) -> Result<Json<RunResponse>, Error> {
        let name = Self::workflow_names(state, user_id, std::slice::from_ref(&run))
            .await?
            .remove(&workflow_id_of(&run))
            .unwrap_or_default();
        Ok(Json(run.into_response(name)))
    }
}

/// Workflow id of a run, `-1` (matches nothing) if the workflow was deleted
fn workflow_id_of(run: &Run) -> i64 {
    run.workflow_id.unwrap_or(-1)
}
