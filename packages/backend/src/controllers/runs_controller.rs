use std::{collections::HashMap, sync::Arc};

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
};
use macworp_core::{
    entities::RunStatus,
    requests::WorkflowStartRequest,
    responses::RunResponse,
    routing::RunsRouting,
    workflow_configuration::render::{apply_values, render_args},
};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, QueryFilter, QueryOrder,
    TransactionError, TransactionTrait,
};
use thiserror::Error;

use crate::{
    authentication::middleware::RequireAuth,
    models::{
        ActiveRun, Project, ProjectTable, Run, RunColumn, RunTable, User,
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
    /// The user does not own the project or workflow, 403
    #[error("Access is forbidden")]
    Forbidden,
    /// Submitted parameter values do not fit the workflow, or it cannot be rendered, 400
    #[error("Invalid parameters: {0}")]
    InvalidParameters(#[from] macworp_core::workflow_configuration::render::RenderError),
    /// The project's run limit is reached and no run can be evicted, 409
    #[error("Run limit of {0} reached, all runs are locked or still active")]
    RunLimitReached(u64),
    /// The run is already finished and cannot be cancelled, 409
    #[error("Run already finished")]
    AlreadyFinished,
}

impl From<crate::models::RunError> for Error {
    fn from(err: crate::models::RunError) -> Self {
        use crate::models::RunError;
        match err {
            RunError::RunLimitReached(limit) => Self::RunLimitReached(limit),
            RunError::NotFound => Self::RunNotFound,
            RunError::AlreadyFinished => Self::AlreadyFinished,
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
            Error::RunLimitReached(_) | Error::AlreadyFinished => {
                (StatusCode::CONFLICT, self.to_string()).into_response()
            }
            Error::Database(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
        }
    }
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
            .with_state(state)
    }

    /// Loads a project and checks the user owns it
    // TODO P2: replace the owner checks by `Authorized<R, Role>`
    async fn project_of(state: &AppState, user: &User, project_id: i64) -> Result<Project, Error> {
        let project = ProjectTable::find_by_id(project_id)
            .one(state.db())
            .await?
            .ok_or(Error::ProjectNotFound)?;
        if project.owner_id != user.id {
            return Err(Error::Forbidden);
        }
        Ok(project)
    }

    /// Loads a run and checks the user may access its project
    async fn run_of(state: &AppState, user: &User, run_id: i64) -> Result<Run, Error> {
        let run = RunTable::find_by_id(run_id)
            .one(state.db())
            .await?
            .ok_or(Error::RunNotFound)?;
        Self::project_of(state, user, run.project_id).await?;
        Ok(run)
    }

    /// Names of the workflows the runs reference, by workflow id. Deleted workflows are missing.
    async fn workflow_names(state: &AppState, runs: &[Run]) -> Result<HashMap<i64, String>, Error> {
        let ids: Vec<i64> = runs.iter().filter_map(|r| r.workflow_id).collect();
        if ids.is_empty() {
            return Ok(HashMap::new());
        }
        Ok(WorkflowIntegrationTable::find()
            .filter(WorkflowIntegrationColumn::Id.is_in(ids))
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
        RequireAuth(user): RequireAuth,
        Path(project_id): Path<i64>,
        Json(payload): Json<WorkflowStartRequest>,
    ) -> Result<Json<RunResponse>, Error> {
        let project = Self::project_of(&state, &user, project_id).await?;

        let workflow = WorkflowIntegrationTable::find_by_id(payload.workflow_id)
            .one(state.db())
            .await?
            .ok_or(Error::WorkflowNotFound)?;
        if workflow.owner_id != user.id {
            return Err(Error::Forbidden);
        }

        // Only dynamic parameter values come from the client, everything else from the stored workflow
        let mut configuration = workflow.configuration.clone();
        apply_values(&mut configuration, &payload.parameters)?;
        // Fail now instead of in the worker
        render_args(&configuration)?;

        let parameters: serde_json::Map<String, serde_json::Value> = payload
            .parameters
            .into_iter()
            .map(|p| (p.name, p.value))
            .collect();

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
                        started_by: Set(user_id),
                        configuration: Set(configuration),
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
            })?;

        Self::respond(&state, run).await
    }

    /// Runs of a project, newest first
    async fn index(
        State(state): State<Arc<AppState>>,
        RequireAuth(user): RequireAuth,
        Path(project_id): Path<i64>,
    ) -> Result<Json<Vec<RunResponse>>, Error> {
        Self::project_of(&state, &user, project_id).await?;
        let runs = RunTable::find()
            .filter(RunColumn::ProjectId.eq(project_id))
            .order_by_desc(RunColumn::CreatedAt)
            .order_by_desc(RunColumn::Id)
            .all(state.db())
            .await?;
        let names = Self::workflow_names(&state, &runs).await?;
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
        RequireAuth(user): RequireAuth,
        Path(id): Path<i64>,
    ) -> Result<Json<RunResponse>, Error> {
        let run = Self::run_of(&state, &user, id).await?;
        Self::respond(&state, run).await
    }

    /// Cancels a queued run or flags a running run for its worker to stop, see [`Run::request_cancel`]
    async fn cancel(
        State(state): State<Arc<AppState>>,
        RequireAuth(user): RequireAuth,
        Path(id): Path<i64>,
    ) -> Result<Json<RunResponse>, Error> {
        let run = Self::run_of(&state, &user, id).await?;
        let run = run.request_cancel(state.db()).await?;
        Self::respond(&state, run).await
    }

    /// Builds the response for a run, looking up its workflow name
    async fn respond(state: &AppState, run: Run) -> Result<Json<RunResponse>, Error> {
        let name = Self::workflow_names(state, std::slice::from_ref(&run))
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
