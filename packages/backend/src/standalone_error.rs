use thiserror::Error;

#[derive(Error, Debug)]
pub enum StandaloneError {
    #[error("Failed to read configuration file")]
    ConfigurationReadError(#[source] std::io::Error),
    #[error("Failed to parse configuration file")]
    ConfigurationParseError(#[source] toml::de::Error),
    #[error("Unable to connect to database: {0}")]
    DatabaseConnection(sqlx::Error),
    #[error("{0}")]
    Core(#[from] macworp_core::error::Error),
    #[error("{0}")]
    Backend(#[from] crate::error::Error),
}
