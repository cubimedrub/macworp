use std::{fmt::Display, path::PathBuf};

use serde::{Deserialize, Serialize};

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
