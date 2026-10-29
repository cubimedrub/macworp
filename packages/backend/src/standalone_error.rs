use thiserror::Error;

#[derive(Error, Debug)]
pub enum StandaloneError {
    #[error("Failed to read configuration file")]
    ConfigurationReadError(#[source] std::io::Error),
    #[error("No configuration file given, create one with the `config` command")]
    ConfigurationMissing,
    #[error("Failed to parse configuration file")]
    ConfigurationParseError(#[source] toml::de::Error),
    #[error("Unable to connect to database: {0}")]
    DatabaseConnection(sea_orm::DbErr),
    #[error("Failed to read password file: {0}")]
    PasswordRead(std::io::Error),
    #[error("Password confirmation does not match")]
    PasswordConfirmation,
    #[error("{0}")]
    Core(#[from] macworp_core::error::Error),
    #[error("{0}")]
    User(#[from] crate::models::user::Error),
    #[error("{0}")]
    Backend(#[from] crate::error::Error),
}
