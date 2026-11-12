use std::hash::{Hash, Hasher};

use macworp_core::{responses::LoginProviderResponse, workflow_configuration::engine::Engine};
use rand::{RngExt, distr::Alphanumeric};
use serde::{Deserialize, Serialize};

/// Login validity in seconds if not configured (24 hours)
pub const DEFAULT_LOGIN_VALIDITY: u32 = 24 * 60 * 60;

/// Runs kept per project if not configured
pub const DEFAULT_MAX_RUNS_PER_PROJECT: u64 = 10;

/// Log bytes stored per run if not configured (10 MiB)
pub const DEFAULT_MAX_LOG_BYTES_PER_RUN: u64 = 10 * 1024 * 1024;

/// Seconds a remote worker may stay silent before its run is requeued, if not configured
pub const DEFAULT_WORKER_LEASE_SECONDS: u32 = 60;

/// Times a run is claimed before a lost worker fails it instead of requeueing, if not configured
pub const DEFAULT_WORKER_MAX_ATTEMPTS: u32 = 3;

/// Seconds a run may wait in the queue before it fails, if not configured (24 hours)
pub const DEFAULT_QUEUED_TIMEOUT_SECONDS: u64 = 24 * 60 * 60;

/// Minimum length of the JWT secret in bytes
const MIN_SECRET_LENGTH: usize = 32;

/// Generates a random alphanumeric secret for signing JWTs
pub fn generate_secret() -> String {
    rand::rng()
        .sample_iter(Alphanumeric)
        .take(64)
        .map(char::from)
        .collect()
}

/// Who may create local workflows (a workflow with a local source: uploaded files on a server,
/// a folder on the desktop). Existing ones keep working whatever is set.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LocalWorkflowPolicy {
    /// Nobody: workflows must use a remote, nf-core or image source.
    No,
    /// Only administrators.
    AdminOnly,
    /// Every user who can create workflows.
    #[default]
    All,
}

impl LocalWorkflowPolicy {
    /// Whether a user may create local workflows.
    ///
    /// # Arguments
    /// * `is_admin` - Whether the user is an administrator
    pub fn permits(self, is_admin: bool) -> bool {
        match self {
            Self::No => false,
            Self::AdminOnly => is_admin,
            Self::All => true,
        }
    }
}

/// Folder below the storage root holding all workflows: `local/<id>` (uploaded) and
/// `remote_cache/{mirrors,checkouts}` (repositories).
pub const WORKFLOWS_DIRECTORY_NAME: &str = "workflows";

/// Name of the repository cache folder below [`WORKFLOWS_DIRECTORY_NAME`].
pub const REMOTE_CACHE_DIRECTORY_NAME: &str = "remote_cache";

/// Default of `branch_cache_seconds`: one hour.
fn default_branch_cache_seconds() -> u64 {
    3600
}

/// Folder below the storage root holding the project directories.
pub const PROJECTS_DIRECTORY_NAME: &str = "projects";

/// Folder below [`WORKFLOWS_DIRECTORY_NAME`] holding uploaded local workflows.
pub const LOCAL_WORKFLOWS_DIRECTORY_NAME: &str = "local";

/// Default of `max_workflow_upload_bytes`: 256 MiB.
fn default_max_workflow_upload_bytes() -> u64 {
    256 * 1024 * 1024
}

/// Default of `max_workflow_extracted_bytes`: 1 GiB.
fn default_max_workflow_extracted_bytes() -> u64 {
    1024 * 1024 * 1024
}

/// Default of `max_workflow_files`.
fn default_max_workflow_files() -> usize {
    20_000
}

