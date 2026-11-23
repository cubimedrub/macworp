use std::fmt::Display;

use thiserror::Error;

const JSON_WEB_TOKEN: &str = "JWT";
const BASIC_AUTH_TOKEN: &str = "Basic";
const RUN_TOKEN: &str = "Run";

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
    /// Run == per-run token a worker received when it claimed the run
    Run,
}

impl Display for TokenType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TokenType::Jwt => write!(f, "{}", JSON_WEB_TOKEN),
            TokenType::Basic => write!(f, "{}", BASIC_AUTH_TOKEN),
            TokenType::Run => write!(f, "{}", RUN_TOKEN),
        }
    }
}

impl TryFrom<&str> for TokenType {
    type Error = Error;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value.trim() {
            JSON_WEB_TOKEN => Ok(TokenType::Jwt),
            BASIC_AUTH_TOKEN => Ok(TokenType::Basic),
            RUN_TOKEN => Ok(TokenType::Run),
            _ => Err(Error::InvalidTokenType),
        }
    }
}
