use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("Failed to read configuration file")]
    ConfigurationReadError(#[source] std::io::Error),
    #[error("No configuration file given, create one with the `config` command")]
    ConfigurationMissing,
    #[error("Failed to parse configuration file")]
    ConfigurationParseError(#[source] toml::de::Error),
    #[error("Unable to connect to database: {0}")]
    DatabaseConnection(sea_orm::DbErr),
    #[error("Failed to read password: {0}")]
    PasswordRead(std::io::Error),
    #[error("Password must not be empty")]
    PasswordEmpty,
    #[error("Password confirmation does not match")]
    PasswordConfirmation,
    #[error("{0}")]
    Core(#[from] macworp_core::error::Error),
    #[error("{0}")]
    User(#[from] crate::models::user::Error),
    #[error("{0}")]
    Run(#[from] crate::models::RunError),
    #[error("{0}")]
    RepositoryCache(#[from] crate::repository_cache::Error),
    /// A blocking task (cache fetch) panicked or was cancelled
    #[error("{0}")]
    Task(#[from] tokio::task::JoinError),
    #[error("{0}")]
    Backend(#[from] crate::error::Error),
}
