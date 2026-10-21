use macworp_core::{entities::RunStatus, workflow_configuration::WorkflowConfiguration};
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
    pub status: RunStatus,
    /// Exit code of the engine, `None` until it exited
    #[sea_orm(nullable)]
    pub exit_code: Option<i32>,
    /// Why the run failed if the reason is not the exit code, e.g. timeout or container runtime missing
    #[sea_orm(nullable)]
    pub error: Option<String>,
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
    #[sea_orm(belongs_to, relation_enum = "Workflow", from = "workflow_id", to = "id")]
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
    #[error("Database error: {0}")]
    Database(#[from] DbErr),
}

impl Model {
    /// A run can be evicted by the run limit if it is finished and not locked
    pub fn is_evictable(&self) -> bool {
        !self.locked && self.status.is_terminal()
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
            status: Set(status),
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
    async fn deleting_workflow_nulls_reference() {
        let (db, u, p) = setup().await;
        let wf = crate::models::ActiveWorkflowIntegration {
            owner_id: Set(u),
            is_public: Set(false),
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
}
