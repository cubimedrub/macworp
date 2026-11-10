use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("Failed to connect to database: {0}")]
    DatabaseConnection(sea_orm::DbErr),
    #[error("Unable to migrate database: {0}")]
    DatabaseMigration(sea_orm::DbErr),
    #[error("Database error when {0}: {1}")]
    Database(String, sea_orm::DbErr),
    #[error("Unable to start backend: {0}")]
    BackendStart(std::io::Error),
    #[error("Unable to create the reference data directory: {0}")]
    ReferenceDirectory(std::io::Error),
    #[error("Invalid configuration: {0}")]
    Configuration(&'static str),
    #[error("Error during authentication: {0}")]
    Authentication(#[from] crate::authentication::error::Error),
}
