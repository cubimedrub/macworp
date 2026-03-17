use sea_orm::{Database, DatabaseConnection};

use crate::{configuration::Configuration, error::Error};

pub struct State {
    config: Configuration,
    db: DatabaseConnection,
}

impl State {
    async fn new(config: Configuration, db: DatabaseConnection) -> Result<Self, Error> {
        Ok(Self { config, db })
    }

    pub fn config(&self) -> &Configuration {
        &self.config
    }

    pub fn db(&self) -> &DatabaseConnection {
        &self.db
    }
}

#[cfg(feature = "standalone")]
impl State {
    pub async fn new_for_standalone(config: Configuration) -> Result<Self, Error> {
        let db = Database::connect(config.database_url())
            .await
            .map_err(Error::DatabaseConnection)?;

        Self::new(config, db).await
    }
}

#[cfg(feature = "integration")]
impl State {
    pub async fn new_for_integration(config: Configuration) -> Result<Self, Error> {
        use sea_orm::ConnectOptions;
        use sqlx::sqlite::SqliteJournalMode;

        let mut connect_options = ConnectOptions::new(config.database_url());

        connect_options.map_sqlx_sqlite_opts(|sqlite_options| {
            sqlite_options
                .journal_mode(SqliteJournalMode::Wal)
                .read_only(false)
                .create_if_missing(true)
        });

        let db = Database::connect(connect_options)
            .await
            .map_err(Error::DatabaseConnection)?;

        Self::new(config, db).await
    }
}
