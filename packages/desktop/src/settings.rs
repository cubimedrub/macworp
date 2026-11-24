use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use macworp_backend::configuration::{REMOTE_CACHE_DIRECTORY_NAME, WORKFLOWS_DIRECTORY_NAME};
use macworp_backend::reference_directory::{ReferenceDirectory, DEFAULT_DIRECTORY_NAME};
use macworp_backend::workflow_repository_cache::WorkflowRepositoryCache;
use macworp_core::paths::{APPTAINER_CACHE_DIRECTORY, CWL_CACHE_DIRECTORY};
use macworp_worker::apptainer_cache_store::{ApptainerCacheStore, DESKTOP_MAX_BYTES};
use macworp_worker::image_store::{ImagePolicy, ImageStore};
use macworp_worker::selinux::DirectoryLabel;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Name of the settings file in the app data directory.
const FILE_NAME: &str = "settings.json";

/// Why settings could not be read, written or applied.
#[derive(Debug, Error)]
pub enum Error {
    /// Reading or writing the settings file or the chosen folder failed
    #[error("{0}")]
    Io(#[from] std::io::Error),
    /// The settings file is not valid JSON
    #[error("Settings file is damaged: {0}")]
    Format(#[from] serde_json::Error),
    /// The chosen path exists but is not a directory
    #[error("`{0}` is not a folder")]
    NotADirectory(PathBuf),
    /// The chosen folder is a symbolic link, which the file browser would not follow
    #[error("`{0}` is a symbolic link, choose the real folder")]
    Symlink(PathBuf),
    /// A storage limit is zero or so large that it cannot be a size
    #[error("{0} must be a whole number of at least 1 and at most {MAX_LIMIT}")]
    InvalidLimit(&'static str),
}

/// Largest accepted value of a limit (images or GiB), far above any disk and small enough that
/// converting GiB to bytes cannot overflow.
const MAX_LIMIT: u32 = 1_000_000;

/// Bytes in one GiB.
const GIB: u64 = 1024 * 1024 * 1024;

/// How much disk space downloaded container images may take; `None` = no limit. Saved in the
/// settings file, applied to the running worker at once.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct StorageLimits {
    /// Most container images kept that runs used
    pub max_images: Option<u32>,
    /// Most GiB those images may take together
    pub max_image_gib: Option<u32>,
    /// Most GiB the Snakemake image cache may take
    pub max_cache_gib: Option<u32>,
}

impl Default for StorageLimits {
    /// The limits of a fresh installation, the same as the worker's desktop defaults.
    fn default() -> Self {
        let policy = ImagePolicy::desktop_default();
        Self {
            max_images: policy.max_images.map(|count| count as u32),
            max_image_gib: policy.max_bytes.map(|bytes| (bytes / GIB) as u32),
            max_cache_gib: Some((DESKTOP_MAX_BYTES / GIB) as u32),
        }
    }
}

impl StorageLimits {
    /// Checks that every limit that is set is between 1 and [`MAX_LIMIT`].
    pub fn validate(&self) -> Result<(), Error> {
        for (value, name) in [
            (self.max_images, "The number of images"),
            (self.max_image_gib, "The image size limit"),
            (self.max_cache_gib, "The image cache size limit"),
        ] {
            if value.is_some_and(|value| value == 0 || value > MAX_LIMIT) {
                return Err(Error::InvalidLimit(name));
            }
        }
        Ok(())
    }

    /// The worker policy for these limits.
    ///
    /// # Arguments
    /// * `current` - the policy in force; its pinned images are kept
    fn image_policy(&self, current: ImagePolicy) -> ImagePolicy {
        ImagePolicy {
            max_images: self.max_images.map(|count| count as usize),
            max_bytes: self.max_image_gib.map(|gib| u64::from(gib) * GIB),
            pinned: current.pinned,
        }
    }

    /// The byte limit of the image cache.
    fn cache_bytes(&self) -> Option<u64> {
        self.max_cache_gib.map(|gib| u64::from(gib) * GIB)
    }
}

/// What the limited stores take now.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct StorageUsage {
    /// Container images that runs used and the worker tracks
    pub images: usize,
    /// Their size in bytes
    pub image_bytes: u64,
    /// Bytes in the Snakemake image cache
    pub cache_bytes: u64,
}

/// What a manual clean-up removed.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct CleanedUp {
    /// Container images removed
    pub images: usize,
    /// Image cache files removed
    pub cache_files: usize,
}

