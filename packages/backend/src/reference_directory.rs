use std::{
    path::{Path, PathBuf},
    sync::{Arc, RwLock},
};

/// Name of the folder holding the global reference data below the storage root.
pub const DEFAULT_DIRECTORY_NAME: &str = "global_reference_data";

/// Shared, changeable location of the global reference data on disk.
///
/// Cloning yields a handle to the same location, so the embedding application (desktop settings
/// page) can change it while the backend keeps serving: new requests and runs see the new path,
/// runs already started keep the mount they got.
#[derive(Clone, Debug)]
pub struct ReferenceDirectory {
    /// Current location, replaced as a whole
    path: Arc<RwLock<PathBuf>>,
}

impl ReferenceDirectory {
    /// Handle starting at `path`.
    ///
    /// # Arguments
    /// * `path` - Initial directory, not required to exist yet
    pub fn new(path: PathBuf) -> Self {
        Self {
            path: Arc::new(RwLock::new(path)),
        }
    }

    /// Default location below the storage root: `<storage>/global_reference_data`.
    ///
    /// # Arguments
    /// * `storage` - The storage root
    pub fn below(storage: &Path) -> Self {
        Self::new(storage.join(DEFAULT_DIRECTORY_NAME))
    }

    /// The current location.
    pub fn get(&self) -> PathBuf {
        self.path
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    /// Moves the handle (and every clone) to `path`. Nothing on disk is moved or checked.
    ///
    /// # Arguments
    /// * `path` - The new directory
    pub fn set(&self, path: PathBuf) {
        *self
            .path
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = path;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A clone sees changes made through the original handle.
    #[test]
    fn clones_share_the_location() {
        let original = ReferenceDirectory::below(Path::new("projects"));
        let clone = original.clone();
        assert_eq!(clone.get(), Path::new("projects/global_reference_data"));
        original.set(PathBuf::from("/data/reference"));
        assert_eq!(clone.get(), Path::new("/data/reference"));
    }
}
