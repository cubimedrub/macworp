use std::path::PathBuf;

use macworp_backend::models::{ProjectTable, Run, RunError, RunLogTable, RunTable};
use macworp_core::entities::{LogStream, RunStatus};
use macworp_worker::{
    container_runtime::BoxFuture,
    executors::{Job, Outcome},
    job_source::{Error as SourceError, JobSource},
    progress::ProgressUpdate,
};
use sea_orm::{DatabaseConnection, EntityTrait};

/// [`JobSource`] of the embedded worker: claims queued runs straight from the backend database
/// and writes logs and results back.
pub struct DbJobSource {
    db: DatabaseConnection,
    /// Log bytes stored per run, `0` = unlimited
    max_log_bytes: u64,
}

fn source_error(error: impl std::fmt::Display) -> SourceError {
    SourceError(error.to_string())
}

impl DbJobSource {
    pub fn new(db: DatabaseConnection, max_log_bytes: u64) -> Self {
        Self { db, max_log_bytes }
    }

    /// Directory of the project a run belongs to, `None` if the project is gone or has no path
    async fn project_dir(&self, run: &Run) -> Result<Option<PathBuf>, SourceError> {
        let project = ProjectTable::find_by_id(run.project_id)
            .one(&self.db)
            .await
            .map_err(source_error)?;
        Ok(project.and_then(|project| project.path).map(PathBuf::from))
    }

    async fn finish_run(&self, run_id: i64, outcome: Outcome) -> Result<(), SourceError> {
        let (status, exit_code, error) = match outcome {
            Outcome::Succeeded => (RunStatus::Succeeded, Some(0), None),
            Outcome::Failed { exit_code, error } => (RunStatus::Failed, exit_code, error),
            Outcome::Cancelled => (RunStatus::Cancelled, None, None),
        };
        match RunTable::finish(&self.db, run_id, status, exit_code, error).await {
            Ok(()) => Ok(()),
            // the result of a run that was already closed (e.g. deleted or cancelled) is not worth failing for
            Err(RunError::AlreadyFinished | RunError::NotFound) => {
                tracing::warn!("Run {run_id} was already closed, dropping its result");
                Ok(())
            }
            Err(e) => Err(source_error(e)),
        }
    }
}

