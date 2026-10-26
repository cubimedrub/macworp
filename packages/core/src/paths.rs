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
    fn confinement() {
        assert!(is_confined(Path::new("a/b")));
        assert!(is_confined(Path::new("/a/b")));
        assert!(is_confined(Path::new("./a")));
        assert!(!is_confined(Path::new("../a")));
        assert!(!is_confined(Path::new("a/../../b")));
    }
}
