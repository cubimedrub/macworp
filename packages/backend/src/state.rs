use sea_orm::{Database, DatabaseConnection};

use macworp_core::workflow_configuration::engine::Engine;

use crate::{
    configuration::Configuration, error::Error, reference_directory::ReferenceDirectory,
    worker_registry::WorkerRegistry,
};

pub struct State {
    config: Configuration,
    db: DatabaseConnection,
    workers: WorkerRegistry,
    reference_directory: ReferenceDirectory,
}

impl State {
    async fn new(config: Configuration, db: DatabaseConnection) -> Result<Self, Error> {
        let reference_directory =
            ReferenceDirectory::below(std::path::Path::new(config.projects_path()));
        Ok(Self {
            config,
            db,
            workers: WorkerRegistry::default(),
            reference_directory,
        })
    }

    pub fn config(&self) -> &Configuration {
        &self.config
    }

    pub fn db(&self) -> &DatabaseConnection {
        &self.db
    }

    /// Location of the global reference data, shared with the embedding application
    pub fn reference_directory(&self) -> &ReferenceDirectory {
        &self.reference_directory
    }

    /// Replaces the reference directory handle by one the embedding application keeps, so it
    /// can move the reference data while the backend runs.
    ///
    /// # Arguments
    /// * `reference_directory` - The handle to use from now on
    pub fn with_reference_directory(mut self, reference_directory: ReferenceDirectory) -> Self {
        self.reference_directory = reference_directory;
        self
    }

    /// Workers that announced themselves
    pub fn workers(&self) -> &WorkerRegistry {
        &self.workers
    }

    /// Engines runs are rejected for: the list the embedding application fixed (desktop), else
    /// every engine no live worker can run (all of them without a live worker).
    pub fn unavailable_engines(&self) -> Vec<Engine> {
        if let Some(fixed) = self.config.fixed_unavailable_engines() {
            return fixed.to_vec();
        }
        let ttl = self.workers.ttl(&self.config);
        let available = self.workers.live_engines(std::time::Instant::now(), ttl);
        Engine::all()
            .into_iter()
            .filter(|engine| !available.contains(engine))
            .collect()
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
