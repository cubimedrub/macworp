use axum::{http::StatusCode, response::IntoResponse};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error(transparent)]
    CoreAuthorization(#[from] macworp_core::authentication::Error),
    #[error("Unabel to decode basic authentication token")]
    BasicAuthDecode(base64::DecodeError),
    #[error("Database error: {0}")]
    Database(#[from] sea_orm::DbErr),
    #[error("Session expired, please login again")]
    ExpiredSession,
    #[error("Invalid authorization header values, {0}")]
    InvalidAuthorizationHeader(&'static str),
    #[error("User has invalid password hash {0}: {1}")]
    InvalidPasswordHash(i64, password_auth::ParseError),
    #[error("Authentication token is not valid")]
    InvalidTokenSubject,
    #[error("Invalid user or password")]
    InvalidUserOrPassword,
    #[error("JWT encryption failed: {0}")]
    JWTEncryption(jsonwebtoken::errors::Error),
    #[error("JWT decryption failed: {0}")]
    JWTDecryption(jsonwebtoken::errors::Error),
    #[error("Missing authorization header")]
    MissingAuthorizationHeader,
    #[error("Authentication token is not valid")]
    Unauthorized,
    #[error("Administrator rights required")]
    AdminRequired,
    /// The account was disabled by an administrator.
    #[error("User account is disabled")]
    AccountDisabled,
    #[error("Invalid query parameters: {0}")]
    InvalidQuery(axum::extract::rejection::QueryRejection),
    #[error("State missing: {0}")]
    StateMissing(axum::extract::rejection::ExtensionRejection),
}

impl IntoResponse for Error {
    fn into_response(self) -> axum::response::Response {
        tracing::error!("{self}");
        match self {
            Error::CoreAuthorization(_)
            | Error::InvalidAuthorizationHeader(_)
            | Error::InvalidQuery(_)
            | Error::MissingAuthorizationHeader => {
                (StatusCode::BAD_REQUEST, self.to_string()).into_response()
            }
            Error::ExpiredSession
            | Error::InvalidTokenSubject
            | Error::InvalidUserOrPassword
            | Error::AccountDisabled
            | Error::Unauthorized => (StatusCode::UNAUTHORIZED, self.to_string()).into_response(),
            Error::AdminRequired => (StatusCode::FORBIDDEN, self.to_string()).into_response(),
            _ => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
        }
    }
}
