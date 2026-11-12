use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use macworp_backend::configuration::{REMOTE_CACHE_DIRECTORY_NAME, WORKFLOWS_DIRECTORY_NAME};
use macworp_backend::reference_directory::{ReferenceDirectory, DEFAULT_DIRECTORY_NAME};
use macworp_backend::workflow_repository_cache::WorkflowRepositoryCache;
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
}

/// The folders the user can move in the settings page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Folder {
    /// Global reference data shared by all projects
    Reference,
    /// Cache of fetched remote workflow repositories
    WorkflowCache,
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
    /// Cache shared with the backend
    workflow_repository_cache: Arc<WorkflowRepositoryCache>,
    /// The explicit choices as saved, so changing one folder keeps the other
    stored: Arc<Mutex<Stored>>,
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
            workflow_repository_cache: Arc::new(WorkflowRepositoryCache::new(
                stored
                    .workflow_cache_path
                    .clone()
                    .unwrap_or_else(|| Self::default_path(directory, Folder::WorkflowCache)),
                Duration::from_secs(branch_cache_seconds),
            )),
            stored: Arc::new(Mutex::new(stored)),
        };
        std::fs::create_dir_all(settings.path(Folder::Reference))?;
        std::fs::create_dir_all(settings.path(Folder::WorkflowCache))?;
        Ok(settings)
    }

    /// The reference data handle for the backend and worker.
    pub fn reference_directory(&self) -> ReferenceDirectory {
        self.reference_directory.clone()
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
        }
        std::fs::write(
            self.directory.join(FILE_NAME),
            serde_json::to_vec_pretty(&updated)?,
        )?;
        *stored = updated;
        match folder {
            Folder::Reference => self.reference_directory.set(target),
            Folder::WorkflowCache => self.workflow_repository_cache.set_root(target),
        }
        Ok(())
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
        for folder in [Folder::Reference, Folder::WorkflowCache] {
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

        // Both choices survive a restart, changing one did not drop the other.
        let reloaded = Settings::load(directory.path(), 3600).unwrap();
        assert_eq!(reloaded.path(Folder::Reference), chosen);
        assert_eq!(reloaded.path(Folder::WorkflowCache), cache);

        let file = directory.path().join("file");
        std::fs::write(&file, "x").unwrap();
        assert!(matches!(
            settings.set_path(Folder::WorkflowCache, Some(file)),
            Err(Error::NotADirectory(_))
        ));
        assert_eq!(cache_handle.root(), cache);

        settings.set_path(Folder::Reference, None).unwrap();
        settings.set_path(Folder::WorkflowCache, None).unwrap();
        let reloaded = Settings::load(directory.path(), 3600).unwrap();
        assert!(reloaded.is_default(Folder::Reference));
        assert!(reloaded.is_default(Folder::WorkflowCache));
    }
}
