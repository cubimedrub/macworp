use macworp_core::{
    entities::{FailureReason, RunStatus},
    responses::RunResponse,
    workflow_configuration::{WorkflowConfiguration, engine::Engine},
};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DbErr, EntityTrait, QueryFilter, QueryOrder,
    Set, entity::prelude::*,
};
use thiserror::Error;

/// One execution of a workflow on a project.
///
/// Stores snapshots of the workflow configuration and parameters, so a run stays reproducible
/// after the workflow was edited or deleted. The number of runs per project is limited,
/// see [`Entity::make_room`]; deleting a run also deletes its logs.
#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "runs")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    /// Project the workflow ran on, runs are deleted with their project
    pub project_id: i64,
    /// `None` once the workflow was deleted, the run keeps its `configuration` snapshot
    #[sea_orm(nullable)]
    pub workflow_id: Option<i64>,
    /// User who started the run, `None` once that user was deleted
    #[sea_orm(nullable)]
    pub started_by: Option<i64>,
    /// Snapshot of the workflow configuration at start
    pub configuration: WorkflowConfiguration,
    /// Snapshot of the user supplied parameter values
    pub parameters: Json,
    /// Comment given by the user at start
    pub comment: String,
    pub status: RunStatus,
    /// Exit code of the engine, `None` until it exited
    #[sea_orm(nullable)]
    pub exit_code: Option<i32>,
    /// Why the run failed if the reason is not the exit code, e.g. timeout or container runtime missing
    #[sea_orm(nullable)]
    pub error: Option<String>,
    /// Machine-readable cause of a failed run, `None` for every other status
    #[sea_orm(nullable)]
    pub failure_reason: Option<FailureReason>,
    /// Tasks the engine announced so far, grows during the run
    pub tasks_total: i32,
    /// Tasks the engine finished
    pub tasks_done: i32,
    /// Latest progress detail reported by the engine
    #[sea_orm(nullable)]
    pub progress_message: Option<String>,
    /// A user asked to stop the run while it was running, the worker checks this and moves the run to `Cancelled`
    pub cancel_requested: bool,
    /// Locked runs are never evicted by the retention limit
    pub locked: bool,
    /// When the run was queued
    pub created_at: DateTimeUtc,
    /// When a worker started executing, `None` while queued
    #[sea_orm(nullable)]
    pub started_at: Option<DateTimeUtc>,
    /// When the run reached a terminal status
    #[sea_orm(nullable)]
    pub finished_at: Option<DateTimeUtc>,
    /// A leased run is requeued when its worker does not heartbeat before this time. `None` for
    /// runs claimed without a lease (embedded desktop worker) and for runs that are not running.
    #[sea_orm(nullable)]
    pub leased_until: Option<DateTimeUtc>,
    /// Name of the worker holding the lease
    #[sea_orm(nullable)]
    pub worker_name: Option<String>,
    /// SHA-256 (hex) of the per-run token the worker authenticates with, see [`Entity::hash_token`]
    #[sea_orm(nullable)]
    pub worker_token_hash: Option<String>,
    /// How often the run was claimed, a run that exhausts its attempts fails instead of requeueing
    pub attempts: i32,

    #[sea_orm(belongs_to, relation_enum = "Project", from = "project_id", to = "id")]
    pub project: HasOne<super::project::Entity>,
    #[sea_orm(
        belongs_to,
        relation_enum = "Workflow",
        from = "workflow_id",
        to = "id"
    )]
    pub workflow: HasOne<super::workflow_integration::Entity>,
    #[sea_orm(belongs_to, relation_enum = "Starter", from = "started_by", to = "id")]
    pub starter: HasOne<super::user::Entity>,

    #[sea_orm(has_many)]
    pub logs: HasMany<super::run_log::Entity>,
}

impl ActiveModelBehavior for ActiveModel {}

/// Errors of run operations
#[derive(Debug, Error)]
pub enum Error {
    /// No run can be evicted for a new one: all are locked or still active
    #[error("Run limit of {0} reached, all runs are locked or still active")]
    RunLimitReached(u64),
    /// No run with the given id
    #[error("Run not found")]
    NotFound,
    /// The run is already in a terminal status
    #[error("Run already finished")]
    AlreadyFinished,
    /// The run is not running or the worker token does not belong to it
    #[error("Invalid run token")]
    InvalidToken,
    #[error("Database error: {0}")]
    Database(#[from] DbErr),
}

/// Lease parameters of a claim by a remote worker
struct Lease<'a> {
    /// Worker taking the run
    worker: &'a str,
    /// Lease duration in seconds
    seconds: i64,
    /// Hash of the per-run token issued with the claim
    token_hash: String,
}

/// Outcome of [`Entity::requeue_expired`]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Requeued {
    /// Runs moved back to `Queued`
    pub requeued: u64,
    /// Runs failed because no attempts were left
    pub failed: u64,
    /// Runs cancelled because a cancel was pending
    pub cancelled: u64,
}

/// Outcome of [`Entity::expire_timeouts`]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Expired {
    /// Queued runs failed with `QueuedTimeout`
    pub queued: u64,
    /// Running runs failed with `TimedOut`
    pub running: u64,
}

impl Model {
    /// A run can be evicted by the run limit if it is finished and not locked
    pub fn is_evictable(&self) -> bool {
        !self.locked && self.status.is_terminal()
    }

    /// Converts to the API response. `workflow_name` is the current name of the workflow, empty if deleted.
    pub fn into_response(self, workflow_name: String) -> RunResponse {
        RunResponse {
            id: self.id,
            project_id: self.project_id,
            workflow_id: self.workflow_id,
            workflow_name,
            status: self.status,
            exit_code: self.exit_code,
            error: self.error,
            failure_reason: self.failure_reason,
            tasks_total: self.tasks_total,
            tasks_done: self.tasks_done,
            progress_message: self.progress_message,
            comment: self.comment,
            cancel_requested: self.cancel_requested,
            locked: self.locked,
            created_at: self.created_at,
            started_at: self.started_at,
            finished_at: self.finished_at,
        }
    }

