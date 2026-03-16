use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("Failed to create app directory, error: {0}")]
    AppDirectoryCreation(std::io::Error),
    #[error("Failed to get address from backend socket, error: {0}")]
    BackendAddressNotFound(std::io::Error),
    #[error("Failed to send stop signal to backend.")]
    BackendStopSignal,
    #[error("Stopping backend went wronf, {0}")]
    BackendStopping(tokio::task::JoinError),
    #[error("Backend stopped unexpectedly.")]
    BackendStoppedUnexpectedly,
    #[error("Failed to get project directory")]
    ProjectDirectoryNotFound,
    #[error("{0}")]
    Core(#[from] macworp_core::error::Error),
    #[error("{0}")]
    Backend(#[from] macworp_backend::error::Error),
}
