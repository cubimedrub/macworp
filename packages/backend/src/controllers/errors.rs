use axum::{http::StatusCode, response::IntoResponse};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum LoginControllerError {
    #[error("{0}")]
    Database(#[from] sea_orm::DbErr),
    #[error("{0}")]
    Authentication(#[from] crate::authentication::error::Error),
}

impl IntoResponse for LoginControllerError {
    fn into_response(self) -> axum::response::Response {
        tracing::error!("{self}");
        match self {
            LoginControllerError::Authentication(_) => self.into_response(),
            _ => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
        }
    }
}

#[derive(Error, Debug)]
pub enum UsersControllerError {
    #[error("{0}")]
    Database(#[from] sea_orm::DbErr),
    #[error("{0}")]
    Authentication(#[from] crate::authentication::error::Error),
}

impl IntoResponse for UsersControllerError {
    fn into_response(self) -> axum::response::Response {
        tracing::error!("{self}");
        match self {
            UsersControllerError::Authentication(_) => self.into_response(),
            _ => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
        }
    }
}
