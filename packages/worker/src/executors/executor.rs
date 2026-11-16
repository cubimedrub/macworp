use macworp_core::workflow_configuration::{engine::Engine, render::Error as RenderError};
use thiserror::Error;
use tokio::sync::{mpsc, watch};

use super::{Job, LogLine, Outcome};
use crate::container_runtime::{BoxFuture, RuntimeKind};

#[derive(Debug, Error)]
pub enum Error {
    #[error("Cannot render workflow parameters: {0}")]
    Render(#[from] RenderError),
    #[error("Workflow source is not supported by this executor: {0}")]
    UnsupportedSource(String),
}

/// Runs jobs of one workflow engine.
pub trait Executor: Send + Sync {
    fn engine(&self) -> Engine;

    /// The container runtime the executor starts its containers with, `None` if unknown
    /// (the progress receiver then uses its default host access).
    fn runtime_kind(&self) -> Option<RuntimeKind> {
        None
    }

    /// Runs the job to its end. Engine output goes to `logs`; the run is stopped when `cancel`
    /// turns `true`. Failures to even start the engine are reported as `Outcome::Failed`.
    fn run<'a>(
        &'a self,
        job: &'a Job,
        logs: mpsc::Sender<LogLine>,
        cancel: watch::Receiver<bool>,
    ) -> BoxFuture<'a, Outcome>;
}
