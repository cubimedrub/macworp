use clap::Args;

use crate::{
    apptainer_cache_store::ApptainerCacheStore,
    container_runtime::{ContainerRuntime, RuntimePreference},
    image_store::{ImagePolicy, ImageStore},
};

/// Worker configuration, usable as clap arguments (worker binary) or built directly (embedded worker)
#[derive(Debug, Clone, Default, Args)]
pub struct WorkerConfiguration {
    /// Container runtime to use for running workflow engines
    #[arg(
        long,
        env = "MACWORP_CONTAINER_RUNTIME",
        value_enum,
        default_value_t = RuntimePreference::Auto
    )]
    pub container_runtime: RuntimePreference,
    /// Most images the worker keeps that runs used; the least recently used ones beyond it are
    /// removed after a run (0 = keep all). Images no run used are never touched.
    #[arg(long, env = "MACWORP_MAX_IMAGES", default_value_t = 0)]
    pub max_images: usize,
    /// Most bytes those images may take together, as the runtime reports sizes (0 = no limit)
    #[arg(long, env = "MACWORP_MAX_IMAGE_BYTES", default_value_t = 0)]
    pub max_image_bytes: u64,
    /// Most bytes the Apptainer image cache of Snakemake may take; the least recently used
    /// image files beyond it are removed after a run (0 = keep all)
    #[arg(long, env = "MACWORP_MAX_APPTAINER_CACHE_BYTES", default_value_t = 0)]
    pub max_apptainer_cache_bytes: u64,
    /// Image names never removed, in addition to the engine images (`macworp-*`): an exact
    /// name, a repository for all its tags, or a prefix ending in `*`. Comma separated or
    /// repeated.
    #[arg(long = "pin-image", env = "MACWORP_PIN_IMAGES", value_delimiter = ',')]
    pub pinned_images: Vec<String>,
    /// Directory for the worker's own state (which images runs used), default
    /// `$XDG_STATE_HOME/macworp` or `~/.local/state/macworp`
    #[arg(long, env = "MACWORP_STATE_DIRECTORY")]
    pub state_directory: Option<std::path::PathBuf>,
}

impl WorkerConfiguration {
    /// The image removal rules these settings describe; 0 turns a limit off.
    pub fn image_policy(&self) -> ImagePolicy {
        ImagePolicy {
            max_images: (self.max_images > 0).then_some(self.max_images),
            max_bytes: (self.max_image_bytes > 0).then_some(self.max_image_bytes),
            pinned: self.pinned_images.clone(),
        }
    }

    /// The tracking and trimming of the Apptainer cache these settings describe.
    pub fn apptainer_cache_store(&self) -> std::sync::Arc<ApptainerCacheStore> {
        std::sync::Arc::new(ApptainerCacheStore::new(
            (self.max_apptainer_cache_bytes > 0).then_some(self.max_apptainer_cache_bytes),
            ApptainerCacheStore::state_file_in(self.state_directory.as_deref()),
        ))
    }

    /// One image store per runtime, so usage and removal follow each runtime's own image store.
    ///
    /// # Arguments
    /// * `runtimes` - usable container runtimes of the worker
    pub fn image_stores(
        &self,
        runtimes: &[std::sync::Arc<dyn ContainerRuntime>],
    ) -> Vec<std::sync::Arc<ImageStore>> {
        runtimes
            .iter()
            .map(|runtime| {
                let state_file = ImageStore::state_file_in(
                    self.state_directory.as_deref(),
                    &runtime.kind().to_string(),
                );
                std::sync::Arc::new(ImageStore::new(
                    runtime.clone(),
                    self.image_policy(),
                    state_file,
                ))
            })
            .collect()
    }
}

