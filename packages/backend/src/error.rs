use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("Failed to connect to database: {0}")]
    DatabaseConnection(sqlx::Error),
    #[error("Unable to migrate database: {0}")]
    DatabaseMigration(sqlx::migrate::MigrateError),
    #[error("Unable to start backend: {0}")]
    BackendStart(std::io::Error),
}
