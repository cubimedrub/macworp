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
            LoginControllerError::Authentication(e) => e.into_response(),
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
            UsersControllerError::Authentication(e) => e.into_response(),
            _ => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
        }
    }
}

#[derive(Error, Debug)]
pub enum ProjectsControllerError {
    #[error("{0}")]
    Database(#[from] sea_orm::DbErr),
    #[error("{0}")]
    Authentication(#[from] crate::authentication::error::Error),
    #[error("Project not found")]
    ProjectNotFound,
    #[error("Access to this project is forbidden")]
    Forbidden,
    #[error("Errors while accessing filesystem: {0}")]
    Filesystem(#[from] crate::filesystems::errors::Error),
    #[error("Missing Content-Range header")]
    MissingContentRangeHeader,
    #[error("Invalid Content-Range header: {0}")]
    InvalidContentRangeHeader(macworp_core::http::headers::ContentRangeHeaderValueError),
}

impl IntoResponse for ProjectsControllerError {
    fn into_response(self) -> axum::response::Response {
        tracing::error!("{self}");
        match self {
            ProjectsControllerError::Forbidden => {
                (StatusCode::FORBIDDEN, self.to_string()).into_response()
            }
            ProjectsControllerError::ProjectNotFound => {
                (StatusCode::NOT_FOUND, self.to_string()).into_response()
            }
            ProjectsControllerError::Authentication(e) => e.into_response(),
            _ => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
        }
    }
}

#[derive(Error, Debug)]
pub enum WorkflowIntegrationsControllerError {
    #[error("{0}")]
    Database(#[from] sea_orm::DbErr),
    #[error("Access to this workflow integration is not allowed")]
    Forbidden,
    #[error("Workflow integration not found")]
    WorkflowIntegrationNotFound,
}

impl IntoResponse for WorkflowIntegrationsControllerError {
    fn into_response(self) -> axum::response::Response {
        tracing::error!("{self}");
        match self {
            WorkflowIntegrationsControllerError::Forbidden => {
                (http::StatusCode::FORBIDDEN, self.to_string()).into_response()
            }
            WorkflowIntegrationsControllerError::WorkflowIntegrationNotFound => {
                (http::StatusCode::NOT_FOUND, self.to_string()).into_response()
            }
            _ => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
        }
    }
}