/// How a remote worker reaches and identifies itself to the backend, usable as clap arguments.
/// Not needed by the embedded desktop worker, which reads the database directly.
#[derive(Debug, Clone, Default, Args)]
pub struct RemoteWorkerConfiguration {
    /// Backend base URL, e.g. https://macworp.example.org
    #[arg(long, env = "MACWORP_BACKEND_URL")]
    pub backend_url: Option<String>,
    /// Username of the worker credential configured in the backend
    #[arg(long, env = "MACWORP_WORKER_USERNAME", default_value = "worker")]
    pub username: String,
    /// Password of the worker credential, prefer `--password-file` (arguments are visible to
    /// other local users)
    #[arg(long, env = "MACWORP_WORKER_PASSWORD", hide_env_values = true)]
    pub password: Option<String>,
    /// File holding the password, one trailing line break is ignored
    #[arg(
        long,
        env = "MACWORP_WORKER_PASSWORD_FILE",
        conflicts_with = "password"
    )]
    pub password_file: Option<std::path::PathBuf>,
    /// Name of this worker process in the backend, defaults to `$HOSTNAME` or `worker`
    #[arg(long, env = "MACWORP_WORKER_NAME")]
    pub name: Option<String>,
    /// Where this machine has the backend's storage mounted (e.g. an NFS share). Paths the
    /// backend sends are relative to the storage root and resolved against this directory, so
    /// it may differ from the mount point on the backend and on other workers.
    #[arg(long, env = "MACWORP_STORAGE_PATH")]
    pub storage_path: Option<std::path::PathBuf>,
    /// Runs this worker executes at the same time
    #[arg(long, env = "MACWORP_WORKER_CAPACITY", default_value_t = 1, value_parser = clap::value_parser!(u16).range(1..))]
    pub capacity: u16,
}

impl RemoteWorkerConfiguration {
    /// The backend URL, required to run a remote worker
    pub fn backend_url(&self) -> Result<&str, String> {
        self.backend_url
            .as_deref()
            .ok_or_else(|| "--backend-url (MACWORP_BACKEND_URL) is required".to_string())
    }

    /// The storage mount as an absolute path, required to run a remote worker (engines mount
    /// paths below it, which must be absolute).
    pub fn storage_path(&self) -> Result<std::path::PathBuf, String> {
        let path = self.storage_path.clone().ok_or_else(|| {
            "--storage-path (MACWORP_STORAGE_PATH) is required: where the backend's storage is mounted on this machine".to_string()
        })?;
        std::path::absolute(&path)
            .map_err(|e| format!("Invalid storage path {}: {e}", path.display()))
    }

    /// The password from `--password` or the password file; the file's trailing line break is
    /// dropped. Empty passwords are rejected.
    pub fn resolve_password(&self) -> Result<String, String> {
        let password = match (&self.password, &self.password_file) {
            (Some(password), _) => password.clone(),
            (None, Some(path)) => {
                let content = std::fs::read_to_string(path)
                    .map_err(|e| format!("Cannot read password file {}: {e}", path.display()))?;
                content
                    .strip_suffix("\r\n")
                    .or_else(|| content.strip_suffix('\n'))
                    .unwrap_or(&content)
                    .to_string()
            }
            (None, None) => {
                return Err(
                    "--password-file (MACWORP_WORKER_PASSWORD_FILE) or --password is required"
                        .to_string(),
                );
            }
        };
        if password.is_empty() {
            return Err("The worker password is empty".to_string());
        }
        Ok(password)
    }

    /// Name reported to the backend: `--name`, else `$HOSTNAME`, else `worker`
    pub fn worker_name(&self) -> String {
        self.name
            .clone()
            .or_else(|| {
                std::env::var("HOSTNAME")
                    .ok()
                    .filter(|name| !name.is_empty())
            })
            .unwrap_or_else(|| "worker".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Password file: one trailing line break is stripped, an empty file is rejected
    #[test]
    fn password_file_is_trimmed_and_must_not_be_empty() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("password");
        std::fs::write(&path, "secret\n").unwrap();
        let configuration = RemoteWorkerConfiguration {
            password_file: Some(path.clone()),
            ..Default::default()
        };
        assert_eq!(configuration.resolve_password().unwrap(), "secret");
        std::fs::write(&path, "\n").unwrap();
        assert!(configuration.resolve_password().is_err());
        assert!(
            RemoteWorkerConfiguration::default()
                .resolve_password()
                .is_err()
        );
    }
}
