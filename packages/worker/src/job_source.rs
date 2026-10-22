use macworp_core::entities::LogStream;
use thiserror::Error;

use crate::{
    container_runtime::BoxFuture,
    executors::{Job, Outcome},
};

#[derive(Debug, Error)]
#[error("{0}")]
pub struct SourceError(pub String);

/// Where a worker gets its jobs from and reports to. In-process on desktop, over HTTPS for
/// remote workers (P3), both behind this trait.
pub trait JobSource: Send + Sync {
    /// Takes the next queued run and marks it running, `None` if there is nothing to do.
    fn claim(&self) -> BoxFuture<'_, Result<Option<Job>, SourceError>>;

    /// Appends a chunk of lines of one stream to the run's log.
    fn append_logs(
        &self,
        run_id: i64,
        stream: LogStream,
        lines: Vec<String>,
    ) -> BoxFuture<'_, Result<(), SourceError>>;

    /// A user asked to stop the run.
    fn cancel_requested(&self, run_id: i64) -> BoxFuture<'_, Result<bool, SourceError>>;

    /// Moves the run to its terminal status.
    fn finish(&self, run_id: i64, outcome: Outcome) -> BoxFuture<'_, Result<(), SourceError>>;
}
