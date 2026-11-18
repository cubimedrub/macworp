use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::entities::FailureReason;
use crate::workflow_configuration::{WorkflowConfiguration, engine::Engine};

/// Request for the login
///
#[derive(Debug, Serialize, Deserialize)]
pub struct PasswordLoginRequest {
    pub username: String,
    pub password: String,
}

/// Request for new project
///
#[derive(Debug, Serialize, Deserialize)]
pub struct NewProjectRequest {
    pub name: String,
    pub path: PathBuf,
}

/// Largest text file the text endpoints read or write (10 MiB).
pub const TEXT_FILE_LIMIT: usize = 10 * 1024 * 1024;

/// Body of `PUT /projects/write-text/{id}`.
#[derive(Debug, Serialize, Deserialize)]
pub struct ProjectWriteTextRequest {
    /// New file content (UTF-8), at most [`TEXT_FILE_LIMIT`] bytes
    pub content: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ProjectCreateFolderRequest {
    /// Folder path
    pub folder_path: PathBuf,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ProjectPathDeletionRequest {
    /// Target path to delete
    pub path: PathBuf,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct NewWorkflowIntegrationRequest {
    /// Name of the new integration
    pub name: String,
    /// Descriptio
    pub description: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateWorkflowIntegrationRequest {
    /// Name of the integration
    pub name: String,
    /// Description of the integration
    pub description: String,
    /// Workflow configuration
    pub configuration: WorkflowConfiguration,
}

/// Value for one dynamic parameter of a workflow, matched by parameter name
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ParameterValue {
    /// Name of the dynamic parameter
    pub name: String,
    /// Text/Glob: string, Integer/Float: number, Path: array of strings (or one string); `null` unsets
    pub value: serde_json::Value,
}

/// Request to start a workflow on a project.
///
/// Only values of dynamic parameters are taken from the client. Engine, source, engine
/// parameters and fixed parameters always come from the stored workflow.
#[derive(Debug, Serialize, Deserialize)]
pub struct WorkflowStartRequest {
    /// Workflow integration ID
    pub workflow_id: i64,
    /// Optional comment for the workflow run
    pub comment: String,
    /// Values for the dynamic parameters
    pub parameters: Vec<ParameterValue>,
    /// Continue this run instead of starting afresh: the new run reads the same snapshot of the
    /// global reference data and the engine's resume parameter is added. `parameters` are taken
    /// as sent, so a user can correct values (typically those of the failed run) before resuming;
    /// the engine decides what to rerun for changed inputs. Only the newest run of
    /// the project can be resumed, and only if it failed or was cancelled.
    #[serde(default)]
    pub resume_from_run: Option<i64>,
}

/// Request to create a group
#[derive(Debug, Serialize, Deserialize)]
pub struct NewGroupRequest {
    /// Name of the group
    pub name: String,
    /// Description of the group
    pub description: String,
}

/// Request to change a group's name and description
#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateGroupRequest {
    /// New name
    pub name: String,
    /// New description
    pub description: String,
}

/// Request to add or remove a group member
#[derive(Debug, Serialize, Deserialize)]
pub struct GroupMemberRequest {
    /// Id of the user
    pub user_id: i64,
}

/// Who a share is granted to, as sent by the client
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PrincipalRequest {
    /// A single user, by id
    User(i64),
    /// All members of a group, by id
    Group(i64),
    /// Every user
    Public,
}

/// Request to grant a role, replacing the principal's current role on the resource
#[derive(Debug, Serialize, Deserialize)]
pub struct GrantShareRequest {
    /// Who receives the role
    pub principal: PrincipalRequest,
    /// The role to grant
    pub role: crate::entities::Role,
}

/// Request to remove the grant of a principal
#[derive(Debug, Serialize, Deserialize)]
pub struct RevokeShareRequest {
    /// Whose grant is removed
    pub principal: PrincipalRequest,
}

/// Request of an administrator to create a user
#[derive(Debug, Serialize, Deserialize)]
pub struct NewUserRequest {
    /// Unique login name
    pub username: String,
    /// First name
    pub firstname: String,
    /// Last name
    pub lastname: String,
    /// Email address
    pub email: String,
    /// Whether the user administers the instance
    #[serde(default)]
    pub is_admin: bool,
    /// Initial local password, required: users created here are internal ones, external users are
    /// provisioned by their login provider
    pub password: String,
}

/// Request of an administrator to change a user
#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateUserRequest {
    /// First name
    pub firstname: String,
    /// Last name
    pub lastname: String,
    /// Email address
    pub email: String,
    /// Whether the user administers the instance
    pub is_admin: bool,
}

/// Request of an administrator to set a user's local password
#[derive(Debug, Serialize, Deserialize)]
pub struct ResetPasswordRequest {
    /// The new password
    pub password: String,
}

/// A worker asks for a run
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkerClaimRequest {
    /// Name of the worker process, shown to admins and used to attribute leases
    pub worker_name: String,
    /// Engines the worker can run; only runs of these engines are leased to it
    pub engines: Vec<Engine>,
}

/// Log lines a worker read from the engine
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkerLogsRequest {
    pub stream: crate::entities::LogStream,
    pub lines: Vec<String>,
}

/// Task progress of a run as the engine reported it
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkerProgressRequest {
    /// Tasks announced so far
    pub total: i32,
    /// Tasks finished
    pub done: i32,
    /// Latest detail, replaces the stored one when given
    pub message: Option<String>,
    /// Engine error report, appended to the log
    pub error: Option<String>,
}

/// How a run ended on the worker
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum WorkerOutcome {
    Succeeded,
    Failed {
        exit_code: Option<i32>,
        error: Option<String>,
        /// Why it failed; backends treat a missing value as `ExitCode`
        #[serde(default)]
        reason: Option<FailureReason>,
    },
    Cancelled,
}

/// A worker reports the end of a run
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkerFinishRequest {
    pub outcome: WorkerOutcome,
}

/// A worker announces what it can run. Sent at start and repeatedly, so the backend knows which
/// workers are alive and which engines are available.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkerAnnounceRequest {
    /// Name of the worker process, same as in `WorkerClaimRequest`
    pub worker_name: String,
    /// Container runtime binary the worker uses, e.g. `podman`
    pub runtime: String,
    pub runtime_version: Option<String>,
    /// Engines the worker can run on that runtime
    pub engines: Vec<Engine>,
}
