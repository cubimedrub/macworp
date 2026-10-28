use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::workflow_configuration::WorkflowConfiguration;

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
