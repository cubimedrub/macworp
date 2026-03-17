use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("Failed to connect to database: {0}")]
    DatabaseConnection(sea_orm::DbErr),
    #[error("Unable to migrate database: {0}")]
    DatabaseMigration(sea_orm::DbErr),
    #[error("Unable to start backend: {0}")]
    BackendStart(std::io::Error),
}