    /// Cancels the run: a queued run is cancelled right away, a running run is flagged with
    /// `cancel_requested` for its worker to stop. Finished runs return [`Error::AlreadyFinished`].
    pub async fn request_cancel<C: ConnectionTrait>(&self, db: &C) -> Result<Model, Error> {
        let mut active = ActiveModel {
            id: Set(self.id),
            ..Default::default()
        };
        match self.status {
            RunStatus::Queued => {
                active.status = Set(RunStatus::Cancelled);
                active.finished_at = Set(Some(chrono::Utc::now()));
            }
            RunStatus::Running => active.cancel_requested = Set(true),
            _ => return Err(Error::AlreadyFinished),
        }
        Ok(active.update(db).await?)
    }

    /// Locks or unlocks the run, a locked run is kept when newer runs exceed the project's limit
    pub async fn set_locked<C: ConnectionTrait>(
        &self,
        db: &C,
        locked: bool,
    ) -> Result<Model, Error> {
        ActiveModel {
            id: Set(self.id),
            locked: Set(locked),
            ..Default::default()
        }
        .update(db)
        .await
        .map_err(|e| match e {
            DbErr::RecordNotUpdated => Error::NotFound,
            e => Error::Database(e),
        })
    }
}

impl Entity {
    /// Moves the oldest queued run to `Running` and returns it, `None` if nothing is queued.
    ///
    /// No lease is taken: the claimer is trusted to live as long as the run (embedded desktop
    /// worker, see [`Entity::fail_orphaned_running`]). The status switch is a conditional update,
    /// so concurrent workers never claim the same run.
    pub async fn claim_next<C: ConnectionTrait>(db: &C) -> Result<Option<Model>, Error> {
        Self::claim(db, None, None).await
    }

    /// Like [`Entity::claim_next`], but for a remote worker: the run is leased to `worker` for
    /// `lease_seconds` (extend it with [`Entity::heartbeat`]) and a fresh per-run token is
    /// returned. Only the token's hash is stored, so the plaintext exists in this return value only.
    ///
    /// Only runs whose workflow engine is in `engines` are considered, others stay queued for a
    /// worker that can run them.
    ///
    /// # Arguments
    /// * `db` - database connection
    /// * `worker` - identity of the worker taking the run
    /// * `lease_seconds` - lease duration, extended by heartbeats
    /// * `engines` - engines the worker can run
    pub async fn claim_next_leased<C: ConnectionTrait>(
        db: &C,
        worker: &str,
        lease_seconds: i64,
        engines: &[Engine],
    ) -> Result<Option<(Model, String)>, Error> {
        let token = crate::configuration::generate_secret();
        let lease = Lease {
            worker,
            seconds: lease_seconds,
            token_hash: Self::hash_token(&token),
        };
        Ok(Self::claim(db, Some(lease), Some(engines))
            .await?
            .map(|run| (run, token)))
    }

    /// SHA-256 of a per-run token as lowercase hex. Tokens are random, so a fast hash is enough.
    pub fn hash_token(token: &str) -> String {
        use sha2::{Digest, Sha256};
        Sha256::digest(token.as_bytes())
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }

    /// Shared claim loop of [`Entity::claim_next`] and [`Entity::claim_next_leased`]: takes the
    /// oldest queued run (of an engine in `engines`, any engine for `None`).
    async fn claim<C: ConnectionTrait>(
        db: &C,
        lease: Option<Lease<'_>>,
        engines: Option<&[Engine]>,
    ) -> Result<Option<Model>, Error> {
        loop {
            // The engine lives in the JSON configuration, so it is filtered here, not in SQL
            let queued = Entity::find()
                .filter(Column::Status.eq(RunStatus::Queued))
                .order_by_asc(Column::CreatedAt)
                .order_by_asc(Column::Id)
                .all(db)
                .await?;
            let mut lost_race = false;
            for next in queued {
                if let Some(engines) = engines
                    && !engines.contains(next.configuration.engine())
                {
                    continue;
                }
                let now = chrono::Utc::now();
                let mut update = Entity::update_many()
                    .col_expr(Column::Status, Expr::value(RunStatus::Running))
                    .col_expr(Column::StartedAt, Expr::value(now))
                    .col_expr(
                        Column::Attempts,
                        sea_orm::ExprTrait::add(Expr::col(Column::Attempts), 1),
                    )
                    .filter(Column::Id.eq(next.id))
                    .filter(Column::Status.eq(RunStatus::Queued));
                if let Some(lease) = &lease {
                    update = update
                        .col_expr(
                            Column::LeasedUntil,
                            Expr::value(now + chrono::Duration::seconds(lease.seconds)),
                        )
                        .col_expr(Column::WorkerName, Expr::value(lease.worker))
                        .col_expr(
                            Column::WorkerTokenHash,
                            Expr::value(lease.token_hash.clone()),
                        );
                }
                if update.exec(db).await?.rows_affected == 1 {
                    return Ok(Entity::find_by_id(next.id).one(db).await?);
                }
                // lost the race (or the run was cancelled meanwhile), try the next one
                lost_race = true;
            }
            if !lost_race {
                return Ok(None);
            }
        }
    }

    /// The running run `id` if `token` is the one issued at its claim, otherwise
    /// [`Error::InvalidToken`] (unknown run, wrong token, or the run is no longer running, e.g.
    /// requeued after the lease expired). Authenticates every worker call except claim.
    pub async fn authenticate<C: ConnectionTrait>(
        db: &C,
        id: i64,
        token: &str,
    ) -> Result<Model, Error> {
        let hash = Self::hash_token(token);
        Entity::find_by_id(id)
            .filter(Column::Status.eq(RunStatus::Running))
            .filter(Column::WorkerTokenHash.eq(hash))
            .one(db)
            .await?
            .ok_or(Error::InvalidToken)
    }

    /// Extends the lease of a run by `lease_seconds` from now and returns whether a user asked
    /// to cancel it. Fails with [`Error::InvalidToken`] like [`Entity::authenticate`].
    pub async fn heartbeat<C: ConnectionTrait>(
        db: &C,
        id: i64,
        token: &str,
        lease_seconds: i64,
    ) -> Result<bool, Error> {
        let run = Self::authenticate(db, id, token).await?;
        Self::extend_lease(db, id, lease_seconds).await?;
        Ok(run.cancel_requested)
    }

    /// Moves the lease of a running run to `lease_seconds` from now. For callers that already
    /// authenticated the run; a run that is not running is left untouched.
    pub async fn extend_lease<C: ConnectionTrait>(
        db: &C,
        id: i64,
        lease_seconds: i64,
    ) -> Result<(), Error> {
        Entity::update_many()
            .col_expr(
                Column::LeasedUntil,
                Expr::value(chrono::Utc::now() + chrono::Duration::seconds(lease_seconds)),
            )
            .filter(Column::Id.eq(id))
            .filter(Column::Status.eq(RunStatus::Running))
            .exec(db)
            .await?;
        Ok(())
    }

