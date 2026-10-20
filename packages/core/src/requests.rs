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
    /// Whether the integration is public
    pub is_public: bool,
    /// Workflow configuration
    pub configuration: WorkflowConfiguration,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WorkflowStartRequest {
    /// Workflow integration ID
    pub id: i64,
    /// Optional comment for the workflow run
    pub comment: String,
    /// Workflow configuration
    pub configuration: WorkflowConfiguration,
}
