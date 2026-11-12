use std::sync::Arc;

use sea_orm::{Database, DatabaseConnection};

use macworp_core::workflow_configuration::engine::Engine;

use crate::{
    configuration::Configuration, error::Error, reference_directory::ReferenceDirectory,
    repository_cache::RepositoryCache, worker_registry::WorkerRegistry,
};

pub struct State {
    config: Configuration,
    db: DatabaseConnection,
    workers: WorkerRegistry,
    reference_directory: ReferenceDirectory,
    repository_cache: Arc<RepositoryCache>,
}

impl State {
    async fn new(config: Configuration, db: DatabaseConnection) -> Result<Self, Error> {
        let reference_directory = ReferenceDirectory::below(&config.storage_directory());
        let repository_cache = Arc::new(RepositoryCache::new(
            config.cache_path(),
            std::time::Duration::from_secs(config.branch_cache_seconds()),
        ));
        Ok(Self {
            config,
            db,
            workers: WorkerRegistry::default(),
            reference_directory,
            repository_cache,
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

    /// Cache of remote workflow repositories.
    pub fn repository_cache(&self) -> Arc<RepositoryCache> {
        self.repository_cache.clone()
    }

    /// Replaces the repository cache by one the embedding application keeps, so it can move
    /// the cache while the backend runs.
    ///
    /// # Arguments
    /// * `repository_cache` - The handle to use from now on
    pub fn with_repository_cache(mut self, repository_cache: Arc<RepositoryCache>) -> Self {
        self.repository_cache = repository_cache;
        self
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

    /// Deletes cached checkouts unused for `checkout_max_age_days` that no queued or running
    /// run points to. Returns how many were removed; 0 when pruning is disabled.
    pub async fn prune_repository_cache(&self) -> Result<usize, Error> {
        let Some(max_age) = self.config.checkout_max_age() else {
            return Ok(0);
        };
        let in_use = crate::models::RunTable::active_source_locations(
            &self.db,
            &self.config.storage_directory(),
        )
        .await
        .map_err(|e| Error::CachePrune(e.to_string()))?;
        let cache = self.repository_cache();
        let removed = tokio::task::spawn_blocking(move || cache.prune(max_age, &in_use))
            .await
            .map_err(|e| Error::CachePrune(e.to_string()))?
            .map_err(|e| Error::CachePrune(e.to_string()))?;
        Ok(removed.len())
    }

    /// Whether the policy `permit_local_workflows` lets `user` create local workflows. The desktop app
    /// (`allow_local_paths`, one local user) always may.
    ///
    /// # Arguments
    /// * `user` - The acting user
    pub fn may_create_local_workflows(&self, user: &crate::models::User) -> bool {
        self.config.allow_local_paths()
            || self.config.permit_local_workflows().permits(user.is_admin)
    }

    /// On servers (no `allow_local_paths`) a local workflow always lives in
    /// `<storage>/local/<id>`: replaces the location of a local source by the relative
    /// `local/<id>` (resolved against each machine's storage mount) and keeps the script. Idempotent; other sources are untouched. Call wherever a stored configuration is
    /// saved, run or read, so a client can never point a workflow at another directory.
    ///
    /// # Arguments
    /// * `workflow_integration_id` - Id of the workflow the configuration belongs to
    /// * `configuration` - Configuration to adjust in place
    pub fn confine_local_source(
        &self,
        workflow_integration_id: i64,
        configuration: &mut macworp_core::workflow_configuration::WorkflowConfiguration,
    ) {
        use macworp_core::workflow_configuration::source::Source;
        if self.config.allow_local_paths() {
            return;
        }
        if let Source::Local(local) = configuration.source_mut() {
            *local.location_mut() = self.config.local_workflow_location(workflow_integration_id);
        }
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