    /// Handles runs whose lease expired (the worker died or lost the network): a run with attempts
    /// left goes back to `Queued` and its token is invalidated, a run that used `max_attempts`
    /// fails. A run with `cancel_requested` is cancelled instead. Returns what happened.
    pub async fn requeue_expired<C: ConnectionTrait>(
        db: &C,
        max_attempts: i32,
    ) -> Result<Requeued, Error> {
        let now = chrono::Utc::now();
        let expired = Entity::find()
            .filter(Column::Status.eq(RunStatus::Running))
            .filter(Column::LeasedUntil.is_not_null())
            .filter(Column::LeasedUntil.lt(now))
            .all(db)
            .await?;
        let mut result = Requeued::default();
        for run in expired {
            // conditional on the status and the exact lease, so a heartbeat that just arrived wins
            let guarded = Entity::update_many()
                .filter(Column::Id.eq(run.id))
                .filter(Column::Status.eq(RunStatus::Running))
                .filter(Column::LeasedUntil.lt(now));
            if run.cancel_requested {
                let updated = guarded
                    .col_expr(Column::Status, Expr::value(RunStatus::Cancelled))
                    .col_expr(
                        Column::Error,
                        Expr::value("Cancelled while the worker was lost"),
                    )
                    .col_expr(Column::FinishedAt, Expr::value(now))
                    .col_expr(Column::LeasedUntil, Expr::value(None::<DateTimeUtc>))
                    .exec(db)
                    .await?
                    .rows_affected;
                result.cancelled += updated;
            } else if run.attempts >= max_attempts {
                let updated = guarded
                    .col_expr(Column::Status, Expr::value(RunStatus::Failed))
                    .col_expr(
                        Column::FailureReason,
                        Expr::value(FailureReason::WorkerLost),
                    )
                    .col_expr(
                        Column::Error,
                        Expr::value(format!(
                            "Worker lost: lease expired after {} attempt(s)",
                            run.attempts
                        )),
                    )
                    .col_expr(Column::FinishedAt, Expr::value(now))
                    .col_expr(Column::LeasedUntil, Expr::value(None::<DateTimeUtc>))
                    .exec(db)
                    .await?
                    .rows_affected;
                result.failed += updated;
            } else {
                let updated = guarded
                    .col_expr(Column::Status, Expr::value(RunStatus::Queued))
                    .col_expr(Column::StartedAt, Expr::value(None::<DateTimeUtc>))
                    .col_expr(Column::LeasedUntil, Expr::value(None::<DateTimeUtc>))
                    .col_expr(Column::WorkerName, Expr::value(None::<String>))
                    .col_expr(Column::WorkerTokenHash, Expr::value(None::<String>))
                    .exec(db)
                    .await?
                    .rows_affected;
                result.requeued += updated;
            }
        }
        Ok(result)
    }

    /// Fails runs that waited or ran too long. A queued run older than `queued_timeout` fails with
    /// [`FailureReason::QueuedTimeout`]; a running run older than `run_timeout` (counted from
    /// `started_at` of the current attempt) fails with [`FailureReason::TimedOut`]. Running runs
    /// also get `cancel_requested` and lose lease and token, so the worker stops the engine (an
    /// unleased desktop worker polls the flag, a remote one gets 401 on its next heartbeat).
    /// `None` disables the respective limit. Returns the number of runs failed.
    ///
    /// # Arguments
    /// * `db` - database connection
    /// * `run_timeout` - maximum running time, `None` for no limit
    /// * `queued_timeout` - maximum time in the queue, `None` for no limit
    pub async fn expire_timeouts<C: ConnectionTrait>(
        db: &C,
        run_timeout: Option<chrono::Duration>,
        queued_timeout: Option<chrono::Duration>,
    ) -> Result<Expired, Error> {
        let now = chrono::Utc::now();
        let mut result = Expired::default();
        if let Some(limit) = queued_timeout {
            result.queued = Entity::update_many()
                .col_expr(Column::Status, Expr::value(RunStatus::Failed))
                .col_expr(
                    Column::FailureReason,
                    Expr::value(FailureReason::QueuedTimeout),
                )
                .col_expr(
                    Column::Error,
                    Expr::value(format!(
                        "No worker picked up the run within {} seconds",
                        limit.num_seconds()
                    )),
                )
                .col_expr(Column::FinishedAt, Expr::value(now))
                .filter(Column::Status.eq(RunStatus::Queued))
                .filter(Column::CreatedAt.lt(now - limit))
                .exec(db)
                .await?
                .rows_affected;
        }
        if let Some(limit) = run_timeout {
            result.running = Entity::update_many()
                .col_expr(Column::Status, Expr::value(RunStatus::Failed))
                .col_expr(Column::FailureReason, Expr::value(FailureReason::TimedOut))
                .col_expr(
                    Column::Error,
                    Expr::value(format!(
                        "Run exceeded the time limit of {} seconds",
                        limit.num_seconds()
                    )),
                )
                .col_expr(Column::CancelRequested, Expr::value(true))
                .col_expr(Column::LeasedUntil, Expr::value(None::<DateTimeUtc>))
                .col_expr(Column::WorkerTokenHash, Expr::value(None::<String>))
                .col_expr(Column::FinishedAt, Expr::value(now))
                .filter(Column::Status.eq(RunStatus::Running))
                .filter(Column::StartedAt.lt(now - limit))
                .exec(db)
                .await?
                .rows_affected;
        }
        Ok(result)
    }

