use std::{fmt::Display, path::PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{
    entities::{FailureReason, LogStream, Role, RunStatus},
    requests::ParameterValue,
    workflow_configuration::WorkflowConfiguration,
};

/// Response of login
///
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UserResponse {
    pub id: i64,
    pub username: String,
    pub firstname: String,
    pub lastname: String,
    pub email: String,
    /// Whether the user administers the instance
    #[serde(default)]
    pub is_admin: bool,
    /// Whether the account is blocked from logging in
    #[serde(default)]
    pub disabled: bool,
}

/// Response of login
///
#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct ProjectResponse {
    pub id: i64,
    pub name: String,
    pub created_at: DateTime<Utc>,
    pub owner_username: String,
    /// Nobody (no user or group) owns the project, see `Resource::ownerless_among`
    #[serde(default)]
    pub ownerless: bool,
    pub current: PathBuf,
    pub files: Vec<String>,
    pub directories: Vec<String>,
    /// The listed folder is read-only for everybody (the virtual global reference data folder),
    /// whatever `my_role` says
    #[serde(default)]
    pub read_only: bool,
    /// Project directory on the user's machine, only set when the backend runs locally (desktop)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local_path: Option<PathBuf>,
    /// Role of the requesting user, only set by endpoints returning a single project
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub my_role: Option<Role>,
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
    /// Nobody (no user or group) owns the workflow, see `Resource::ownerless_among`
    #[serde(default)]
    pub ownerless: bool,
    pub name: String,
    pub description: String,
    pub configuration: WorkflowConfiguration,
    /// Role of the requesting user, only set by endpoints returning a single workflow
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub my_role: Option<Role>,
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
    /// Machine-readable cause, only set for failed runs
    #[serde(default)]
    pub failure_reason: Option<FailureReason>,
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

/// Workflow and parameter values of the last run of a project
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LastRunResponse {
    pub workflow_id: i64,
    /// Values as submitted at start, may contain names the workflow no longer defines
    pub parameters: Vec<ParameterValue>,
}

/// A user as shown when picking share principals or group members
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct UserSummaryResponse {
    /// User id
    pub id: i64,
    /// Login name
    pub username: String,
}

/// A group with its members
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GroupResponse {
    /// Group id
    pub id: i64,
    /// Name of the group
    pub name: String,
    /// Description of the group
    pub description: String,
    /// Username of the group's owner
    pub owner_username: String,
    /// The group has no owner (its owner deleted their account)
    #[serde(default)]
    pub ownerless: bool,
    /// Whether the requesting user owns the group and may change it
    pub is_owner: bool,
    /// Members, only set by the endpoint returning a single group
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub members: Vec<UserSummaryResponse>,
}

/// Who a share is granted to
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PrincipalResponse {
    /// A single user
    User {
        /// User id
        id: i64,
        /// Login name
        username: String,
    },
    /// All members of a group
    Group {
        /// Group id
        id: i64,
        /// Group name
        name: String,
    },
    /// Every user
    Public,
}

/// One grant of a role on a resource
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShareResponse {
    /// Who holds the role
    pub principal: PrincipalResponse,
    /// The granted role
    pub role: Role,
}

/// A run leased to a worker
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WorkerClaimResponse {
    pub run_id: i64,
    /// Project directory, valid on the worker too (shared filesystem)
    pub project_dir: PathBuf,
    /// Global reference data directory, valid on the worker too (shared filesystem); `None` for
    /// an older backend
    #[serde(default)]
    pub reference_dir: Option<PathBuf>,
    /// Workflow configuration snapshot with the parameter values applied
    pub configuration: WorkflowConfiguration,
    /// Per-run token for all further calls, sent as `Authorization: Run <token>`
    pub token: String,
    /// Seconds the worker may stay silent; heartbeat several times within it
    pub lease_seconds: u32,
}

/// Answer to a heartbeat
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkerHeartbeatResponse {
    /// A user asked to stop the run, the worker should cancel it and report `Cancelled`
    pub cancel_requested: bool,
}

/// Answer to an announce
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkerAnnounceResponse {
    /// Seconds the announcement stays valid, the worker should announce again well within it
    pub ttl_seconds: u32,
    /// File the worker must be able to read to prove it shares the backend's project storage,
    /// `None` if the backend has no shared storage requirement (desktop)
    #[serde(default)]
    pub storage: Option<StorageProbe>,
}

/// A marker file on the shared project storage. The backend keeps it with a random content; a
/// worker that reads the same content at the same path sees the same filesystem.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StorageProbe {
    /// Absolute path of the marker file, valid on the backend and on every worker
    pub path: PathBuf,
    /// Content the file must have
    pub content: String,
}

/// Engines no live worker can run
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnginesResponse {
    pub unavailable: Vec<crate::workflow_configuration::engine::Engine>,
    /// Workers with a valid announcement
    pub workers_online: usize,
}
