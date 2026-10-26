use std::{fmt::Display, path::PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{
    entities::{LogStream, RunStatus},
    workflow_configuration::WorkflowConfiguration,
};

/// Response of login
///
#[derive(Debug, Serialize, Deserialize)]
pub struct UserResponse {
    pub id: i64,
    pub username: String,
    pub firstname: String,
    pub lastname: String,
    pub email: String,
}

/// Response of login
///
#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct ProjectResponse {
    pub id: i64,
    pub name: String,
    pub created_at: DateTime<Utc>,
    pub owner_username: String,
    pub current: PathBuf,
    pub files: Vec<String>,
    pub directories: Vec<String>,
    /// Project directory on the user's machine, only set when the backend runs locally (desktop)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local_path: Option<PathBuf>,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum LoginProviderResponse {
    Local(bool),
    OpenIdConnect { name: String, description: String },
}

impl LoginProviderResponse {
    pub fn description(&self) -> String {
        match self {
            LoginProviderResponse::Local(_) => "Local authentication".to_string(),
            LoginProviderResponse::OpenIdConnect { description, .. } => description.clone(),
        }
    }
}

impl Display for LoginProviderResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LoginProviderResponse::Local(_) => write!(f, "Local login"),
            LoginProviderResponse::OpenIdConnect {
                name,
                description: _,
            } => write!(f, "{name}"),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WorkflowIntegrationResponse {
    pub id: i64,
    pub owner_username: String,
    pub is_public: bool,
    pub name: String,
    pub description: String,
    pub configuration: WorkflowConfiguration,
}

impl WorkflowIntegrationResponse {
    pub fn configuration(&self) -> &WorkflowConfiguration {
        &self.configuration
    }

    pub fn configuration_mut(&mut self) -> &mut WorkflowConfiguration {
        &mut self.configuration
    }
}

/// State of a run
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RunResponse {
    pub id: i64,
    /// Project the run belongs to
    pub project_id: i64,
    /// `None` if the workflow was deleted since
    pub workflow_id: Option<i64>,
    /// Name of the workflow at start, empty if unknown
    pub workflow_name: String,
    pub status: RunStatus,
    /// Engine exit code, `None` until it exited
    pub exit_code: Option<i32>,
    /// Why the run failed when the exit code does not say, e.g. timeout
    pub error: Option<String>,
    /// Tasks the engine announced so far, grows during the run (0 if the engine reports none)
    pub tasks_total: i32,
    /// Tasks the engine finished
    pub tasks_done: i32,
    /// Latest progress detail reported by the engine
    pub progress_message: Option<String>,
    /// Comment given at start
    pub comment: String,
    /// A user asked to cancel a running run, the worker has not stopped it yet
    pub cancel_requested: bool,
    /// Locked runs are kept by the run limit
    pub locked: bool,
    /// When the run was queued
    pub created_at: DateTime<Utc>,
    /// When a worker started it, `None` while queued
    pub started_at: Option<DateTime<Utc>>,
    /// When it reached a terminal status
    pub finished_at: Option<DateTime<Utc>>,
}

/// A batch of consecutive log lines of one stream
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RunLogChunk {
    /// Position within the run, the cursor for the next request
    pub seq: i64,
    pub stream: LogStream,
    pub created_at: DateTime<Utc>,
    /// Lines joined by `\n`
    pub content: String,
    pub line_count: i32,
}

/// New log chunks of a run together with the current state
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RunLogsResponse {
    /// Chunks after the requested cursor, oldest first. More may be waiting if the page limit was hit.
    pub chunks: Vec<RunLogChunk>,
    /// The run itself, includes status and progress, so one request is enough per poll
    pub run: RunResponse,
}
