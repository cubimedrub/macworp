use std::path::PathBuf;

use serde::{Deserialize, Serialize};

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
