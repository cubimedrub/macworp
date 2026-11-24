//! Single definition of what an in-project relative path is, shared by backend and clients so
//! both agree on it. Purely lexical: nothing here touches the filesystem, so symlinks still have
//! to be checked where the path is resolved (see the backend `IsFilesystem::join_paths`).

use std::path::{Component, Path, PathBuf};

/// Rebuilds a relative `path` from its components joined with `/`, whatever the platform.
///
/// Relative paths cross machines (backend to worker, API to UI) as JSON strings, and a
/// `PathBuf` serialises with the separator of the platform it was built on. Normalising to `/`
/// keeps the wire form identical on Windows and Unix; Windows accepts `/` when the path is
/// used. Only use it on relative paths: a root or drive prefix is dropped.
///
/// # Arguments
/// * `path` - Relative path in any separator style the platform understands
pub fn to_portable(path: &Path) -> PathBuf {
    let parts: Vec<_> = path
        .components()
        .filter_map(|component| match component {
            Component::Normal(c) => Some(c.to_string_lossy()),
            _ => None,
        })
        .collect();
    PathBuf::from(parts.join("/"))
}

/// Keeps only the normal components of `path`: root, prefix (drive), `.` and `..` are dropped,
/// so joining the result onto a base directory cannot leave it. The result uses `/` as
/// separator on every platform (see [`to_portable`]).
pub fn sanitize_relative(path: &Path) -> PathBuf {
    to_portable(path)
}

/// Where the Apptainer image cache of Snakemake lives below the storage root, shared by all
/// projects and by every worker mounting the same storage.
pub const APPTAINER_CACHE_DIRECTORY: &str = "cache/apptainer";

/// Where the step cache of the CWL engine (Toil `--cachedir`) lives below the storage root,
/// shared by all projects and by every worker mounting the same storage. A run that starts
/// again after a failure or with corrected values reuses the outputs of the steps whose inputs
/// did not change.
pub const CWL_CACHE_DIRECTORY: &str = "cache/cwl";

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
        .map(to_portable)
        .unwrap_or_else(|_| path.to_path_buf())
}

/// How a path of the machine running the worker (the *host path*) looks inside the engine
/// container and in the sibling containers the engine starts, which the runtime's daemon
/// resolves. Mounts use the host path as source and this as the target, and every path handed
/// to the engine is the target form, so the container-side path is identical wherever it is
/// seen (the rule of ADR 0002/0004).
///
/// Only Windows hosts differ: the runtime runs in a Linux VM that sees the Windows drives
/// below a mount point, and the mount point depends on the runtime (checked 2026-11-24 with
/// the engine socket pattern: podman machine needs `/mnt/c/...`, Docker Desktop `/c/...`,
/// the other form does not resolve for the sibling container).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum PathMapping {
    /// The host path is valid inside the container as it is (Linux, macOS)
    #[default]
    Native,
    /// `C:\Users\x` becomes `/mnt/c/Users/x` (podman machine, WSL)
    WslMount,
    /// `C:\Users\x` becomes `/c/Users/x` (Docker Desktop)
    DriveRoot,
}

impl PathMapping {
    /// The container-side form of `host`, always with `/` as separator for the Windows
    /// mappings. A drive path (`C:\a`, `C:/a`) gets the drive prefix of the mapping, a path
    /// without a drive (relative, or already POSIX) only has its separators normalised, and a
    /// UNC path (`\\server\share`) cannot be mounted by any of the runtimes and comes out as
    /// `//server/share/...`, which then fails when the run starts. [`Self::Native`] returns the
    /// path as it is.
    ///
    /// # Arguments
    /// * `host` - A path on the machine running the worker
    pub fn container(self, host: &Path) -> String {
        let text = host.to_string_lossy();
        match self {
            Self::Native => text.into_owned(),
            Self::WslMount => Self::map_windows(&text, "/mnt"),
            Self::DriveRoot => Self::map_windows(&text, ""),
        }
    }

    /// The part of [`Self::container`] that does not depend on the platform: maps a Windows path
    /// given as text. `root` is what goes before the lower-case drive letter.
    ///
    /// # Arguments
    /// * `host` - Path text, `\` or `/` separated
    /// * `root` - `/mnt` or empty
    fn map_windows(host: &str, root: &str) -> String {
        let normalised = host.replace('\\', "/");
        // `std::fs::canonicalize` on Windows returns `\\?\C:\...` (verbatim form)
        let normalised = normalised
            .strip_prefix("//?/")
            .map_or(normalised.as_str(), |rest| rest)
            .to_string();
        let mut chars = normalised.chars();
        match (chars.next(), chars.next()) {
            (Some(drive), Some(':')) if drive.is_ascii_alphabetic() => {
                let rest = &normalised[2..];
                let rest = if rest.is_empty() || rest.starts_with('/') {
                    rest.to_string()
                } else {
                    // `C:file` is relative to the drive's current folder, which has no meaning here
                    format!("/{rest}")
                };
                format!("{root}/{}{rest}", drive.to_ascii_lowercase())
            }
            _ => normalised,
        }
    }
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
    // Native mapping with POSIX host paths: a Windows host uses a mapped style (see the `PathMapping` tests).
    #[cfg(unix)]
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

    /// Drive paths map per runtime, with either separator and any drive letter case.
    #[test]
    fn windows_drive_paths_map_to_the_runtime_view() {
        let host = Path::new(r"C:\Users\Admin\hp");
        assert_eq!(
            PathMapping::WslMount.container(host),
            "/mnt/c/Users/Admin/hp"
        );
        assert_eq!(PathMapping::DriveRoot.container(host), "/c/Users/Admin/hp");
        let slashes = Path::new("D:/data/p 1");
        assert_eq!(PathMapping::WslMount.container(slashes), "/mnt/d/data/p 1");
        assert_eq!(PathMapping::DriveRoot.container(slashes), "/d/data/p 1");
        assert_eq!(PathMapping::DriveRoot.container(Path::new("C:\\")), "/c/");
        assert_eq!(PathMapping::DriveRoot.container(Path::new("C:")), "/c");
        // The verbatim form `canonicalize` returns on Windows
        assert_eq!(
            PathMapping::WslMount.container(Path::new(r"\\?\C:\Users\a")),
            "/mnt/c/Users/a"
        );
    }

    /// Without a drive only the separators change, `Native` changes nothing.
    #[test]
    fn paths_without_a_drive_are_only_normalised() {
        assert_eq!(
            PathMapping::WslMount.container(Path::new("/data/ref")),
            "/data/ref"
        );
        assert_eq!(
            PathMapping::DriveRoot.container(Path::new(r"reads\a.fq")),
            "reads/a.fq"
        );
        assert_eq!(
            PathMapping::DriveRoot.container(Path::new(r"\\server\share\x")),
            "//server/share/x"
        );
        assert_eq!(
            PathMapping::Native.container(Path::new("/data/p1")),
            "/data/p1"
        );
    }
}
