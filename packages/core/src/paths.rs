//! Single definition of what an in-project relative path is, shared by backend and clients so
//! both agree on it. Purely lexical: nothing here touches the filesystem, so symlinks still have
//! to be checked where the path is resolved (see the backend `IsFilesystem::join_paths`).

use std::path::{Component, Path, PathBuf};

/// Keeps only the normal components of `path`: root, prefix (drive), `.` and `..` are dropped,
/// so joining the result onto a base directory cannot leave it.
pub fn sanitize_relative(path: &Path) -> PathBuf {
    path.components()
        .filter_map(|component| match component {
            Component::Normal(c) => Some(c),
            _ => None,
        })
        .collect()
}

/// Name of the virtual top-level folder under which the global reference data appears in the
/// file browser of every project. A `Path` parameter value starting with it is rewritten to the
/// absolute reference directory when the run is rendered. Chosen to be unlikely to clash with a
/// folder of a project, which cannot use the name.
pub const REFERENCE_PREFIX: &str = "macworp_global_reference_data";

/// The part of `path` below [`REFERENCE_PREFIX`] (empty for the folder itself), `None` if the
/// path does not point into the reference data.
///
/// # Arguments
/// * `path` - A path relative to the project, as stored in a `Path` parameter
pub fn strip_reference_prefix(path: &Path) -> Option<PathBuf> {
    let normal = sanitize_relative(path);
    normal
        .strip_prefix(REFERENCE_PREFIX)
        .ok()
        .map(Path::to_path_buf)
}

/// Resolves a path stored or sent relative to the storage root against the machine's own mount
/// of that storage. Absolute paths are returned as they are (they live outside the storage and
/// must be valid on the machine). The relative part is sanitised, so it cannot leave `storage`.
///
/// # Arguments
/// * `storage` - Where this machine has the shared storage mounted
/// * `path` - Path relative to the storage root, or absolute
pub fn resolve_in_storage(storage: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        storage.join(sanitize_relative(path))
    }
}

/// The inverse of [`resolve_in_storage`]: `path` relative to `storage` if it lies below it,
/// otherwise unchanged. Used before a path leaves the backend, so machines with the storage
/// mounted elsewhere can resolve it.
///
/// # Arguments
/// * `storage` - The backend's absolute storage root
/// * `path` - Absolute path, possibly below the storage root
pub fn relative_to_storage(storage: &Path, path: &Path) -> PathBuf {
    path.strip_prefix(storage)
        .map(Path::to_path_buf)
        .unwrap_or_else(|_| path.to_path_buf())
}

/// Whether `path` is free of components that could point out of the project: `..` and
/// drive prefixes. A leading `/` is tolerated, [`sanitize_relative`] strips it.
pub fn is_confined(path: &Path) -> bool {
    !path
        .components()
        .any(|c| matches!(c, Component::ParentDir | Component::Prefix(_)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_drops_everything_but_normal_components() {
        assert_eq!(
            sanitize_relative(Path::new("/../a/./../b")),
            PathBuf::from("a/b")
        );
        assert_eq!(sanitize_relative(Path::new("")), PathBuf::new());
        assert_eq!(
            sanitize_relative(Path::new("x/y.txt")),
            PathBuf::from("x/y.txt")
        );
    }

    #[test]
    fn reference_prefix_is_split_off() {
        assert_eq!(
            strip_reference_prefix(Path::new("macworp_global_reference_data/hg38/a.fa")),
            Some(PathBuf::from("hg38/a.fa"))
        );
        assert_eq!(
            strip_reference_prefix(Path::new("/macworp_global_reference_data")),
            Some(PathBuf::new())
        );
        assert_eq!(strip_reference_prefix(Path::new("data/a.fa")), None);
        // Only the first component counts, and the name must match whole
        assert_eq!(
            strip_reference_prefix(Path::new("x/macworp_global_reference_data")),
            None
        );
        assert_eq!(
            strip_reference_prefix(Path::new("macworp_global_reference_data2/a")),
            None
        );
    }

    #[test]
    fn confinement() {
        assert!(is_confined(Path::new("a/b")));
        assert!(is_confined(Path::new("/a/b")));
        assert!(is_confined(Path::new("./a")));
        assert!(!is_confined(Path::new("../a")));
        assert!(!is_confined(Path::new("a/../../b")));
    }

    /// Relative paths are resolved below the storage (and cannot leave it), absolute ones are
    /// kept; `relative_to_storage` strips the root and leaves outside paths alone.
    #[test]
    fn storage_paths_round_trip() {
        let storage = Path::new("/mnt/storage");
        assert_eq!(
            resolve_in_storage(storage, Path::new("projects/3")),
            PathBuf::from("/mnt/storage/projects/3")
        );
        assert_eq!(
            resolve_in_storage(storage, Path::new("../etc/passwd")),
            PathBuf::from("/mnt/storage/etc/passwd")
        );
        assert_eq!(
            resolve_in_storage(storage, Path::new("/data/elsewhere")),
            PathBuf::from("/data/elsewhere")
        );
        let backend = Path::new("/srv/macworp");
        assert_eq!(
            relative_to_storage(backend, Path::new("/srv/macworp/local/3")),
            PathBuf::from("local/3")
        );
        assert_eq!(
            relative_to_storage(backend, Path::new("/data/elsewhere")),
            PathBuf::from("/data/elsewhere")
        );
    }
}
