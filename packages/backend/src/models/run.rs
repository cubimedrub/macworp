use macworp_core::{
    entities::RunStatus, responses::RunResponse, workflow_configuration::WorkflowConfiguration,
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
    /// User who started the run
    pub started_by: i64,
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
pub enum RunError {
    /// No run can be evicted for a new one: all are locked or still active
    #[error("Run limit of {0} reached, all runs are locked or still active")]
    RunLimitReached(u64),
    /// No run with the given id
    #[error("Run not found")]
    NotFound,
    /// The run is already in a terminal status
    #[error("Run already finished")]
    AlreadyFinished,
    #[error("Database error: {0}")]
    Database(#[from] DbErr),
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
    /// `cancel_requested` for its worker to stop. Finished runs return [`RunError::AlreadyFinished`].
    pub async fn request_cancel<C: ConnectionTrait>(&self, db: &C) -> Result<Model, RunError> {
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
            _ => return Err(RunError::AlreadyFinished),
        }
        Ok(active.update(db).await?)
    }

    /// Locks or unlocks the run, a locked run is kept when newer runs exceed the project's limit
    pub async fn set_locked<C: ConnectionTrait>(
        &self,
        db: &C,
        locked: bool,
    ) -> Result<Model, RunError> {
        ActiveModel {
            id: Set(self.id),
            locked: Set(locked),
            ..Default::default()
        }
        .update(db)
        .await
        .map_err(|e| match e {
            DbErr::RecordNotUpdated => RunError::NotFound,
            e => RunError::Database(e),
        })
    }
}

impl Entity {
    /// Moves the oldest queued run to `Running` and returns it, `None` if nothing is queued.
    ///
    /// The status switch is a conditional update, so concurrent workers never claim the same run.
    pub async fn claim_next<C: ConnectionTrait>(db: &C) -> Result<Option<Model>, RunError> {
        loop {
            let Some(next) = Entity::find()
                .filter(Column::Status.eq(RunStatus::Queued))
                .order_by_asc(Column::CreatedAt)
                .order_by_asc(Column::Id)
                .one(db)
                .await?
            else {
                return Ok(None);
            };
            let claimed = Entity::update_many()
                .col_expr(Column::Status, Expr::value(RunStatus::Running))
                .col_expr(Column::StartedAt, Expr::value(chrono::Utc::now()))
                .filter(Column::Id.eq(next.id))
                .filter(Column::Status.eq(RunStatus::Queued))
                .exec(db)
                .await?
                .rows_affected;
            if claimed == 1 {
                return Ok(Entity::find_by_id(next.id).one(db).await?);
            }
            // lost the race (or the run was cancelled meanwhile), look at the next one
        }
    }

    /// Moves a run to the terminal `status`. A run that is already terminal (e.g. cancelled while
    /// queued) is left untouched and returns [`RunError::AlreadyFinished`].
    pub async fn finish<C: ConnectionTrait>(
        db: &C,
        id: i64,
        status: RunStatus,
        exit_code: Option<i32>,
        error: Option<String>,
    ) -> Result<(), RunError> {
        debug_assert!(status.is_terminal());
        let updated = Entity::update_many()
            .col_expr(Column::Status, Expr::value(status))
            .col_expr(Column::ExitCode, Expr::value(exit_code))
            .col_expr(Column::Error, Expr::value(error))
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
            Some(_) => Err(RunError::AlreadyFinished),
            None => Err(RunError::NotFound),
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
    ) -> Result<(), RunError> {
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
            Some(_) => Err(RunError::AlreadyFinished),
            None => Err(RunError::NotFound),
        }
    }

    /// Newest run of a project whose workflow still exists, optionally of one workflow only.
    /// Its `parameters` are the history that prefills the start dialog.
    pub async fn latest_for_project<C: ConnectionTrait>(
        db: &C,
        project_id: i64,
        workflow_id: Option<i64>,
    ) -> Result<Option<Model>, RunError> {
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
    pub async fn is_cancel_requested<C: ConnectionTrait>(
        db: &C,
        id: i64,
    ) -> Result<bool, RunError> {
        Entity::find_by_id(id)
            .one(db)
            .await?
            .map(|run| run.cancel_requested)
            .ok_or(RunError::NotFound)
    }

    /// Fails runs still marked `Running`, for use at start when no worker can be executing them
    /// (embedded desktop worker crashed or was killed). Returns the number of runs changed.
    pub async fn fail_orphaned_running<C: ConnectionTrait>(db: &C) -> Result<u64, RunError> {
        Ok(Entity::update_many()
            .col_expr(Column::Status, Expr::value(RunStatus::Failed))
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
    ) -> Result<(), RunError> {
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
            return Err(RunError::RunLimitReached(max_runs));
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
            owner_id: Set(user.id),
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
            started_by: Set(user),
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
            Err(RunError::AlreadyFinished)
        ));
        assert!(matches!(
            Entity::set_progress(&db, 9999, 1, 1, None).await,
            Err(RunError::NotFound)
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
        assert!(matches!(err, RunError::AlreadyFinished));
        let err = get(queued).await.request_cancel(&db).await.unwrap_err();
        assert!(matches!(err, RunError::AlreadyFinished));
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
        assert!(matches!(err, RunError::RunLimitReached(2)));
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
        )
        .await
        .unwrap();
        let r = get(running).await;
        assert_eq!(r.status, RunStatus::Failed);
        assert_eq!(r.exit_code, Some(2));
        assert_eq!(r.error.as_deref(), Some("boom"));
        assert!(r.finished_at.is_some());

        let cancelled = add_run(&db, u, p, 1, RunStatus::Cancelled, false).await;
        let err = Entity::finish(&db, cancelled, RunStatus::Succeeded, Some(0), None)
            .await
            .unwrap_err();
        assert!(matches!(err, RunError::AlreadyFinished));
        assert_eq!(get(cancelled).await.status, RunStatus::Cancelled);
        let err = Entity::finish(&db, 999, RunStatus::Failed, None, None)
            .await
            .unwrap_err();
        assert!(matches!(err, RunError::NotFound));
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
            owner_id: Set(u),
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
                    owner_id: Set(u),
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
}