/// The worker's stores the limits are applied to, known once the worker started.
#[derive(Debug, Default)]
struct Stores {
    /// One per container runtime
    images: Vec<Arc<ImageStore>>,
    /// The Snakemake image cache store
    cache: Option<Arc<ApptainerCacheStore>>,
}

/// What the user chose in the settings page, persisted as JSON in the app data directory. Holds no
/// secrets.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
struct Stored {
    /// Folder with the global reference data, `None` = the default below the app data directory
    #[serde(default)]
    global_reference_path: Option<PathBuf>,
    /// Folder of the workflow repository cache, `None` = the default below the app data directory
    #[serde(default)]
    workflow_cache_path: Option<PathBuf>,
    /// Folder of the Apptainer image cache of Snakemake, `None` = the default below the app data
    /// directory
    #[serde(default)]
    apptainer_cache_path: Option<PathBuf>,
    /// Storage limits, `None` = the defaults
    #[serde(default)]
    storage_limits: Option<StorageLimits>,
}

/// The folders the user can move in the settings page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Folder {
    /// Global reference data shared by all projects
    Reference,
    /// Cache of fetched remote workflow repositories
    WorkflowCache,
    /// Container images Snakemake pulls through Apptainer, shared by all projects
    ApptainerCache,
}

/// The desktop settings: where the global reference data and the workflow cache live. Cloning
/// shares the same backend handles, so a change applies to the running backend and worker
/// without restart (runs already started keep their mount).
#[derive(Debug, Clone)]
pub struct Settings {
    /// Directory holding the settings file; the default folders are below it
    directory: PathBuf,
    /// Handle shared with the backend and the embedded worker
    reference_directory: ReferenceDirectory,
    /// Handle shared with the embedded worker
    apptainer_cache: ReferenceDirectory,
    /// Cache shared with the backend
    workflow_repository_cache: Arc<WorkflowRepositoryCache>,
    /// The explicit choices as saved, so changing one folder keeps the other
    stored: Arc<Mutex<Stored>>,
    /// The worker's stores, filled by [`Settings::attach_stores`]
    stores: Arc<Mutex<Stores>>,
}

impl Settings {
    /// Loads the settings and creates both folders.
    ///
    /// # Arguments
    /// * `directory` - The app data directory
    /// * `branch_cache_seconds` - Branch trust time of the repository cache
    pub fn load(directory: &Path, branch_cache_seconds: u64) -> Result<Self, Error> {
        let stored = Self::read(directory)?;
        let settings = Self {
            directory: directory.to_path_buf(),
            reference_directory: ReferenceDirectory::new(
                stored
                    .global_reference_path
                    .clone()
                    .unwrap_or_else(|| Self::default_path(directory, Folder::Reference)),
            ),
            apptainer_cache: ReferenceDirectory::new(
                stored
                    .apptainer_cache_path
                    .clone()
                    .unwrap_or_else(|| Self::default_path(directory, Folder::ApptainerCache)),
            ),
            workflow_repository_cache: Arc::new(WorkflowRepositoryCache::new(
                stored
                    .workflow_cache_path
                    .clone()
                    .unwrap_or_else(|| Self::default_path(directory, Folder::WorkflowCache)),
                Duration::from_secs(branch_cache_seconds),
            )),
            stored: Arc::new(Mutex::new(stored)),
            stores: Arc::default(),
        };
        std::fs::create_dir_all(settings.path(Folder::Reference))?;
        std::fs::create_dir_all(settings.path(Folder::WorkflowCache))?;
        std::fs::create_dir_all(settings.path(Folder::ApptainerCache))?;
        std::fs::create_dir_all(settings.cwl_cache())?;
        settings.label_default_folders();
        Ok(settings)
    }

