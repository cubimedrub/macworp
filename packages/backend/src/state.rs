use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use sea_orm::{Database, DatabaseConnection, EntityTrait};

use macworp_core::responses::PrefetchResponse;
use macworp_core::workflow_configuration::{WorkflowConfiguration, engine::Engine};

use crate::{
    authentication::one_time_token::OneTimeTokens,
    configuration::{Configuration, WORKFLOWS_DIRECTORY_NAME},
    error::Error,
    nf_core_index::NfCoreIndex,
    reference_directory::ReferenceDirectory,
    reference_snapshots::ReferenceSnapshots,
    worker_registry::WorkerRegistry,
    workflow_repository_cache::WorkflowRepositoryCache,
};

/// Name of the folder below the storage root holding the per-run reference snapshots.
pub const REFERENCE_SNAPSHOTS_DIRECTORY_NAME: &str = "reference_snapshots";

pub struct State {
    config: Configuration,
    db: DatabaseConnection,
    workers: WorkerRegistry,
    reference_directory: ReferenceDirectory,
    /// Per-run snapshots of the reference data, `None` where the app does not write the
    /// reference data itself (desktop: the user's own folder, runs read it directly)
    reference_snapshots: Option<ReferenceSnapshots>,
    workflow_repository_cache: Arc<WorkflowRepositoryCache>,
    nf_core_index: Arc<NfCoreIndex>,
    /// Latest background download per workflow, with a sequence number so a slow older
    /// download cannot overwrite the result of a newer one
    prefetches: Mutex<HashMap<i64, (u64, PrefetchResponse)>>,
    /// Tokens that authenticate one request made by the browser itself (downloads)
    one_time_tokens: OneTimeTokens,
}

impl State {
    async fn new(config: Configuration, db: DatabaseConnection) -> Result<Self, Error> {
        let reference_directory = ReferenceDirectory::below(&config.storage_directory());
        let reference_snapshots = Some(ReferenceSnapshots::new(
            reference_directory.clone(),
            config
                .storage_directory()
                .join(REFERENCE_SNAPSHOTS_DIRECTORY_NAME),
        ));
        let workflow_repository_cache = Arc::new(WorkflowRepositoryCache::new(
            config.cache_directory(),
            std::time::Duration::from_secs(config.branch_cache_seconds()),
        ));
        let nf_core_index = Arc::new(NfCoreIndex::new(
            config.nf_core_index_url().to_string(),
            std::time::Duration::from_secs(config.nf_core_index_cache_seconds()),
            config.storage_directory().join(WORKFLOWS_DIRECTORY_NAME),
        ));
        Ok(Self {
            config,
            db,
            workers: WorkerRegistry::default(),
            reference_directory,
            reference_snapshots,
            workflow_repository_cache,
            nf_core_index,
            prefetches: Mutex::default(),
            one_time_tokens: OneTimeTokens::default(),
        })
    }

    pub fn config(&self) -> &Configuration {
        &self.config
    }

    /// The store of one-time tokens, see [`OneTimeTokens`].
    pub fn one_time_tokens(&self) -> &OneTimeTokens {
        &self.one_time_tokens
    }

    pub fn db(&self) -> &DatabaseConnection {
        &self.db
    }

    /// Location of the global reference data, shared with the embedding application
    pub fn reference_directory(&self) -> &ReferenceDirectory {
        &self.reference_directory
    }

    /// Cache of remote workflow repositories.
    pub fn workflow_repository_cache(&self) -> Arc<WorkflowRepositoryCache> {
        self.workflow_repository_cache.clone()
    }

    /// The public nf-core pipeline index.
    pub fn nf_core_index(&self) -> Arc<NfCoreIndex> {
        self.nf_core_index.clone()
    }

    /// Replaces the nf-core index access (tests point it at a local server).
    ///
    /// # Arguments
    /// * `nf_core_index` - The access to use from now on
    pub fn with_nf_core_index(mut self, nf_core_index: Arc<NfCoreIndex>) -> Self {
        self.nf_core_index = nf_core_index;
        self
    }

