use std::path::PathBuf;

use macworp_core::{entities::LogStream, workflow_configuration::WorkflowConfiguration};

/// A run claimed by a worker.
#[derive(Debug, Clone)]
pub struct Job {
    pub run_id: i64,
    /// Project data directory, the only place the workflow may write to. Mounted at the same
    /// path inside the engine container so mounts the engine creates resolve on the host.
    pub project_dir: PathBuf,
    /// Workflow configuration snapshot with the user's parameter values already applied
    pub configuration: WorkflowConfiguration,
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
    },
    Cancelled,
}
