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
#[derive(Debug, Serialize, Deserialize)]
pub struct ProjectResponse {
    pub id: i64,
    pub name: String,
    pub owner_username: String,
    pub files: Vec<String>,
    pub directories: Vec<String>,
}