    /// On an enforcing SELinux host, labels the folders at their default place (created and owned
    /// by the app) so containers the engine starts itself can use them. Folders the user chose
    /// are never touched; a failure is only logged.
    fn label_default_folders(&self) {
        if !macworp_worker::executors::selinux_enforcing() {
            return;
        }
        for folder in [
            Folder::Reference,
            Folder::WorkflowCache,
            Folder::ApptainerCache,
        ] {
            if !self.is_default(folder) {
                continue;
            }
            let path = self.path(folder);
            if let Err(error) = DirectoryLabel::of(&path).apply() {
                tracing::warn!("Cannot label {} for containers: {error}", path.display());
            }
        }
        let cwl_cache = self.cwl_cache();
        if let Err(error) = DirectoryLabel::of(&cwl_cache).apply() {
            tracing::warn!(
                "Cannot label {} for containers: {error}",
                cwl_cache.display()
            );
        }
    }

    /// Whether containers the engine starts itself can use `folder`: `Some(true)` when it carries
    /// the container label, `None` where SELinux does not enforce (nothing to toggle).
    ///
    /// # Arguments
    /// * `folder` - Which folder
    pub fn container_access(&self, folder: Folder) -> Option<bool> {
        if !macworp_worker::executors::selinux_enforcing() {
            return None;
        }
        let path = self.path(folder);
        Some(
            DirectoryLabel::of(&path)
                .current()
                .is_some_and(|label| DirectoryLabel::allows_containers(&label)),
        )
    }

    /// Labels `folder` and everything in it for containers, or gives it the label of its parent
    /// folder again. The only way to label a folder the user picked, always on an explicit request.
    ///
    /// # Arguments
    /// * `folder` - Which folder
    /// * `allowed` - `true` labels it `container_file_t`, `false` restores the label of the parent folder
    pub fn set_container_access(&self, folder: Folder, allowed: bool) -> Result<(), Error> {
        let path = self.path(folder);
        let label = DirectoryLabel::of(&path);
        if allowed {
            label.apply()
        } else {
            label.restore()
        }?;
        Ok(())
    }

    /// The reference data handle for the backend and worker.
    pub fn reference_directory(&self) -> ReferenceDirectory {
        self.reference_directory.clone()
    }

    /// The image cache handle for the embedded worker.
    pub fn apptainer_cache(&self) -> ReferenceDirectory {
        self.apptainer_cache.clone()
    }

    /// The folder of the CWL step cache, always below the app data directory (not movable in the
    /// settings page). Created by [`Settings::load`].
    pub fn cwl_cache(&self) -> PathBuf {
        self.directory.join(CWL_CACHE_DIRECTORY)
    }

    /// The repository cache for the backend.
    pub fn workflow_repository_cache(&self) -> Arc<WorkflowRepositoryCache> {
        self.workflow_repository_cache.clone()
    }

    /// Folder used while the user chose none.
    ///
    /// # Arguments
    /// * `directory` - The app data directory
    /// * `folder` - Which folder
    pub fn default_path(directory: &Path, folder: Folder) -> PathBuf {
        match folder {
            Folder::Reference => directory.join(DEFAULT_DIRECTORY_NAME),
            Folder::WorkflowCache => directory
                .join(WORKFLOWS_DIRECTORY_NAME)
                .join(REMOTE_CACHE_DIRECTORY_NAME),
            Folder::ApptainerCache => directory.join(APPTAINER_CACHE_DIRECTORY),
        }
    }

    /// The folder currently in use.
    ///
    /// # Arguments
    /// * `folder` - Which folder
    pub fn path(&self, folder: Folder) -> PathBuf {
        match folder {
            Folder::Reference => self.reference_directory.get(),
            Folder::WorkflowCache => self.workflow_repository_cache.root(),
            Folder::ApptainerCache => self.apptainer_cache.get(),
        }
    }

