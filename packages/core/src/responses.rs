use std::{fmt::Display, path::PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{entities::RunStatus, workflow_configuration::WorkflowConfiguration};

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
