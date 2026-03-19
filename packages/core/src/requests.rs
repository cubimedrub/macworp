use serde::{Deserialize, Serialize};

/// Request for the login
///
#[derive(Debug, Serialize, Deserialize)]
pub struct PasswordLoginRequest {
    pub username: String,
    pub password: String,
}
