use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("Failed to create app directory, error: {0}")]
    AppDirectoryCreation(std::io::Error),
    #[error("{0}")]
    Backend(#[from] macworp_backend::error::Error),
    #[error("Failed to get address from backend socket, error: {0}")]
    BackendAddressNotFound(std::io::Error),
    #[error("Failed to send stop signal to backend.")]
    BackendStopSignal,
    #[error("Stopping backend went wronf, {0}")]
    BackendStopping(tokio::task::JoinError),
    #[error("Backend stopped unexpectedly.")]
    BackendStoppedUnexpectedly,
    #[error("{0}")]
    Core(#[from] macworp_core::error::Error),
    #[error("Login failed: {0}")]
    LoginFailed(macworp_ui_core::client::Error),
    #[error("Unable to read password file: {0}")]
    PasswordFileRead(std::io::Error),
    #[error("Unable to write password file: {0}")]
    PasswordFileWrite(std::io::Error),
    #[error("Password file is a directory")]
    PasswordFileIsDirectory,
    #[error("Unable to read secret file: {0}")]
    SecretFileRead(std::io::Error),
    #[error("Unable to write secret file: {0}")]
    SecretFileWrite(std::io::Error),
    #[error("Failed to get project directory")]
    ProjectDirectoryNotFound,
    #[error(transparent)]
    Storage(#[from] crate::storage::Error),
}