/// Default of `checkout_max_age_days`.
fn default_checkout_max_age_days() -> u64 {
    30
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Configuration {
    interface: String,
    port: u16,
    database_url: String,
    secret: String,
    /// Duration after the login expires in seconds, defaults to `DEFAULT_LOGIN_VALIDITY`
    login_validity: Option<u32>,
    /// Storage root on standalone, shared (e.g. over NFS) between the backend and the workers.
    /// Everything the backend keeps on disk lives below it: `projects/<id>` (project data),
    /// `workflows/remote_cache` (repository cache), `global_reference_data` and
    /// `workflows/local/<id>` (uploaded local workflows). Workers mount it wherever they like (`--storage-path`) and resolve the
    /// paths they are given relative to it.
    storage_path: String,
    /// Directory of the global repository cache (mirrors and checkouts of remote workflows).
    /// Defaults to `<storage_path>/workflows/remote_cache`, see [`Self::cache_path`]. Outside the
    /// storage root workers must have it at the same absolute path.
    #[serde(default)]
    cache_path: Option<String>,
    /// Seconds a fetched branch of a cached repository is trusted before it is fetched again.
    #[serde(default = "default_branch_cache_seconds")]
    branch_cache_seconds: u64,
    /// Days after which an unused checkout of the repository cache is deleted, `0` keeps all.
    #[serde(default = "default_checkout_max_age_days")]
    checkout_max_age_days: u64,
    /// Who may create local workflows: `no`, `admin-only` or `all` (default).
    #[serde(default)]
    permit_local_workflows: LocalWorkflowPolicy,
    /// Whether a local workflow may point at any directory of the machine. Off for servers:
    /// local workflows are then uploaded as zip archives and live below
    /// `<storage_path>/workflows/local/<workflow id>`. The desktop app turns it on (single local user
    /// picking a folder).
    #[serde(default)]
    allow_local_paths: bool,
    /// Largest accepted workflow archive upload in bytes.
    #[serde(default = "default_max_workflow_upload_bytes")]
    max_workflow_upload_bytes: u64,
    /// Most bytes a workflow archive may extract to.
    #[serde(default = "default_max_workflow_extracted_bytes")]
    max_workflow_extracted_bytes: u64,
    /// Most files a workflow archive may contain.
    #[serde(default = "default_max_workflow_files")]
    max_workflow_files: usize,
    /// Worker credentials
    worker: WorkerCredentials,
    /// Login providers
    // TODO: Store as hashset, needs custom module for serialization
    login_providers: Vec<LoginProvider>,
    /// Origins allowed to call the API cross-origin, e.g. a separately deployed web frontend.
    /// Empty (default) disables CORS, `"*"` allows any origin. Ignored by the desktop integration.
    #[serde(default)]
    cors_allowed_origins: Vec<String>,
    /// Runs kept per project, the oldest finished and unlocked run is replaced by a new one.
    /// `0` disables the limit.
    #[serde(default = "default_max_runs_per_project")]
    max_runs_per_project: u64,
    /// Log bytes stored per run, further output is dropped after one truncation marker.
    /// `0` disables the limit.
    #[serde(default = "default_max_log_bytes_per_run")]
    max_log_bytes_per_run: u64,
    /// Seconds a remote worker may stay silent before its run is requeued, workers heartbeat
    /// several times per lease.
    #[serde(default = "default_worker_lease_seconds")]
    worker_lease_seconds: u32,
    /// Times a run is claimed before losing its worker fails it instead of requeueing it
    #[serde(default = "default_worker_max_attempts")]
    worker_max_attempts: u32,
    /// Seconds a run may be running before it fails with `timed_out` and its engine is stopped.
    /// `0` disables the limit (default).
    #[serde(default)]
    run_timeout_seconds: u64,
    /// Seconds a run may wait in the queue (e.g. no worker offers its engine) before it fails
    /// with `queued_timeout`. `0` disables the limit.
    #[serde(default = "default_queued_timeout_seconds")]
    queued_timeout_seconds: u64,
    /// Engines the container runtime cannot run, runs of them are rejected. `Some` when the
    /// embedding application (desktop) detected them itself, never read from the config file.
    /// `None`: derived from the announcements of live workers, see `State::unavailable_engines`.
    #[serde(skip)]
    fixed_unavailable_engines: Option<Vec<Engine>>,
}

fn default_max_runs_per_project() -> u64 {
    DEFAULT_MAX_RUNS_PER_PROJECT
}

fn default_max_log_bytes_per_run() -> u64 {
    DEFAULT_MAX_LOG_BYTES_PER_RUN
}

fn default_worker_lease_seconds() -> u32 {
    DEFAULT_WORKER_LEASE_SECONDS
}

fn default_queued_timeout_seconds() -> u64 {
    DEFAULT_QUEUED_TIMEOUT_SECONDS
}

fn default_worker_max_attempts() -> u32 {
    DEFAULT_WORKER_MAX_ATTEMPTS
}

impl Configuration {
    /// Checks the configuration for values that must not be used, e.g. weak JWT secrets
    pub fn validate(&self) -> Result<(), crate::error::Error> {
        if self.secret == "development" || self.secret.len() < MIN_SECRET_LENGTH {
            return Err(crate::error::Error::Configuration(
                "secret must be at least 32 bytes long and not the development default, use the `config` command to generate one",
            ));
        }
        for origin in &self.cors_allowed_origins {
            if origin != "*" && origin.parse::<http::HeaderValue>().is_err() {
                return Err(crate::error::Error::Configuration(
                    "cors_allowed_origins must contain `*` or valid origins, e.g. https://example.org",
                ));
            }
        }
        if self.worker_lease_seconds < 5 {
            return Err(crate::error::Error::Configuration(
                "worker_lease_seconds must be at least 5",
            ));
        }
        if self.worker_max_attempts == 0 {
            return Err(crate::error::Error::Configuration(
                "worker_max_attempts must be at least 1",
            ));
        }
        Ok(())
    }

    pub fn interface(&self) -> &str {
        &self.interface
    }

    pub fn interface_mut(&mut self) -> &mut String {
        &mut self.interface
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn port_mut(&mut self) -> &mut u16 {
        &mut self.port
    }

    pub fn database_url(&self) -> &str {
        &self.database_url
    }

    pub fn database_url_mut(&mut self) -> &mut String {
        &mut self.database_url
    }

    pub fn secret(&self) -> &str {
        &self.secret
    }

    pub fn secret_mut(&mut self) -> &mut String {
        &mut self.secret
    }

    pub fn login_validity(&self) -> &Option<u32> {
        &self.login_validity
    }

    pub fn login_validity_mut(&mut self) -> &mut Option<u32> {
        &mut self.login_validity
    }

    /// The configured storage root as written in the configuration (may be relative).
    pub fn storage_path(&self) -> &str {
        &self.storage_path
    }

    /// Mutable access to [`Self::storage_path`].
    pub fn storage_path_mut(&mut self) -> &mut String {
        &mut self.storage_path
    }

    /// The storage root as an absolute path (against the working directory, symlinks are not
    /// resolved). Engines mount paths below it, which must be absolute.
    pub fn storage_directory(&self) -> std::path::PathBuf {
        let path = std::path::PathBuf::from(&self.storage_path);
        std::path::absolute(&path).unwrap_or(path)
    }

    /// Directory holding the project directories: `<storage>/projects`.
    pub fn projects_directory(&self) -> std::path::PathBuf {
        self.storage_directory().join(PROJECTS_DIRECTORY_NAME)
    }

    /// Directory of the global repository cache: `cache_path` if set, else
    /// `<storage>/workflows/remote_cache`.
    pub fn cache_path(&self) -> std::path::PathBuf {
        match &self.cache_path {
            Some(path) => std::path::PathBuf::from(path),
            None => self
                .storage_directory()
                .join(WORKFLOWS_DIRECTORY_NAME)
                .join(REMOTE_CACHE_DIRECTORY_NAME),
        }
    }

    /// Makes a path under the storage root relative to it (workers resolve it against their own
    /// mount, see [`macworp_core::paths::resolve_in_storage`]); other paths are unchanged.
    ///
    /// # Arguments
    /// * `path` - Absolute path on the backend
    pub fn relative_to_storage(&self, path: &std::path::Path) -> std::path::PathBuf {
        macworp_core::paths::relative_to_storage(&self.storage_directory(), path)
    }

    /// Resolves a path that may be relative to the storage root to an absolute path on the
    /// backend, see [`macworp_core::paths::resolve_in_storage`].
    ///
    /// # Arguments
    /// * `path` - Relative to the storage root, or absolute
    pub fn resolve_storage_path(&self, path: &std::path::Path) -> std::path::PathBuf {
        macworp_core::paths::resolve_in_storage(&self.storage_directory(), path)
    }

    /// Mutable access to [`Self::cache_path`], e.g. for the desktop app.
    pub fn cache_path_mut(&mut self) -> &mut Option<String> {
        &mut self.cache_path
    }

    /// Seconds a fetched branch of a cached repository is trusted.
    pub fn branch_cache_seconds(&self) -> u64 {
        self.branch_cache_seconds
    }

    /// Age after which unused cache checkouts are deleted, `None` when disabled.
    pub fn checkout_max_age(&self) -> Option<std::time::Duration> {
        (self.checkout_max_age_days > 0)
            .then(|| std::time::Duration::from_secs(self.checkout_max_age_days * 86_400))
    }

    /// Who may create local workflows.
    pub fn permit_local_workflows(&self) -> LocalWorkflowPolicy {
        self.permit_local_workflows
    }

    /// Mutable access to [`Self::permit_local_workflows`].
    pub fn permit_local_workflows_mut(&mut self) -> &mut LocalWorkflowPolicy {
        &mut self.permit_local_workflows
    }

    /// Whether local workflows may point at arbitrary directories.
    pub fn allow_local_paths(&self) -> bool {
        self.allow_local_paths
    }

    /// Mutable access to [`Self::allow_local_paths`], for the desktop app.
    pub fn allow_local_paths_mut(&mut self) -> &mut bool {
        &mut self.allow_local_paths
    }

    /// Largest accepted workflow archive upload in bytes.
    pub fn max_workflow_upload_bytes(&self) -> u64 {
        self.max_workflow_upload_bytes
    }

    /// Limits applied when extracting an uploaded workflow archive.
    pub fn workflow_upload_limits(&self) -> crate::workflow_upload::Limits {
        crate::workflow_upload::Limits {
            max_files: self.max_workflow_files,
            max_extracted_bytes: self.max_workflow_extracted_bytes,
        }
    }

    /// Directory of an uploaded local workflow: `<storage>/workflows/local/<id>`, absolute.
    ///
    /// # Arguments
    /// * `workflow_integration_id` - Id of the workflow
    pub fn local_workflow_directory(&self, workflow_integration_id: i64) -> std::path::PathBuf {
        self.storage_directory()
            .join(WORKFLOWS_DIRECTORY_NAME)
            .join(LOCAL_WORKFLOWS_DIRECTORY_NAME)
            .join(workflow_integration_id.to_string())
    }

    /// Where an uploaded local workflow is stored in a run snapshot or workflow configuration:
    /// relative to the storage root (`workflows/local/<id>`), so every worker finds it on its own mount.
    ///
    /// # Arguments
    /// * `workflow_integration_id` - Id of the workflow
    pub fn local_workflow_location(&self, workflow_integration_id: i64) -> std::path::PathBuf {
        std::path::PathBuf::from(WORKFLOWS_DIRECTORY_NAME)
            .join(LOCAL_WORKFLOWS_DIRECTORY_NAME)
            .join(workflow_integration_id.to_string())
    }

    pub fn worker(&self) -> &WorkerCredentials {
        &self.worker
    }

    pub fn worker_mut(&mut self) -> &mut WorkerCredentials {
        &mut self.worker
    }

    pub fn login_providers(&self) -> &Vec<LoginProvider> {
        &self.login_providers
    }

    pub fn login_providers_mut(&mut self) -> &mut Vec<LoginProvider> {
        &mut self.login_providers
    }

    pub fn cors_allowed_origins(&self) -> &Vec<String> {
        &self.cors_allowed_origins
    }

    pub fn cors_allowed_origins_mut(&mut self) -> &mut Vec<String> {
        &mut self.cors_allowed_origins
    }

    pub fn max_runs_per_project(&self) -> u64 {
        self.max_runs_per_project
    }

    pub fn max_runs_per_project_mut(&mut self) -> &mut u64 {
        &mut self.max_runs_per_project
    }

    pub fn max_log_bytes_per_run(&self) -> u64 {
        self.max_log_bytes_per_run
    }

    pub fn max_log_bytes_per_run_mut(&mut self) -> &mut u64 {
        &mut self.max_log_bytes_per_run
    }

    pub fn worker_lease_seconds(&self) -> u32 {
        self.worker_lease_seconds
    }

    pub fn worker_lease_seconds_mut(&mut self) -> &mut u32 {
        &mut self.worker_lease_seconds
    }

    pub fn worker_max_attempts(&self) -> u32 {
        self.worker_max_attempts
    }

    pub fn worker_max_attempts_mut(&mut self) -> &mut u32 {
        &mut self.worker_max_attempts
    }

    pub fn run_timeout_seconds(&self) -> u64 {
        self.run_timeout_seconds
    }

    pub fn run_timeout_seconds_mut(&mut self) -> &mut u64 {
        &mut self.run_timeout_seconds
    }

    pub fn queued_timeout_seconds(&self) -> u64 {
        self.queued_timeout_seconds
    }

    pub fn queued_timeout_seconds_mut(&mut self) -> &mut u64 {
        &mut self.queued_timeout_seconds
    }

    /// Running time limit, `None` when disabled (`run_timeout_seconds` is `0`)
    pub fn run_timeout(&self) -> Option<chrono::Duration> {
        Self::limit(self.run_timeout_seconds)
    }

    /// Queue time limit, `None` when disabled (`queued_timeout_seconds` is `0`)
    pub fn queued_timeout(&self) -> Option<chrono::Duration> {
        Self::limit(self.queued_timeout_seconds)
    }

    /// Converts a limit in seconds to a duration, `0` and values beyond `i64` mean no limit
    fn limit(seconds: u64) -> Option<chrono::Duration> {
        i64::try_from(seconds)
            .ok()
            .filter(|seconds| *seconds > 0)
            .and_then(chrono::Duration::try_seconds)
    }

    pub fn fixed_unavailable_engines(&self) -> Option<&[Engine]> {
        self.fixed_unavailable_engines.as_deref()
    }

    pub fn fixed_unavailable_engines_mut(&mut self) -> &mut Option<Vec<Engine>> {
        &mut self.fixed_unavailable_engines
    }
}

impl Default for Configuration {
    fn default() -> Self {
        Self {
            interface: "127.0.0.1".to_string(),
            port: 8080,
            database_url: "postgresql://postgres:postgres@127.0.0.1:5432/postgres".to_string(), // for the provided docker compose
            secret: generate_secret(),
            login_validity: None,
            storage_path: "dev_storage".to_string(),
            cache_path: None,
            branch_cache_seconds: default_branch_cache_seconds(),
            checkout_max_age_days: default_checkout_max_age_days(),
            permit_local_workflows: LocalWorkflowPolicy::default(),
            allow_local_paths: false,
            max_workflow_upload_bytes: default_max_workflow_upload_bytes(),
            max_workflow_extracted_bytes: default_max_workflow_extracted_bytes(),
            max_workflow_files: default_max_workflow_files(),
            worker: WorkerCredentials::default(),
            login_providers: vec![LoginProvider::Local(LocalLoginProvider::default())],
            cors_allowed_origins: Vec::new(),
            max_runs_per_project: DEFAULT_MAX_RUNS_PER_PROJECT,
            max_log_bytes_per_run: DEFAULT_MAX_LOG_BYTES_PER_RUN,
            worker_lease_seconds: DEFAULT_WORKER_LEASE_SECONDS,
            worker_max_attempts: DEFAULT_WORKER_MAX_ATTEMPTS,
            run_timeout_seconds: 0,
            queued_timeout_seconds: DEFAULT_QUEUED_TIMEOUT_SECONDS,
            fixed_unavailable_engines: None,
        }
    }
}

/// Credentials for the worker
///
#[derive(Debug, Serialize, Deserialize)]
pub struct WorkerCredentials {
    /// username of the worker
    pub username: String,
    /// Hashed password of the worker
    pub password: String,
}

impl WorkerCredentials {
    /// Creates new worker credentials
    ///
    /// # Arguments
    /// * `username` - The username of the worker
    /// * `password` - The hashed password of the worker
    ///
    pub fn new(username: String, password: String) -> Self {
        Self { username, password }
    }

    pub fn username(&self) -> &str {
        &self.username
    }

    /// Sets the username of the worker
    pub fn username_mut(&mut self) -> &String {
        &mut self.username
    }

    /// Returns the hashed password
    pub fn password(&self) -> &str {
        &self.password
    }

    /// Sets the hashed password of the worker
    pub fn password_mut(&mut self) -> &mut String {
        &mut self.password
    }
}

impl Default for WorkerCredentials {
    fn default() -> Self {
        Self {
            username: "worker".to_string(),
            password: password_auth::generate_hash(b"developer"),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub enum LoginProvider {
    Local(LocalLoginProvider),
}

/// Login provider using the internal database
///
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct LocalLoginProvider {
    pub registration: bool,
}

impl From<&LoginProvider> for LoginProviderResponse {
    fn from(value: &LoginProvider) -> Self {
        match value {
            LoginProvider::Local(provider) => LoginProviderResponse::Local(provider.registration),
        }
    }
}

impl Hash for LoginProvider {
    fn hash<H: Hasher>(&self, state: &mut H) {
        match self {
            LoginProvider::Local(_) => {
                "local".hash(state);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The storage layout is absolute even with the relative default, and paths below the
    /// storage root go over the wire relative.
    #[test]
    fn storage_layout() {
        let config = Configuration::default();
        for directory in [
            config.storage_directory(),
            config.projects_directory(),
            config.cache_path(),
            config.local_workflow_directory(3),
        ] {
            assert!(directory.is_absolute(), "{directory:?}");
        }
        assert!(
            config
                .projects_directory()
                .ends_with("dev_storage/projects")
        );
        assert!(
            config
                .cache_path()
                .ends_with("dev_storage/workflows/remote_cache")
        );
        assert!(
            config
                .local_workflow_directory(3)
                .ends_with("dev_storage/workflows/local/3")
        );
        assert_eq!(
            config.local_workflow_location(3),
            std::path::PathBuf::from("workflows/local/3")
        );
        let project = config.projects_directory().join("7");
        assert_eq!(
            config.relative_to_storage(&project),
            std::path::PathBuf::from("projects/7")
        );
        assert_eq!(
            config.resolve_storage_path(std::path::Path::new("projects/7")),
            project
        );
    }

    /// The policy names parse from the config file and decide as documented.
    #[test]
    fn local_workflow_policy() {
        for (name, policy) in [
            ("no", LocalWorkflowPolicy::No),
            ("admin-only", LocalWorkflowPolicy::AdminOnly),
            ("all", LocalWorkflowPolicy::All),
        ] {
            let parsed: LocalWorkflowPolicy = serde_json::from_str(&format!("\"{name}\"")).unwrap();
            assert_eq!(parsed, policy);
        }
        assert!(!LocalWorkflowPolicy::No.permits(true));
        assert!(!LocalWorkflowPolicy::AdminOnly.permits(false));
        assert!(LocalWorkflowPolicy::AdminOnly.permits(true));
        assert!(LocalWorkflowPolicy::All.permits(false));
        assert_eq!(
            Configuration::default().permit_local_workflows(),
            LocalWorkflowPolicy::All
        );
    }

    #[test]
    fn validate_rejects_invalid_cors_origin() {
        let mut config = Configuration::default();
        assert!(config.validate().is_ok());
        config
            .cors_allowed_origins_mut()
            .push("bad\norigin".to_string());
        assert!(config.validate().is_err());
    }

    #[test]
    fn validate_accepts_cors_origins() {
        let mut config = Configuration::default();
        config.cors_allowed_origins_mut().push("*".to_string());
        config
            .cors_allowed_origins_mut()
            .push("http://localhost:8080".to_string());
        assert!(config.validate().is_ok());
    }
}