    /// Starts downloading the remote repository of a saved workflow into the cache in the
    /// background, so errors show in the editor and the first run needs no network. Returns
    /// immediately; failures (unreachable host, unknown reference) are only recorded, never
    /// block saving. Does nothing for sources that are not remote.
    ///
    /// # Arguments
    /// * `workflow_id` - The saved workflow
    /// * `configuration` - Its configuration as saved
    pub fn prefetch_workflow(
        self: &Arc<Self>,
        workflow_id: i64,
        configuration: WorkflowConfiguration,
    ) {
        if !configuration.source().is_remote() && !configuration.source().is_nf_core() {
            self.prefetches
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .remove(&workflow_id);
            return;
        }
        let sequence = {
            let mut prefetches = self.prefetches.lock().unwrap_or_else(|e| e.into_inner());
            let sequence = prefetches.get(&workflow_id).map_or(1, |(n, _)| n + 1);
            prefetches.insert(workflow_id, (sequence, PrefetchResponse::Fetching));
            sequence
        };
        let state = self.clone();
        let cache = self.workflow_repository_cache();
        tokio::spawn(async move {
            let result = tokio::task::spawn_blocking(move || cache.prefetch(&configuration)).await;
            let outcome = match result {
                Ok(Ok(Some(resolved))) => PrefetchResponse::Done {
                    commit: resolved.sha,
                },
                Ok(Ok(None)) => PrefetchResponse::Idle,
                Ok(Err(error)) => {
                    tracing::warn!(%error, workflow_id, "prefetching the workflow repository failed");
                    PrefetchResponse::Failed {
                        message: error.to_string(),
                    }
                }
                Err(error) => PrefetchResponse::Failed {
                    message: error.to_string(),
                },
            };
            let mut prefetches = state.prefetches.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(entry) = prefetches.get_mut(&workflow_id)
                && entry.0 == sequence
            {
                entry.1 = outcome;
            }
        });
    }

