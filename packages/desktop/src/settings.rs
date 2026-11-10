use std::path::{Path, PathBuf};

use macworp_backend::reference_directory::{ReferenceDirectory, DEFAULT_DIRECTORY_NAME};
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
}

/// The desktop settings: where the global reference data lives. Cloning shares the same backend
/// handle, so a change applies to the running backend and worker without restart (runs already
/// started keep their mount).
#[derive(Debug, Clone)]
pub struct Settings {
    /// Directory holding the settings file; the default reference folder is below it
    directory: PathBuf,
    /// Handle shared with the backend and the embedded worker
    reference_directory: ReferenceDirectory,
}

impl Settings {
    /// Loads the settings and creates the reference folder.
    ///
    /// # Arguments
    /// * `directory` - The app data directory
    pub fn load(directory: &Path) -> Result<Self, Error> {
        let stored = Self::read(directory)?;
        let settings = Self {
            directory: directory.to_path_buf(),
            reference_directory: ReferenceDirectory::new(
                stored
                    .global_reference_path
                    .unwrap_or_else(|| Self::default_reference_path(directory)),
            ),
        };
        std::fs::create_dir_all(settings.reference_directory.get())?;
        Ok(settings)
    }

    /// The handle for the backend and worker.
    pub fn reference_directory(&self) -> ReferenceDirectory {
        self.reference_directory.clone()
    }

    /// Folder used while the user chose none.
    ///
    /// # Arguments
    /// * `directory` - The app data directory
    pub fn default_reference_path(directory: &Path) -> PathBuf {
        directory.join(DEFAULT_DIRECTORY_NAME)
    }

    /// Whether the folder in use is the default one.
    pub fn is_default(&self) -> bool {
        self.reference_directory.get() == Self::default_reference_path(&self.directory)
    }

    /// Switches to `path` (`None` = default): validates it, creates it if missing, saves the
    /// choice and applies it to the running backend. Nothing is moved from the old folder.
    ///
    /// # Arguments
    /// * `path` - The new folder or `None` for the default
    pub fn set_reference_path(&self, path: Option<PathBuf>) -> Result<(), Error> {
        let target = path
            .clone()
            .unwrap_or_else(|| Self::default_reference_path(&self.directory));
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
        let stored = Stored {
            global_reference_path: path,
        };
        std::fs::write(
            self.directory.join(FILE_NAME),
            serde_json::to_vec_pretty(&stored)?,
        )?;
        self.reference_directory.set(target);
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

    /// Default folder is created, a choice is persisted and applied live, reset returns to the
    /// default, invalid targets are refused and leave the setting alone.
    #[test]
    fn choice_is_persisted_and_applied() {
        let directory = tempfile::tempdir().unwrap();
        let settings = Settings::load(directory.path()).unwrap();
        assert!(settings.is_default());
        assert!(settings.reference_directory().get().is_dir());

        let chosen = directory.path().join("elsewhere");
        let handle = settings.reference_directory();
        settings.set_reference_path(Some(chosen.clone())).unwrap();
        assert_eq!(handle.get(), chosen);
        assert!(chosen.is_dir());
        assert_eq!(
            Settings::load(directory.path())
                .unwrap()
                .reference_directory()
                .get(),
            chosen
        );

        let file = directory.path().join("file");
        std::fs::write(&file, "x").unwrap();
        assert!(matches!(
            settings.set_reference_path(Some(file)),
            Err(Error::NotADirectory(_))
        ));
        assert_eq!(handle.get(), chosen);

        settings.set_reference_path(None).unwrap();
        assert!(settings.is_default());
        assert!(Settings::load(directory.path()).unwrap().is_default());
    }
}
