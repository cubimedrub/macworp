use std::path::PathBuf;

use macworp_core::{
    entities::{FailureReason, LogStream},
    workflow_configuration::WorkflowConfiguration,
};

use crate::log_proxy::ProgressEndpoint;

/// A run claimed by a worker.
#[derive(Debug, Clone)]
pub struct Job {
    pub run_id: i64,
    /// Project data directory, the only place the workflow may write to. Mounted at the same
    /// path inside the engine container so mounts the engine creates resolve on the host.
    pub project_dir: PathBuf,
    /// Global reference data, mounted read-only at the same path. `Path` values below the
    /// reference prefix are rewritten onto it. `None` where the instance has none.
    pub reference_dir: Option<PathBuf>,
    /// Image cache shared by all runs, mounted read-write at the same path for engines that pull
    /// images themselves (Snakemake with Apptainer). `None` where there is none.
    pub apptainer_cache: Option<PathBuf>,
    /// Workflow configuration snapshot with the user's parameter values already applied
    pub configuration: WorkflowConfiguration,
    /// Where the engine reports its progress, set by the worker for engines that do
    pub progress: Option<ProgressEndpoint>,
}

impl Job {
    /// Resolves the paths of a job claimed from a backend against this machine's storage mount:
    /// the project directory, the reference directory and a relative location of a local
    /// source are relative to the storage root, absolute paths stay.
    ///
    /// # Arguments
    /// * `storage` - Where this machine has the shared storage mounted, absolute
    pub fn resolve_storage(&mut self, storage: &std::path::Path) {
        use macworp_core::{paths::resolve_in_storage, workflow_configuration::source::Source};
        self.project_dir = resolve_in_storage(storage, &self.project_dir);
        self.reference_dir = self
            .reference_dir
            .take()
            .map(|path| resolve_in_storage(storage, &path));
        if let Source::Local(local) = self.configuration.source_mut() {
            let resolved = resolve_in_storage(storage, local.location());
            *local.location_mut() = resolved;
        }
    }
}

/// One line of engine output or a MAcWorP message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogLine {
    pub stream: LogStream,
    pub line: String,
}

/// How an execution ended. A run that did not succeed is never reported as `Succeeded`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Succeeded,
    Failed {
        exit_code: Option<i32>,
        error: Option<String>,
        /// Why it failed, `None` is treated as `ExitCode` by the backend
        reason: Option<FailureReason>,
    },
    Cancelled,
}
