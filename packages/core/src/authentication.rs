use std::fmt::Display;

use thiserror::Error;

const JSON_WEB_TOKEN: &str = "JWT";
const BASIC_AUTH_TOKEN: &str = "Basic";

/// Authentication error of the core crate
#[derive(Error, Debug)]
pub enum Error {
    #[error("Invalid token type")]
    InvalidTokenType,
}

// Token types
pub enum TokenType {
    /// JWT == user authentication
    Jwt,
    /// Basic == Basic authentication for worker
    Basic,
}

impl Display for TokenType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TokenType::Jwt => write!(f, "{}", JSON_WEB_TOKEN),
            TokenType::Basic => write!(f, "{}", BASIC_AUTH_TOKEN),
        }
    }
}

impl TryFrom<&str> for TokenType {
    type Error = Error;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value.trim() {
            JSON_WEB_TOKEN => Ok(TokenType::Jwt),
            BASIC_AUTH_TOKEN => Ok(TokenType::Basic),
            _ => Err(Error::InvalidTokenType),
        }
    }
}