    /// Forgets the background download state of a deleted workflow.
    ///
    /// # Arguments
    /// * `workflow_id` - The deleted workflow
    pub fn forget_prefetch(&self, workflow_id: i64) {
        self.prefetches
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&workflow_id);
    }

    /// Current state of the background download of a workflow's repository.
    ///
    /// # Arguments
    /// * `workflow_id` - The workflow
    pub fn prefetch_status(&self, workflow_id: i64) -> PrefetchResponse {
        self.prefetches
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&workflow_id)
            .map(|(_, status)| status.clone())
            .unwrap_or_default()
    }

    /// Replaces the repository cache by one the embedding application keeps, so it can move
    /// the cache while the backend runs.
    ///
    /// # Arguments
    /// * `workflow_repository_cache` - The handle to use from now on
    pub fn with_workflow_repository_cache(
        mut self,
        workflow_repository_cache: Arc<WorkflowRepositoryCache>,
    ) -> Self {
        self.workflow_repository_cache = workflow_repository_cache;
        self
    }

    /// Replaces the reference directory handle by one the embedding application keeps, so it
    /// can move the reference data while the backend runs.
    ///
    /// # Arguments
    /// * `reference_directory` - The handle to use from now on
    pub fn with_reference_directory(mut self, reference_directory: ReferenceDirectory) -> Self {
        self.reference_snapshots = self.reference_snapshots.map(|snapshots| {
            ReferenceSnapshots::new(reference_directory.clone(), snapshots.root().to_path_buf())
        });
        self.reference_directory = reference_directory;
        self
    }

    /// Turns per-run reference snapshots on (the standalone default) for a state that has none,
    /// stored below `root`. Used where the integration state needs them, e.g. in tests.
    ///
    /// # Arguments
    /// * `root` - Folder for the snapshots, must be on the filesystem of the reference data
    pub fn with_reference_snapshots(mut self, root: PathBuf) -> Self {
        self.reference_snapshots = Some(ReferenceSnapshots::new(
            self.reference_directory.clone(),
            root,
        ));
        self
    }

    /// The per-run snapshots of the reference data, `None` where runs read the live folder.
    pub fn reference_snapshots(&self) -> Option<&ReferenceSnapshots> {
        self.reference_snapshots.as_ref()
    }

    /// Reference data directory a run reads: its snapshot, or the live folder for runs without
    /// one (embedded desktop worker, runs queued before snapshots existed).
    ///
    /// # Arguments
    /// * `run` - The run being handed to a worker
    pub fn reference_directory_of(&self, run: &crate::models::Run) -> PathBuf {
        match (&self.reference_snapshots, &run.reference_snapshot) {
            (Some(snapshots), Some(name)) => snapshots.path_of(name),
            _ => self.reference_directory.get(),
        }
    }

    /// Deletes snapshots no run needs and that are older than `grace`. A run needs its snapshot
    /// while it is queued or running, and while it is the newest run of its project and failed
    /// or was cancelled, so it can be resumed
    /// ([`crate::models::RunTable::kept_reference_snapshots`]). Returns how many were removed;
    /// 0 without snapshots.
    ///
    /// # Arguments
    /// * `grace` - Minimum age of an unreferenced snapshot, covers the window between creating
    ///   a snapshot and storing its run
    pub async fn sweep_reference_snapshots(
        &self,
        grace: std::time::Duration,
    ) -> Result<usize, Error> {
        let Some(snapshots) = self.reference_snapshots.clone() else {
            return Ok(0);
        };
        let in_use = crate::models::RunTable::kept_reference_snapshots(&self.db)
            .await
            .map_err(|error| Error::ReferenceSnapshot(error.to_string()))?;
        tokio::task::spawn_blocking(move || snapshots.sweep(&in_use, grace))
            .await
            .map_err(|error| Error::ReferenceSnapshot(error.to_string()))?
            .map_err(|error| Error::ReferenceSnapshot(error.to_string()))
    }

    /// Deletes cached checkouts unused for `checkout_max_age_days` that no queued or running
    /// run points to. Returns how many were removed; 0 when pruning is disabled.
    pub async fn prune_workflow_repository_cache(&self) -> Result<usize, Error> {
        let Some(max_age) = self.config.checkout_max_age() else {
            return Ok(0);
        };
        let in_use = crate::models::RunTable::active_source_locations(
            &self.db,
            &self.config.storage_directory(),
        )
        .await
        .map_err(|e| Error::CachePrune(e.to_string()))?;
        let cache = self.workflow_repository_cache();
        let removed = tokio::task::spawn_blocking(move || cache.prune(max_age, &in_use))
            .await
            .map_err(|e| Error::CachePrune(e.to_string()))?
            .map_err(|e| Error::CachePrune(e.to_string()))?;
        Ok(removed.len())
    }

    /// Deletes the old versions of uploaded local workflows: of each existing workflow all but the
    /// newest, of deleted workflows all, always keeping versions that queued or running runs use.
    /// Returns how many versions were removed.
    pub async fn prune_local_workflow_versions(&self) -> Result<usize, Error> {
        let ids: std::collections::HashSet<i64> = crate::models::WorkflowIntegrationTable::find()
            .all(&self.db)
            .await
            .map_err(|e| Error::WorkflowVersionPrune(e.to_string()))?
            .into_iter()
            .map(|workflow| workflow.id)
            .collect();
        let in_use = crate::models::RunTable::active_source_locations(
            &self.db,
            &self.config.storage_directory(),
        )
        .await
        .map_err(|e| Error::WorkflowVersionPrune(e.to_string()))?;
        let root = self.config.local_workflows_directory();
        tokio::task::spawn_blocking(move || {
            let Ok(entries) = std::fs::read_dir(&root) else {
                return 0;
            };
            let mut removed = 0;
            for entry in entries.flatten() {
                let Ok(id) = entry.file_name().to_string_lossy().parse::<i64>() else {
                    continue;
                };
                removed += if ids.contains(&id) {
                    crate::workflow_upload::WorkflowUpload::prune(&entry.path(), &in_use)
                } else {
                    crate::workflow_upload::WorkflowUpload::remove(&entry.path(), &in_use)
                };
            }
            removed
        })
        .await
        .map_err(|e| Error::WorkflowVersionPrune(e.to_string()))
    }

    /// Like [`Self::prune_local_workflow_versions`] for one workflow, used right after an upload
    /// (`exists`) or the deletion of the workflow (`!exists`).
    ///
    /// # Arguments
    /// * `workflow_integration_id` - Id of the workflow
    /// * `exists` - `true` keeps the newest version, `false` removes everything not in use
    pub async fn prune_local_workflow(
        &self,
        workflow_integration_id: i64,
        exists: bool,
    ) -> Result<usize, Error> {
        let in_use = crate::models::RunTable::active_source_locations(
            &self.db,
            &self.config.storage_directory(),
        )
        .await
        .map_err(|e| Error::WorkflowVersionPrune(e.to_string()))?;
        let directory = self
            .config
            .local_workflow_directory(workflow_integration_id);
        tokio::task::spawn_blocking(move || {
            if exists {
                crate::workflow_upload::WorkflowUpload::prune(&directory, &in_use)
            } else {
                crate::workflow_upload::WorkflowUpload::remove(&directory, &in_use)
            }
        })
        .await
        .map_err(|e| Error::WorkflowVersionPrune(e.to_string()))
    }

    /// Updates the cached repositories of all workflows with a remote or nf-core source from
    /// their remotes (branches and the default branch; tags and commits need nothing), so a
    /// user's run or schema request finds the newest commit already checked out. Each repository
    /// and reference is fetched once however many workflows use it, one after the other; a
    /// failing repository is logged and skipped. Returns how many were fetched successfully.
    pub async fn refresh_workflow_repositories(&self) -> Result<usize, Error> {
        use macworp_core::workflow_configuration::source::Source;
        let workflows = crate::models::WorkflowIntegrationTable::find()
            .all(&self.db)
            .await
            .map_err(|e| Error::CacheRefresh(e.to_string()))?;
        let mut seen = std::collections::HashSet::new();
        let mut configurations = Vec::new();
        for workflow in workflows {
            let key = match workflow.configuration.source() {
                Source::Remote(remote) => (remote.url().to_string(), remote.version().cloned()),
                Source::NfCore(nf_core) => {
                    (nf_core.pipeline().to_string(), nf_core.version().cloned())
                }
                _ => continue,
            };
            if seen.insert(key) {
                configurations.push(workflow.configuration);
            }
        }
        let cache = self.workflow_repository_cache();
        tokio::task::spawn_blocking(move || {
            let mut refreshed = 0;
            for configuration in configurations {
                match cache.refresh(&configuration) {
                    Ok(_) => refreshed += 1,
                    Err(error) => {
                        tracing::warn!(%error, "refreshing a workflow repository failed");
                    }
                }
            }
            refreshed
        })
        .await
        .map_err(|e| Error::CacheRefresh(e.to_string()))
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
    /// `<storage>/workflows/local/<id>`: replaces the location of a local source by the relative
    /// `workflows/local/<id>` (resolved against each machine's storage mount) and keeps the script. Idempotent; other sources are untouched. Call wherever a stored configuration is
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

    /// Pins an uploaded local workflow to its newest version: points a local source of
    /// `configuration` (already confined) at `workflows/local/<id>/<version>`, so a run keeps the
    /// files it started with however often the workflow is uploaded again. Without an uploaded
    /// version (or with `allow_local_paths`) nothing changes. Call for runs and schemas, never for
    /// the configuration that is saved.
    ///
    /// # Arguments
    /// * `configuration` - Configuration to adjust in place
    pub fn pin_local_source(
        &self,
        configuration: &mut macworp_core::workflow_configuration::WorkflowConfiguration,
    ) {
        use macworp_core::workflow_configuration::source::Source;
        if self.config.allow_local_paths() {
            return;
        }
        if let Source::Local(local) = configuration.source_mut() {
            let directory = self.config.resolve_storage_path(local.location());
            if let Some(version) =
                crate::workflow_upload::WorkflowUpload::current_version(&directory)
            {
                *local.location_mut() = self.config.relative_to_storage(&version);
            }
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

        let mut state = Self::new(config, db).await?;
        // The reference folder is the user's own; the app never writes into it
        state.reference_snapshots = None;
        Ok(state)
    }
}
