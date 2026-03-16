use sqlx::{Database, Pool};

use crate::{configuration::Configuration, error::Error};

pub struct State<T: Database> {
    config: Configuration,
    db: Pool<T>,
}

impl<T> State<T>
where
    T: Database,
{
    async fn new(config: Configuration, db: Pool<T>) -> Result<Self, Error> {
        Ok(Self { config, db })
    }

    pub fn config(&self) -> &Configuration {
        &self.config
    }

    pub fn db(&self) -> &Pool<T> {
        &self.db
    }
}

#[cfg(feature = "standalone")]
impl State<sqlx::Postgres> {
    pub async fn new_for_standalone(config: Configuration) -> Result<Self, Error> {
        let db = Pool::connect(config.database_url())
            .await
            .map_err(Error::DatabaseConnection)?;

        Self::new(config, db).await
    }
}

#[cfg(feature = "integration")]
impl State<sqlx::Sqlite> {
    pub async fn new_for_integration(config: Configuration) -> Result<Self, Error> {
        use std::str::FromStr;

        use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode};

        let options = SqliteConnectOptions::from_str(config.database_url())
            .unwrap()
            .journal_mode(SqliteJournalMode::Wal)
            .read_only(false)
            .create_if_missing(true);

        let db = Pool::connect_with(options)
            .await
            .map_err(Error::DatabaseConnection)?;

        Self::new(config, db).await
    }
}