    /// Moves a run to the terminal `status`. A run that is already terminal (e.g. cancelled while
    /// queued) is left untouched and returns [`Error::AlreadyFinished`].
    ///
    /// A `Failed` report for a run with a pending cancel request is stored as `Cancelled` (the
    /// engine was stopped on purpose, the error is noise). `Succeeded` is kept: the work completed.
    /// For `Failed`, a missing `failure_reason` defaults to [`FailureReason::ExitCode`]; for every
    /// other status the reason is dropped.
    ///
    /// # Arguments
    /// * `db` - database connection
    /// * `id` - run to finish
    /// * `status` - terminal status reported by the worker
    /// * `exit_code` - engine exit code, if it exited
    /// * `error` - details of a failure
    /// * `failure_reason` - cause of a failure, only used with `Failed`
    pub async fn finish<C: ConnectionTrait>(
        db: &C,
        id: i64,
        status: RunStatus,
        exit_code: Option<i32>,
        error: Option<String>,
        failure_reason: Option<FailureReason>,
    ) -> Result<(), Error> {
        debug_assert!(status.is_terminal());
        let (status, error, failure_reason) = if status == RunStatus::Failed {
            let cancel_requested = Entity::find_by_id(id)
                .one(db)
                .await?
                .is_some_and(|run| run.cancel_requested);
            if cancel_requested {
                (RunStatus::Cancelled, None, None)
            } else {
                (
                    status,
                    error,
                    Some(failure_reason.unwrap_or(FailureReason::ExitCode)),
                )
            }
        } else {
            (status, error, None)
        };
        let updated = Entity::update_many()
            .col_expr(Column::Status, Expr::value(status))
            .col_expr(Column::ExitCode, Expr::value(exit_code))
            .col_expr(Column::Error, Expr::value(error))
            .col_expr(Column::FailureReason, Expr::value(failure_reason))
            .col_expr(Column::FinishedAt, Expr::value(chrono::Utc::now()))
            .filter(Column::Id.eq(id))
            .filter(Column::Status.is_in([RunStatus::Queued, RunStatus::Running]))
            .exec(db)
            .await?
            .rows_affected;
        if updated == 1 {
            return Ok(());
        }
        match Entity::find_by_id(id).one(db).await? {
            Some(_) => Err(Error::AlreadyFinished),
            None => Err(Error::NotFound),
        }
    }

    /// Stores the progress the engine reported. Values are absolute (the worker keeps the counts);
    /// `message` replaces the previous detail only when given. Terminal runs keep their progress.
    pub async fn set_progress<C: ConnectionTrait>(
        db: &C,
        id: i64,
        tasks_total: i32,
        tasks_done: i32,
        message: Option<String>,
    ) -> Result<(), Error> {
        let mut update = Entity::update_many()
            .col_expr(Column::TasksTotal, Expr::value(tasks_total.max(0)))
            .col_expr(Column::TasksDone, Expr::value(tasks_done.max(0)))
            .filter(Column::Id.eq(id))
            .filter(Column::Status.is_in([RunStatus::Queued, RunStatus::Running]));
        if let Some(message) = message {
            update = update.col_expr(Column::ProgressMessage, Expr::value(message));
        }
        let updated = update.exec(db).await?.rows_affected;
        if updated == 1 {
            return Ok(());
        }
        match Entity::find_by_id(id).one(db).await? {
            Some(_) => Err(Error::AlreadyFinished),
            None => Err(Error::NotFound),
        }
    }

    /// Newest run of a project whose workflow still exists, optionally of one workflow only.
    /// Its `parameters` are the history that prefills the start dialog.
    pub async fn latest_for_project<C: ConnectionTrait>(
        db: &C,
        project_id: i64,
        workflow_id: Option<i64>,
    ) -> Result<Option<Model>, Error> {
        let mut query = Entity::find().filter(Column::ProjectId.eq(project_id));
        query = match workflow_id {
            Some(id) => query.filter(Column::WorkflowId.eq(id)),
            None => query.filter(Column::WorkflowId.is_not_null()),
        };
        Ok(query
            .order_by_desc(Column::CreatedAt)
            .order_by_desc(Column::Id)
            .one(db)
            .await?)
    }

    /// Whether a user asked to stop the run
    pub async fn is_cancel_requested<C: ConnectionTrait>(db: &C, id: i64) -> Result<bool, Error> {
        Entity::find_by_id(id)
            .one(db)
            .await?
            .map(|run| run.cancel_requested)
            .ok_or(Error::NotFound)
    }

    /// Fails runs still marked `Running`, for use at start when no worker can be executing them
    /// (embedded desktop worker crashed or was killed). Returns the number of runs changed.
    pub async fn fail_orphaned_running<C: ConnectionTrait>(db: &C) -> Result<u64, Error> {
        Ok(Entity::update_many()
            .col_expr(Column::Status, Expr::value(RunStatus::Failed))
            .col_expr(
                Column::FailureReason,
                Expr::value(FailureReason::WorkerLost),
            )
            .col_expr(
                Column::Error,
                Expr::value("Interrupted: the application stopped while the run was active"),
            )
            .col_expr(Column::FinishedAt, Expr::value(chrono::Utc::now()))
            .filter(Column::Status.eq(RunStatus::Running))
            .exec(db)
            .await?
            .rows_affected)
    }

    /// Evicts the oldest evictable runs of a project (see [`Model::is_evictable`]) so that one
    /// more run fits into `max_runs`.
    ///
    /// Call it inside the same transaction that inserts the new run. Logs are removed by cascade.
    /// Locked runs count towards `max_runs`; queued/running runs are never evicted.
    /// Nothing is deleted if too few runs are evictable. `max_runs == 0` means unlimited.
    pub async fn make_room<C: ConnectionTrait>(
        db: &C,
        project_id: i64,
        max_runs: u64,
    ) -> Result<(), Error> {
        if max_runs == 0 {
            return Ok(());
        }
        let runs = Entity::find()
            .filter(Column::ProjectId.eq(project_id))
            .order_by_asc(Column::CreatedAt)
            .order_by_asc(Column::Id)
            .all(db)
            .await?;
        let excess = (runs.len() as u64 + 1).saturating_sub(max_runs) as usize;
        let victims: Vec<i64> = runs
            .iter()
            .filter(|run| run.is_evictable())
            .take(excess)
            .map(|run| run.id)
            .collect();
        if victims.len() < excess {
            return Err(Error::RunLimitReached(max_runs));
        }
        if !victims.is_empty() {
            Entity::delete_many()
                .filter(Column::Id.is_in(victims))
                .exec(db)
                .await?;
        }
        Ok(())
    }
}

#[cfg(all(test, feature = "integration"))]
mod tests {
    use chrono::{Duration, Utc};
    use macworp_database_migration::{Migrator, MigratorTrait};
    use sea_orm::{ActiveModelTrait, Database, DatabaseConnection, Set};

    use super::*;
    use crate::models::{ActiveProject, ActiveRunLog, ActiveUser, RunLogTable};