    /// Whether the folder in use is the default one.
    ///
    /// # Arguments
    /// * `folder` - Which folder
    pub fn is_default(&self, folder: Folder) -> bool {
        self.path(folder) == Self::default_path(&self.directory, folder)
    }

    /// Switches `folder` to `path` (`None` = default): validates it, creates it if missing,
    /// saves the choice and applies it to the running backend. Nothing is moved from the old
    /// folder.
    ///
    /// # Arguments
    /// * `folder` - Which folder
    /// * `path` - The new folder or `None` for the default
    pub fn set_path(&self, folder: Folder, path: Option<PathBuf>) -> Result<(), Error> {
        let target = path
            .clone()
            .unwrap_or_else(|| Self::default_path(&self.directory, folder));
        match std::fs::symlink_metadata(&target) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(Error::Symlink(target));
            }
            Ok(metadata) if !metadata.is_dir() => return Err(Error::NotADirectory(target)),
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                std::fs::create_dir_all(&target)?;
            }
            Err(error) => return Err(error.into()),
        }
        let mut stored = self.stored.lock().unwrap_or_else(|e| e.into_inner());
        let mut updated = stored.clone();
        match folder {
            Folder::Reference => updated.global_reference_path = path,
            Folder::WorkflowCache => updated.workflow_cache_path = path,
            Folder::ApptainerCache => updated.apptainer_cache_path = path,
        }
        std::fs::write(
            self.directory.join(FILE_NAME),
            serde_json::to_vec_pretty(&updated)?,
        )?;
        *stored = updated;
        match folder {
            Folder::Reference => self.reference_directory.set(target),
            Folder::WorkflowCache => self.workflow_repository_cache.set_root(target),
            Folder::ApptainerCache => self.apptainer_cache.set(target),
        }
        Ok(())
    }

    /// The app data directory the settings live in; the worker keeps its usage state there too.
    pub fn directory(&self) -> &Path {
        &self.directory
    }

    /// The storage limits in force.
    pub fn storage_limits(&self) -> StorageLimits {
        self.stored
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .storage_limits
            .unwrap_or_default()
    }

    /// Registers the worker's stores and applies the saved limits to them. Called once when the
    /// worker starts; without a container runtime there is nothing to attach.
    ///
    /// # Arguments
    /// * `images` - the image store of every container runtime
    /// * `cache` - the Snakemake image cache store
    pub fn attach_stores(&self, images: Vec<Arc<ImageStore>>, cache: Arc<ApptainerCacheStore>) {
        let limits = self.storage_limits();
        Self::apply(&limits, &images, Some(&cache));
        *self.stores.lock().unwrap_or_else(|e| e.into_inner()) = Stores {
            images,
            cache: Some(cache),
        };
    }

    /// Pushes `limits` into the stores.
    ///
    /// # Arguments
    /// * `limits` - what to apply
    /// * `images` - image stores
    /// * `cache` - image cache store
    fn apply(
        limits: &StorageLimits,
        images: &[Arc<ImageStore>],
        cache: Option<&Arc<ApptainerCacheStore>>,
    ) {
        for store in images {
            store.set_policy(limits.image_policy(store.policy()));
        }
        if let Some(cache) = cache {
            cache.set_max_bytes(limits.cache_bytes());
        }
    }

    /// Validates, saves and applies new storage limits. The next collection after a run uses
    /// them; "Clean up now" applies them at once.
    ///
    /// # Arguments
    /// * `limits` - the new limits; a missing value means no limit
    pub fn set_storage_limits(&self, limits: StorageLimits) -> Result<(), Error> {
        limits.validate()?;
        let mut stored = self.stored.lock().unwrap_or_else(|e| e.into_inner());
        let mut updated = stored.clone();
        updated.storage_limits = Some(limits);
        std::fs::write(
            self.directory.join(FILE_NAME),
            serde_json::to_vec_pretty(&updated)?,
        )?;
        *stored = updated;
        let stores = self.stores.lock().unwrap_or_else(|e| e.into_inner());
        Self::apply(&limits, &stores.images, stores.cache.as_ref());
        Ok(())
    }

    /// The stores and cache folder, copied so no lock is held while waiting.
    fn attached(
        &self,
    ) -> (
        Vec<Arc<ImageStore>>,
        Option<Arc<ApptainerCacheStore>>,
        PathBuf,
    ) {
        let stores = self.stores.lock().unwrap_or_else(|e| e.into_inner());
        (
            stores.images.clone(),
            stores.cache.clone(),
            self.apptainer_cache.get(),
        )
    }

    /// What the tracked images and the image cache take now. Parts that cannot be read count
    /// as zero.
    pub async fn storage_usage(&self) -> StorageUsage {
        let (images, cache, directory) = self.attached();
        let mut usage = StorageUsage::default();
        for store in images {
            match store.usage().await {
                Ok(used) => {
                    usage.images += used.images;
                    usage.image_bytes += used.bytes;
                }
                Err(error) => tracing::warn!("Cannot read the image usage: {error}"),
            }
        }
        if cache.is_some() {
            usage.cache_bytes = ApptainerCacheStore::usage(&directory).unwrap_or(0);
        }
        usage
    }

    /// Removes what exceeds the limits now instead of after the next run. Stores with a run in
    /// progress are skipped, since an engine may be about to start one of their images.
    pub async fn clean_up_now(&self) -> CleanedUp {
        let (images, cache, directory) = self.attached();
        let mut cleaned = CleanedUp::default();
        for store in images.iter().filter(|store| store.is_idle()) {
            match store.collect().await {
                Ok(collected) => cleaned.images += collected.removed.len(),
                Err(error) => tracing::warn!("Image clean-up failed: {error}"),
            }
        }
        if let Some(cache) = cache {
            match cache.trim_now(&directory) {
                Ok(removed) => cleaned.cache_files += removed.len(),
                Err(error) => tracing::warn!("Image cache clean-up failed: {error}"),
            }
        }
        cleaned
    }

    /// Reads the settings file, defaults when it does not exist.
    ///
    /// # Arguments
    /// * `directory` - The app data directory
    fn read(directory: &Path) -> Result<Stored, Error> {
        match std::fs::read(directory.join(FILE_NAME)) {
            Ok(bytes) => Ok(serde_json::from_slice(&bytes)?),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Stored::default()),
            Err(error) => Err(error.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Default folders are created, a choice is persisted and applied live, reset returns to the
    /// default, invalid targets are refused and leave the setting alone, and the two folders
    /// are saved independently.
    #[test]
    fn choice_is_persisted_and_applied() {
        let directory = tempfile::tempdir().unwrap();
        let settings = Settings::load(directory.path(), 3600).unwrap();
        for folder in [
            Folder::Reference,
            Folder::WorkflowCache,
            Folder::ApptainerCache,
        ] {
            assert!(settings.is_default(folder));
            assert!(settings.path(folder).is_dir());
        }

        let chosen = directory.path().join("elsewhere");
        let handle = settings.reference_directory();
        settings
            .set_path(Folder::Reference, Some(chosen.clone()))
            .unwrap();
        assert_eq!(handle.get(), chosen);
        assert!(chosen.is_dir());
        assert!(settings.is_default(Folder::WorkflowCache));

        let cache = directory.path().join("cache");
        let cache_handle = settings.workflow_repository_cache();
        settings
            .set_path(Folder::WorkflowCache, Some(cache.clone()))
            .unwrap();
        assert_eq!(cache_handle.root(), cache);

        let images = directory.path().join("images");
        settings
            .set_path(Folder::ApptainerCache, Some(images.clone()))
            .unwrap();
        assert_eq!(settings.apptainer_cache().get(), images);

        // Both choices survive a restart, changing one did not drop the other.
        let reloaded = Settings::load(directory.path(), 3600).unwrap();
        assert_eq!(reloaded.path(Folder::Reference), chosen);
        assert_eq!(reloaded.path(Folder::WorkflowCache), cache);
        assert_eq!(reloaded.path(Folder::ApptainerCache), images);

        let file = directory.path().join("file");
        std::fs::write(&file, "x").unwrap();
        assert!(matches!(
            settings.set_path(Folder::WorkflowCache, Some(file)),
            Err(Error::NotADirectory(_))
        ));
        assert_eq!(cache_handle.root(), cache);

        settings.set_path(Folder::Reference, None).unwrap();
        settings.set_path(Folder::WorkflowCache, None).unwrap();
        settings.set_path(Folder::ApptainerCache, None).unwrap();
        let reloaded = Settings::load(directory.path(), 3600).unwrap();
        assert!(reloaded.is_default(Folder::Reference));
        assert!(reloaded.is_default(Folder::WorkflowCache));
        assert!(reloaded.is_default(Folder::ApptainerCache));
    }

    /// Limits are validated before anything is saved, saved across restarts, and reach stores
    /// attached before and after the change without a restart.
    #[test]
    fn storage_limits_are_validated_saved_and_applied() {
        use macworp_worker::container_runtime::RuntimeKind;
        let directory = tempfile::tempdir().unwrap();
        let settings = Settings::load(directory.path(), 3600).unwrap();
        assert_eq!(settings.storage_limits(), StorageLimits::default());

        let image_store = Arc::new(ImageStore::new(
            // Never started: only the policy is exercised
            Arc::new(macworp_worker::container_runtime::CliRuntime::new(
                RuntimeKind::Podman,
            )),
            ImagePolicy::desktop_default(),
            None,
        ));
        let cache_store = Arc::new(ApptainerCacheStore::new(Some(DESKTOP_MAX_BYTES), None));
        settings.attach_stores(vec![image_store.clone()], cache_store.clone());

        let limits = StorageLimits {
            max_images: Some(5),
            max_image_gib: None,
            max_cache_gib: Some(2),
        };
        settings.set_storage_limits(limits).unwrap();
        assert_eq!(image_store.policy().max_images, Some(5));
        assert_eq!(image_store.policy().max_bytes, None);
        assert_eq!(cache_store.max_bytes(), Some(2 * GIB));
        assert_eq!(
            Settings::load(directory.path(), 3600)
                .unwrap()
                .storage_limits(),
            limits
        );

        for invalid in [
            StorageLimits {
                max_images: Some(0),
                ..limits
            },
            StorageLimits {
                max_image_gib: Some(MAX_LIMIT + 1),
                ..limits
            },
            StorageLimits {
                max_cache_gib: Some(0),
                ..limits
            },
        ] {
            assert!(matches!(
                settings.set_storage_limits(invalid),
                Err(Error::InvalidLimit(_))
            ));
        }
        assert_eq!(settings.storage_limits(), limits);
        assert_eq!(image_store.policy().max_images, Some(5));
    }

    /// On an enforcing SELinux host the container label of a chosen folder can be switched on
    /// and off; elsewhere there is nothing to toggle.
    #[test]
    fn container_label_can_be_toggled() {
        let directory = tempfile::tempdir().unwrap();
        let settings = Settings::load(directory.path(), 3600).unwrap();
        let chosen = directory.path().join("picked");
        settings
            .set_path(Folder::Reference, Some(chosen.clone()))
            .unwrap();
        if !macworp_worker::executors::selinux_enforcing() {
            assert_eq!(settings.container_access(Folder::Reference), None);
            return;
        }
        std::fs::create_dir(chosen.join("inner")).unwrap();
        assert_eq!(settings.container_access(Folder::Reference), Some(false));
        settings
            .set_container_access(Folder::Reference, true)
            .unwrap();
        assert_eq!(settings.container_access(Folder::Reference), Some(true));
        let inner = DirectoryLabel::of(&chosen.join("inner")).current().unwrap();
        assert!(DirectoryLabel::allows_containers(&inner));
        settings
            .set_container_access(Folder::Reference, false)
            .unwrap();
        assert_eq!(settings.container_access(Folder::Reference), Some(false));
    }
}
