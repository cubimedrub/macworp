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