    async fn setup() -> (DatabaseConnection, i64, i64) {
        let db = Database::connect("sqlite::memory:").await.unwrap();
        Migrator::up(&db, None).await.unwrap();
        let user = ActiveUser {
            username: Set("u".into()),
            firstname: Set("f".into()),
            lastname: Set("l".into()),
            email: Set("u@example.org".into()),
            ..Default::default()
        }
        .insert(&db)
        .await
        .unwrap();
        let project = ActiveProject {
            owner_id: Set(Some(user.id)),
            name: Set("p".into()),
            ..Default::default()
        }
        .insert(&db)
        .await
        .unwrap();
        (db, user.id, project.id)
    }

    async fn add_run(
        db: &DatabaseConnection,
        user: i64,
        project: i64,
        age: i64,
        status: RunStatus,
        locked: bool,
    ) -> i64 {
        let run = ActiveModel {
            project_id: Set(project),
            started_by: Set(Some(user)),
            configuration: Set(WorkflowConfiguration::default()),
            parameters: Set(Json::Object(Default::default())),
            comment: Set(String::new()),
            status: Set(status),
            cancel_requested: Set(false),
            locked: Set(locked),
            created_at: Set(Utc::now() - Duration::minutes(age)),
            ..Default::default()
        }
        .insert(db)
        .await
        .unwrap();
        ActiveRunLog {
            run_id: Set(run.id),
            seq: Set(0),
            stream: Set(Default::default()),
            created_at: Set(Utc::now()),
            content: Set("hi\nthere".into()),
            line_count: Set(2),
            ..Default::default()
        }
        .insert(db)
        .await
        .unwrap();
        run.id
    }

    async fn ids(db: &DatabaseConnection) -> Vec<i64> {
        let mut ids: Vec<i64> = Entity::find()
            .all(db)
            .await
            .unwrap()
            .into_iter()
            .map(|r| r.id)
            .collect();
        ids.sort();
        ids
    }

    #[tokio::test]
    async fn evicts_oldest_unlocked_and_cascades_logs() {
        let (db, u, p) = setup().await;
        let oldest = add_run(&db, u, p, 30, RunStatus::Succeeded, false).await;
        let b = add_run(&db, u, p, 20, RunStatus::Failed, false).await;
        let c = add_run(&db, u, p, 10, RunStatus::Succeeded, false).await;
        Entity::make_room(&db, p, 3).await.unwrap();
        assert_eq!(ids(&db).await, vec![b, c]);
        assert!(!ids(&db).await.contains(&oldest));
        assert_eq!(RunLogTable::find().all(&db).await.unwrap().len(), 2);
    }

    #[tokio::test]
    async fn progress_is_stored_for_active_runs_only() {
        let (db, u, p) = setup().await;
        let running = add_run(&db, u, p, 2, RunStatus::Running, false).await;
        let done = add_run(&db, u, p, 1, RunStatus::Succeeded, false).await;

        Entity::set_progress(&db, running, 5, 2, Some("Task 1: foo".into()))
            .await
            .unwrap();
        // without message the previous one stays
        Entity::set_progress(&db, running, 6, 3, None)
            .await
            .unwrap();
        let run = Entity::find_by_id(running).one(&db).await.unwrap().unwrap();
        assert_eq!((run.tasks_total, run.tasks_done), (6, 3));
        assert_eq!(run.progress_message.as_deref(), Some("Task 1: foo"));

        assert!(matches!(
            Entity::set_progress(&db, done, 1, 1, None).await,
            Err(Error::AlreadyFinished)
        ));
        assert!(matches!(
            Entity::set_progress(&db, 9999, 1, 1, None).await,
            Err(Error::NotFound)
        ));
    }

    #[tokio::test]
    async fn logs_are_listed_after_cursor() {
        let (db, u, p) = setup().await;
        let run = add_run(&db, u, p, 1, RunStatus::Running, false).await; // seq 0 exists
        for line in ["a", "b", "c"] {
            RunLogTable::append(&db, run, Default::default(), &[line.to_string()], 0)
                .await
                .unwrap();
        }
        let all = RunLogTable::list_after(&db, run, None, 10).await.unwrap();
        assert_eq!(
            all.iter().map(|c| c.seq).collect::<Vec<_>>(),
            vec![0, 1, 2, 3]
        );
        let rest = RunLogTable::list_after(&db, run, Some(1), 10)
            .await
            .unwrap();
        assert_eq!(rest.iter().map(|c| c.seq).collect::<Vec<_>>(), vec![2, 3]);
        let page = RunLogTable::list_after(&db, run, Some(1), 1).await.unwrap();
        assert_eq!(page.len(), 1);
        assert!(
            RunLogTable::list_after(&db, run, Some(3), 10)
                .await
                .unwrap()
                .is_empty()
        );
    }

    #[tokio::test]
    async fn cancel_rules() {
        let (db, u, p) = setup().await;
        let get = async |id| Entity::find_by_id(id).one(&db).await.unwrap().unwrap();
        let queued = add_run(&db, u, p, 3, RunStatus::Queued, false).await;
        let running = add_run(&db, u, p, 2, RunStatus::Running, false).await;
        let done = add_run(&db, u, p, 1, RunStatus::Succeeded, false).await;

        let r = get(queued).await.request_cancel(&db).await.unwrap();
        assert_eq!(r.status, RunStatus::Cancelled);
        assert!(r.finished_at.is_some());

        let r = get(running).await.request_cancel(&db).await.unwrap();
        assert_eq!(r.status, RunStatus::Running);
        assert!(r.cancel_requested);

        let err = get(done).await.request_cancel(&db).await.unwrap_err();
        assert!(matches!(err, Error::AlreadyFinished));
        let err = get(queued).await.request_cancel(&db).await.unwrap_err();
        assert!(matches!(err, Error::AlreadyFinished));
    }

    #[tokio::test]
    async fn locked_run_is_kept() {
        let (db, u, p) = setup().await;
        let oldest = add_run(&db, u, p, 30, RunStatus::Succeeded, false).await;
        let b = add_run(&db, u, p, 20, RunStatus::Succeeded, false).await;
        let c = add_run(&db, u, p, 10, RunStatus::Succeeded, false).await;
        Entity::find_by_id(oldest)
            .one(&db)
            .await
            .unwrap()
            .unwrap()
            .set_locked(&db, true)
            .await
            .unwrap();
        Entity::make_room(&db, p, 3).await.unwrap();
        assert_eq!(ids(&db).await, vec![oldest, c]);
        assert!(!ids(&db).await.contains(&b));
    }