impl JobSource for DbJobSource {
    fn claim(&self) -> BoxFuture<'_, Result<Option<Job>, SourceError>> {
        Box::pin(async move {
            loop {
                let Some(run) = RunTable::claim_next(&self.db).await.map_err(source_error)? else {
                    return Ok(None);
                };
                match self.project_dir(&run).await? {
                    Some(project_dir) => {
                        return Ok(Some(Job {
                            run_id: run.id,
                            project_dir,
                            configuration: run.configuration,
                            progress: None,
                        }));
                    }
                    None => {
                        self.finish_run(
                            run.id,
                            Outcome::Failed {
                                exit_code: None,
                                error: Some("The project has no directory or was deleted".into()),
                            },
                        )
                        .await?;
                    }
                }
            }
        })
    }

    fn append_logs(
        &self,
        run_id: i64,
        stream: LogStream,
        lines: Vec<String>,
    ) -> BoxFuture<'_, Result<(), SourceError>> {
        Box::pin(async move {
            RunLogTable::append(&self.db, run_id, stream, &lines, self.max_log_bytes)
                .await
                .map_err(source_error)
        })
    }

    fn report_progress(
        &self,
        run_id: i64,
        update: ProgressUpdate,
    ) -> BoxFuture<'_, Result<(), SourceError>> {
        Box::pin(async move {
            RunTable::set_progress(&self.db, run_id, update.total, update.done, update.message)
                .await
                .map_err(source_error)?;
            if let Some(error) = update.error {
                self.append_logs(run_id, LogStream::Engine, vec![error])
                    .await?;
            }
            Ok(())
        })
    }

    fn cancel_requested(&self, run_id: i64) -> BoxFuture<'_, Result<bool, SourceError>> {
        Box::pin(async move {
            RunTable::is_cancel_requested(&self.db, run_id)
                .await
                .map_err(source_error)
        })
    }

    fn finish(&self, run_id: i64, outcome: Outcome) -> BoxFuture<'_, Result<(), SourceError>> {
        Box::pin(async move {
            // A failed write would leave the run `Running` forever, so try once more.
            match self.finish_run(run_id, outcome.clone()).await {
                Ok(()) => Ok(()),
                Err(first) => {
                    tracing::error!("Storing the result of run {run_id} failed, retrying: {first}");
                    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                    self.finish_run(run_id, outcome).await
                }
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use std::{sync::Arc, time::Duration};

    use macworp_backend::models::{ActiveProject, ActiveRun, ActiveUser, RunLogColumn};
    use macworp_core::workflow_configuration::WorkflowConfiguration;
    use macworp_database_migration::{Migrator, MigratorTrait};
    use macworp_worker::{
        executors::{Executor, LogLine},
        worker::{Worker, WorkerOptions},
    };
    use sea_orm::{ActiveModelTrait, ColumnTrait, Database, QueryFilter, QueryOrder, Set};
    use tokio::sync::{mpsc, watch};

    use super::*;

    async fn setup() -> (DatabaseConnection, i64, i64, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::connect(format!(
            "sqlite://{}?mode=rwc",
            dir.path().join("db.sqlite").display()
        ))
        .await
        .unwrap();
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
            path: Set(Some(dir.path().display().to_string())),
            ..Default::default()
        }
        .insert(&db)
        .await
        .unwrap();
        (db, user.id, project.id, dir)
    }

    async fn queue(db: &DatabaseConnection, user: i64, project: i64) -> i64 {
        ActiveRun {
            project_id: Set(project),
            started_by: Set(Some(user)),
            configuration: Set(WorkflowConfiguration::default()),
            parameters: Set(serde_json::Value::Object(Default::default())),
            comment: Set(String::new()),
            status: Set(RunStatus::Queued),
            cancel_requested: Set(false),
            locked: Set(false),
            created_at: Set(chrono::Utc::now()),
            ..Default::default()
        }
        .insert(db)
        .await
        .unwrap()
        .id
    }

    #[tokio::test]
    async fn claim_log_finish_roundtrip() {
        let (db, user, project, dir) = setup().await;
        let source = DbJobSource::new(db.clone(), 0);
        assert!(source.claim().await.unwrap().is_none());

        let id = queue(&db, user, project).await;
        let job = source.claim().await.unwrap().unwrap();
        assert_eq!(job.run_id, id);
        assert_eq!(job.project_dir, dir.path());
        assert!(!source.cancel_requested(id).await.unwrap());

        source
            .append_logs(id, LogStream::Stdout, vec!["a".into(), "b".into()])
            .await
            .unwrap();
        source.finish(id, Outcome::Succeeded).await.unwrap();

        let run = RunTable::find_by_id(id).one(&db).await.unwrap().unwrap();
        assert_eq!(run.status, RunStatus::Succeeded);
        assert_eq!(run.exit_code, Some(0));
        let logs = RunLogTable::find()
            .filter(RunLogColumn::RunId.eq(id))
            .all(&db)
            .await
            .unwrap();
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].content, "a\nb");
    }

    #[tokio::test]
    async fn run_without_project_path_fails_and_next_is_claimed() {
        let (db, user, project, _dir) = setup().await;
        let orphan_project = ActiveProject {
            owner_id: Set(Some(user)),
            name: Set("no path".into()),
            ..Default::default()
        }
        .insert(&db)
        .await
        .unwrap();
        let bad = queue(&db, user, orphan_project.id).await;
        let good = queue(&db, user, project).await;

        let source = DbJobSource::new(db.clone(), 0);
        let job = source.claim().await.unwrap().unwrap();
        assert_eq!(job.run_id, good);
        let bad = RunTable::find_by_id(bad).one(&db).await.unwrap().unwrap();
        assert_eq!(bad.status, RunStatus::Failed);
        assert!(bad.error.is_some());
    }

    #[tokio::test]
    async fn finishing_cancelled_run_is_not_an_error() {
        let (db, user, project, _dir) = setup().await;
        let id = queue(&db, user, project).await;
        let run = RunTable::find_by_id(id).one(&db).await.unwrap().unwrap();
        run.request_cancel(&db).await.unwrap();
        let source = DbJobSource::new(db.clone(), 0);
        source.finish(id, Outcome::Succeeded).await.unwrap();
        let run = RunTable::find_by_id(id).one(&db).await.unwrap().unwrap();
        assert_eq!(run.status, RunStatus::Cancelled);
    }

    /// Emits two lines, then reports success
    struct EchoExecutor;

    impl Executor for EchoExecutor {
        fn engine(&self) -> macworp_core::workflow_configuration::engine::Engine {
            WorkflowConfiguration::default().engine().clone()
        }

        fn run<'a>(
            &'a self,
            _job: &'a Job,
            logs: mpsc::Sender<LogLine>,
            _cancel: watch::Receiver<bool>,
        ) -> BoxFuture<'a, Outcome> {
            Box::pin(async move {
                for line in ["one", "two"] {
                    logs.send(LogLine {
                        stream: LogStream::Stdout,
                        line: line.into(),
                    })
                    .await
                    .unwrap();
                }
                Outcome::Succeeded
            })
        }
    }

    #[tokio::test]
    async fn worker_runs_queued_run_to_completion() {
        let (db, user, project, _dir) = setup().await;
        let id = queue(&db, user, project).await;
        let worker = Worker::new(
            Arc::new(DbJobSource::new(db.clone(), 0)),
            vec![Arc::new(EchoExecutor) as Arc<dyn Executor>],
            WorkerOptions {
                idle_interval: Duration::from_millis(20),
                ..Default::default()
            },
        );
        assert!(worker.run_next().await.unwrap());
        assert!(!worker.run_next().await.unwrap());

        let run = RunTable::find_by_id(id).one(&db).await.unwrap().unwrap();
        assert_eq!(run.status, RunStatus::Succeeded);
        let logs = RunLogTable::find()
            .filter(RunLogColumn::RunId.eq(id))
            .order_by_asc(RunLogColumn::Seq)
            .all(&db)
            .await
            .unwrap();
        let text: Vec<&str> = logs.iter().flat_map(|l| l.content.split('\n')).collect();
        assert_eq!(text, ["one", "two"]);
    }
}