    #[tokio::test]
    async fn all_locked_or_active_is_rejected() {
        let (db, u, p) = setup().await;
        add_run(&db, u, p, 30, RunStatus::Succeeded, true).await;
        add_run(&db, u, p, 20, RunStatus::Running, false).await;
        let err = Entity::make_room(&db, p, 2).await.unwrap_err();
        assert!(matches!(err, Error::RunLimitReached(2)));
        assert_eq!(ids(&db).await.len(), 2);
    }

    #[tokio::test]
    async fn zero_is_unlimited_and_other_projects_untouched() {
        let (db, u, p) = setup().await;
        for i in 0..4 {
            add_run(&db, u, p, 10 + i, RunStatus::Succeeded, false).await;
        }
        Entity::make_room(&db, p, 0).await.unwrap();
        assert_eq!(ids(&db).await.len(), 4);
        Entity::make_room(&db, p + 1, 1).await.unwrap();
        assert_eq!(ids(&db).await.len(), 4);
    }

    #[tokio::test]
    async fn claim_takes_oldest_queued_once() {
        let (db, u, p) = setup().await;
        let newer = add_run(&db, u, p, 1, RunStatus::Queued, false).await;
        let older = add_run(&db, u, p, 5, RunStatus::Queued, false).await;
        add_run(&db, u, p, 9, RunStatus::Running, false).await;

        let a = Entity::claim_next(&db).await.unwrap().unwrap();
        assert_eq!(a.id, older);
        assert_eq!(a.status, RunStatus::Running);
        assert!(a.started_at.is_some());
        let b = Entity::claim_next(&db).await.unwrap().unwrap();
        assert_eq!(b.id, newer);
        assert!(Entity::claim_next(&db).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn finish_sets_result_and_keeps_cancelled() {
        let (db, u, p) = setup().await;
        let get = async |id| Entity::find_by_id(id).one(&db).await.unwrap().unwrap();
        let running = add_run(&db, u, p, 2, RunStatus::Running, false).await;
        Entity::finish(
            &db,
            running,
            RunStatus::Failed,
            Some(2),
            Some("boom".into()),
            None,
        )
        .await
        .unwrap();
        let r = get(running).await;
        assert_eq!(r.status, RunStatus::Failed);
        // no reason given: a failure defaults to the exit code
        assert_eq!(r.failure_reason, Some(FailureReason::ExitCode));
        assert_eq!(r.exit_code, Some(2));
        assert_eq!(r.error.as_deref(), Some("boom"));
        assert!(r.finished_at.is_some());

        let cancelled = add_run(&db, u, p, 1, RunStatus::Cancelled, false).await;
        let err = Entity::finish(&db, cancelled, RunStatus::Succeeded, Some(0), None, None)
            .await
            .unwrap_err();
        assert!(matches!(err, Error::AlreadyFinished));
        assert_eq!(get(cancelled).await.status, RunStatus::Cancelled);
        let err = Entity::finish(&db, 999, RunStatus::Failed, None, None, None)
            .await
            .unwrap_err();
        assert!(matches!(err, Error::NotFound));
    }

    #[tokio::test]
    async fn orphaned_running_runs_fail() {
        let (db, u, p) = setup().await;
        let running = add_run(&db, u, p, 2, RunStatus::Running, false).await;
        let queued = add_run(&db, u, p, 1, RunStatus::Queued, false).await;
        assert_eq!(Entity::fail_orphaned_running(&db).await.unwrap(), 1);
        let get = async |id| Entity::find_by_id(id).one(&db).await.unwrap().unwrap();
        assert_eq!(get(running).await.status, RunStatus::Failed);
        assert_eq!(get(queued).await.status, RunStatus::Queued);
    }

    #[tokio::test]
    async fn cancel_flag_is_readable() {
        let (db, u, p) = setup().await;
        let id = add_run(&db, u, p, 1, RunStatus::Running, false).await;
        assert!(!Entity::is_cancel_requested(&db, id).await.unwrap());
        get_run(&db, id).await.request_cancel(&db).await.unwrap();
        assert!(Entity::is_cancel_requested(&db, id).await.unwrap());
    }

    async fn get_run(db: &DatabaseConnection, id: i64) -> Model {
        Entity::find_by_id(id).one(db).await.unwrap().unwrap()
    }

    #[tokio::test]
    async fn log_append_sequences_and_truncates_once() {
        use crate::models::{RunLogColumn, RunLogTable, run_log::TRUNCATION_MARKER};
        use macworp_core::entities::LogStream;
        let (db, u, p) = setup().await;
        let id = add_run(&db, u, p, 1, RunStatus::Running, false).await;
        // add_run stored one chunk (seq 0, 8 bytes)
        let lines = vec!["abcd".to_string(), "efgh".to_string()];
        RunLogTable::append(&db, id, LogStream::Stderr, &lines, 0)
            .await
            .unwrap();
        RunLogTable::append(&db, id, LogStream::Stdout, &lines, 20)
            .await
            .unwrap(); // 16 bytes stored < 20: kept, crosses the limit
        RunLogTable::append(&db, id, LogStream::Stdout, &lines, 20)
            .await
            .unwrap(); // 25 >= 20: marker
        RunLogTable::append(&db, id, LogStream::Stdout, &lines, 20)
            .await
            .unwrap(); // dropped
        let logs = RunLogTable::find()
            .filter(RunLogColumn::RunId.eq(id))
            .order_by_asc(RunLogColumn::Seq)
            .all(&db)
            .await
            .unwrap();
        let seqs: Vec<i64> = logs.iter().map(|l| l.seq).collect();
        assert_eq!(seqs, vec![0, 1, 2, 3]);
        assert_eq!(logs[1].line_count, 2);
        assert_eq!(logs[1].stream, LogStream::Stderr);
        assert_eq!(logs[3].content, TRUNCATION_MARKER);
    }

    #[tokio::test]
    async fn deleting_workflow_nulls_reference() {
        let (db, u, p) = setup().await;
        let wf = crate::models::ActiveWorkflowIntegration {
            owner_id: Set(Some(u)),
            name: Set("w".into()),
            description: Set("d".into()),
            configuration: Set(WorkflowConfiguration::default()),
            ..Default::default()
        }
        .insert(&db)
        .await
        .unwrap();
        let id = add_run(&db, u, p, 1, RunStatus::Succeeded, false).await;
        Entity::update_many()
            .col_expr(Column::WorkflowId, Expr::value(wf.id))
            .exec(&db)
            .await
            .unwrap();
        crate::models::WorkflowIntegrationTable::delete_by_id(wf.id)
            .exec(&db)
            .await
            .unwrap();
        let run = Entity::find_by_id(id).one(&db).await.unwrap().unwrap();
        assert_eq!(run.workflow_id, None);
    }

    #[tokio::test]
    async fn latest_for_project_picks_newest_with_workflow() {
        let (db, u, p) = setup().await;
        let mut wfs = Vec::new();
        for name in ["a", "b"] {
            wfs.push(
                crate::models::ActiveWorkflowIntegration {
                    owner_id: Set(Some(u)),
                    name: Set(name.into()),
                    description: Set("d".into()),
                    configuration: Set(WorkflowConfiguration::default()),
                    ..Default::default()
                }
                .insert(&db)
                .await
                .unwrap()
                .id,
            );
        }
        assert!(
            Entity::latest_for_project(&db, p, None)
                .await
                .unwrap()
                .is_none()
        );

        let old = add_run(&db, u, p, 30, RunStatus::Succeeded, false).await;
        let mid = add_run(&db, u, p, 20, RunStatus::Failed, false).await;
        let new = add_run(&db, u, p, 10, RunStatus::Succeeded, false).await;
        // Newest run, but without a workflow: skipped
        add_run(&db, u, p, 1, RunStatus::Succeeded, false).await;
        for (id, wf) in [(old, wfs[0]), (mid, wfs[1]), (new, wfs[0])] {
            Entity::update_many()
                .col_expr(Column::WorkflowId, Expr::value(wf))
                .filter(Column::Id.eq(id))
                .exec(&db)
                .await
                .unwrap();
        }

        let latest = |wf| Entity::latest_for_project(&db, p, wf);
        assert_eq!(latest(None).await.unwrap().unwrap().id, new);
        assert_eq!(latest(Some(wfs[1])).await.unwrap().unwrap().id, mid);
        assert_eq!(latest(Some(wfs[0])).await.unwrap().unwrap().id, new);
        assert!(latest(Some(wfs[1] + 100)).await.unwrap().is_none());
    }

    /// Expires the lease of `id` by moving `leased_until` into the past
    async fn expire_lease(db: &DatabaseConnection, id: i64) {
        Entity::update_many()
            .col_expr(
                Column::LeasedUntil,
                Expr::value(Utc::now() - Duration::seconds(5)),
            )
            .filter(Column::Id.eq(id))
            .exec(db)
            .await
            .unwrap();
    }

    /// A leased claim stores lease, worker and token hash (never the token), and the token works
    #[tokio::test]
    async fn leased_claim_issues_token() {
        let (db, user, project) = setup().await;
        let id = add_run(&db, user, project, 1, RunStatus::Queued, false).await;
        let (run, token) = Entity::claim_next_leased(&db, "w1", 60, &Engine::all())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(run.id, id);
        assert_eq!(run.status, RunStatus::Running);
        assert_eq!(run.attempts, 1);
        assert_eq!(run.worker_name.as_deref(), Some("w1"));
        assert!(run.leased_until.unwrap() > Utc::now());
        assert_ne!(run.worker_token_hash.as_deref(), Some(token.as_str()));
        assert!(Entity::authenticate(&db, id, &token).await.is_ok());
        assert!(matches!(
            Entity::authenticate(&db, id, "wrong").await,
            Err(Error::InvalidToken)
        ));
        assert!(
            Entity::claim_next_leased(&db, "w2", 60, &Engine::all())
                .await
                .unwrap()
                .is_none()
        );
    }

    /// Heartbeat extends the lease and reports a pending cancel
    #[tokio::test]
    async fn heartbeat_extends_lease_and_reports_cancel() {
        let (db, user, project) = setup().await;
        let id = add_run(&db, user, project, 1, RunStatus::Queued, false).await;
        let (_, token) = Entity::claim_next_leased(&db, "w1", 60, &Engine::all())
            .await
            .unwrap()
            .unwrap();
        expire_lease(&db, id).await;
        assert!(!Entity::heartbeat(&db, id, &token, 60).await.unwrap());
        assert!(
            Entity::find_by_id(id)
                .one(&db)
                .await
                .unwrap()
                .unwrap()
                .leased_until
                .unwrap()
                > Utc::now()
        );
        Entity::update_many()
            .col_expr(Column::CancelRequested, Expr::value(true))
            .filter(Column::Id.eq(id))
            .exec(&db)
            .await
            .unwrap();
        assert!(Entity::heartbeat(&db, id, &token, 60).await.unwrap());
    }

    /// An expired run is requeued, its old token stops working, and a new claim gets a new token
    #[tokio::test]
    async fn expired_lease_requeues_and_invalidates_token() {
        let (db, user, project) = setup().await;
        let id = add_run(&db, user, project, 1, RunStatus::Queued, false).await;
        let (_, old) = Entity::claim_next_leased(&db, "w1", 60, &Engine::all())
            .await
            .unwrap()
            .unwrap();
        // not expired yet: nothing happens
        assert_eq!(
            Entity::requeue_expired(&db, 3).await.unwrap(),
            Requeued::default()
        );
        expire_lease(&db, id).await;
        let result = Entity::requeue_expired(&db, 3).await.unwrap();
        assert_eq!(
            result,
            Requeued {
                requeued: 1,
                ..Default::default()
            }
        );
        assert!(matches!(
            Entity::authenticate(&db, id, &old).await,
            Err(Error::InvalidToken)
        ));
        assert!(matches!(
            Entity::heartbeat(&db, id, &old, 60).await,
            Err(Error::InvalidToken)
        ));
        let (run, new) = Entity::claim_next_leased(&db, "w2", 60, &Engine::all())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(run.attempts, 2);
        assert_ne!(old, new);
    }

    /// A run that used all attempts fails instead of requeueing; a pending cancel wins
    #[tokio::test]
    async fn expired_lease_fails_at_max_attempts_or_cancels() {
        let (db, user, project) = setup().await;
        let id = add_run(&db, user, project, 2, RunStatus::Queued, false).await;
        Entity::claim_next_leased(&db, "w1", 60, &Engine::all())
            .await
            .unwrap()
            .unwrap();
        expire_lease(&db, id).await;
        let result = Entity::requeue_expired(&db, 1).await.unwrap();
        assert_eq!(
            result,
            Requeued {
                failed: 1,
                ..Default::default()
            }
        );
        let run = Entity::find_by_id(id).one(&db).await.unwrap().unwrap();
        assert_eq!(run.status, RunStatus::Failed);
        assert!(run.error.unwrap().contains("lease expired"));
        assert_eq!(run.failure_reason, Some(FailureReason::WorkerLost));

        let id = add_run(&db, user, project, 1, RunStatus::Queued, false).await;
        Entity::claim_next_leased(&db, "w1", 60, &Engine::all())
            .await
            .unwrap()
            .unwrap();
        Entity::update_many()
            .col_expr(Column::CancelRequested, Expr::value(true))
            .filter(Column::Id.eq(id))
            .exec(&db)
            .await
            .unwrap();
        expire_lease(&db, id).await;
        let result = Entity::requeue_expired(&db, 3).await.unwrap();
        assert_eq!(
            result,
            Requeued {
                cancelled: 1,
                ..Default::default()
            }
        );
    }

    /// Runs claimed without a lease (desktop) are never requeued
    #[tokio::test]
    async fn unleased_running_runs_are_not_requeued() {
        let (db, user, project) = setup().await;
        add_run(&db, user, project, 1, RunStatus::Queued, false).await;
        Entity::claim_next(&db).await.unwrap().unwrap();
        assert_eq!(
            Entity::requeue_expired(&db, 3).await.unwrap(),
            Requeued::default()
        );
    }

    /// Queued runs over the queue limit fail with `QueuedTimeout`, younger ones and a disabled
    /// limit leave runs alone.
    #[tokio::test]
    async fn queued_runs_time_out() {
        let (db, user, project) = setup().await;
        let old = add_run(&db, user, project, 120, RunStatus::Queued, false).await;
        let young = add_run(&db, user, project, 1, RunStatus::Queued, false).await;
        let limit = Some(Duration::minutes(60));

        assert_eq!(
            Entity::expire_timeouts(&db, None, None).await.unwrap(),
            Expired::default()
        );
        let result = Entity::expire_timeouts(&db, None, limit).await.unwrap();
        assert_eq!(
            result,
            Expired {
                queued: 1,
                running: 0
            }
        );
        let old = Entity::find_by_id(old).one(&db).await.unwrap().unwrap();
        assert_eq!(old.status, RunStatus::Failed);
        assert_eq!(old.failure_reason, Some(FailureReason::QueuedTimeout));
        assert!(old.finished_at.is_some());
        let young = Entity::find_by_id(young).one(&db).await.unwrap().unwrap();
        assert_eq!(young.status, RunStatus::Queued);
    }

    /// A running run over the limit fails with `TimedOut`, is flagged for cancellation and loses
    /// its token, so the worker stops it; a run within the limit is untouched.
    #[tokio::test]
    async fn running_runs_time_out() {
        let (db, user, project) = setup().await;
        let slow = add_run(&db, user, project, 3, RunStatus::Queued, false).await;
        let (_, token) = Entity::claim_next_leased(&db, "w1", 60, &Engine::all())
            .await
            .unwrap()
            .unwrap();
        let fresh = add_run(&db, user, project, 2, RunStatus::Queued, false).await;
        Entity::claim_next_leased(&db, "w1", 60, &Engine::all())
            .await
            .unwrap()
            .unwrap();
        Entity::update_many()
            .col_expr(
                Column::StartedAt,
                Expr::value(Utc::now() - Duration::hours(2)),
            )
            .filter(Column::Id.eq(slow))
            .exec(&db)
            .await
            .unwrap();

        let result = Entity::expire_timeouts(&db, Some(Duration::hours(1)), None)
            .await
            .unwrap();
        assert_eq!(
            result,
            Expired {
                queued: 0,
                running: 1
            }
        );
        let slow_run = Entity::find_by_id(slow).one(&db).await.unwrap().unwrap();
        assert_eq!(slow_run.status, RunStatus::Failed);
        assert_eq!(slow_run.failure_reason, Some(FailureReason::TimedOut));
        assert!(slow_run.cancel_requested);
        assert!(slow_run.leased_until.is_none());
        assert!(matches!(
            Entity::heartbeat(&db, slow, &token, 60).await,
            Err(Error::InvalidToken)
        ));
        let fresh_run = Entity::find_by_id(fresh).one(&db).await.unwrap().unwrap();
        assert_eq!(fresh_run.status, RunStatus::Running);
    }

    /// A failure report for a run with a pending cancel is stored as cancelled; a success is kept;
    /// reasons are dropped for other statuses.
    #[tokio::test]
    async fn finish_applies_cancel_and_reason_rules() {
        let (db, user, project) = setup().await;
        let get = async |id| Entity::find_by_id(id).one(&db).await.unwrap().unwrap();

        let id = add_run(&db, user, project, 3, RunStatus::Running, false).await;
        get(id).await.request_cancel(&db).await.unwrap();
        Entity::finish(
            &db,
            id,
            RunStatus::Failed,
            Some(143),
            Some("Exited with code 143".into()),
            Some(FailureReason::ExitCode),
        )
        .await
        .unwrap();
        let run = get(id).await;
        assert_eq!(run.status, RunStatus::Cancelled);
        assert_eq!(run.failure_reason, None);

        let id = add_run(&db, user, project, 2, RunStatus::Running, false).await;
        get(id).await.request_cancel(&db).await.unwrap();
        Entity::finish(
            &db,
            id,
            RunStatus::Succeeded,
            Some(0),
            None,
            Some(FailureReason::Signal),
        )
        .await
        .unwrap();
        let run = get(id).await;
        assert_eq!(run.status, RunStatus::Succeeded);
        assert_eq!(run.failure_reason, None);

        let id = add_run(&db, user, project, 1, RunStatus::Running, false).await;
        Entity::finish(
            &db,
            id,
            RunStatus::Failed,
            None,
            Some("Killed by signal 9".into()),
            Some(FailureReason::Signal),
        )
        .await
        .unwrap();
        assert_eq!(get(id).await.failure_reason, Some(FailureReason::Signal));
    }
}
