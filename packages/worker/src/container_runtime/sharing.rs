//! Which folders of the computer the container VM can see. On macOS podman machine and Docker
//! Desktop run containers in a VM that shares only some folders; a project in another folder
//! fails with a confusing mount error at the first run, so it is checked up front.

use std::path::{Path, PathBuf};

use serde_json::Value;

use super::{RuntimeKind, runtime::RuntimeInfo};

/// The folders the VM of a runtime shares with the computer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SharedFolders {
    /// Shared folders, a path below one of them is visible in the VM
    roots: Vec<PathBuf>,
    /// Who shares them, for messages ("podman machine podman-machine-default", "Docker Desktop")
    owner: String,
    /// What the user can do to share another folder, for messages
    fix: &'static str,
}

impl SharedFolders {
    /// The folders a podman machine mounts into its VM under the same path (the mounts of its
    /// configuration file, `<name>.json` in the machine's config directory).
    ///
    /// # Arguments
    /// * `name` - name of the machine
    /// * `configuration` - content of the machine's configuration file
    pub fn from_podman_machine(name: &str, configuration: &str) -> Option<Self> {
        let value: Value = serde_json::from_str(configuration).ok()?;
        let roots = value
            .get("Mounts")?
            .as_array()?
            .iter()
            .filter_map(|mount| {
                let source = mount.get("Source")?.as_str()?;
                // Engines are given host paths as they are, so only identical paths help
                (mount.get("Target")?.as_str()? == source).then(|| PathBuf::from(source))
            })
            .collect();
        Some(Self {
            roots,
            owner: format!("podman machine {name}"),
            fix: "a podman machine shares folders only when it is created: create a new machine that shares it (`podman machine init -v /folder:/folder`) or move the data into a shared folder",
        })
    }

    /// The folders Docker Desktop shares (`FilesharingDirectories` of its settings file).
    ///
    /// # Arguments
    /// * `settings` - content of Docker Desktop's `settings-store.json`
    pub fn from_docker_desktop(settings: &str) -> Option<Self> {
        let value: Value = serde_json::from_str(settings).ok()?;
        let roots = value
            .get("FilesharingDirectories")?
            .as_array()?
            .iter()
            .filter_map(|directory| directory.as_str().map(PathBuf::from))
            .collect();
        Some(Self {
            roots,
            owner: "Docker Desktop".to_string(),
            fix: "add the folder under Settings > Resources > File sharing in Docker Desktop, or move the data into a shared folder",
        })
    }

    /// The folders the runtime of this computer shares. Only macOS needs it: on Linux the
    /// containers see the host directly and on Windows the VM sees every drive.
    ///
    /// # Arguments
    /// * `info` - what the runtime reported about itself
    /// * `home` - the user's home directory
    pub async fn detect(info: &RuntimeInfo, home: &Path) -> Option<Self> {
        if !cfg!(target_os = "macos") {
            return None;
        }
        match info.kind {
            RuntimeKind::Docker => {
                let file =
                    home.join("Library/Group Containers/group.com.docker/settings-store.json");
                Self::from_docker_desktop(&std::fs::read_to_string(file).ok()?)
            }
            RuntimeKind::Podman => {
                let machine = super::adjustment::Machine::list()
                    .await
                    .into_iter()
                    .next()?;
                let file = home
                    .join(".config/containers/podman/machine")
                    .join(&machine.provider)
                    .join(format!("{}.json", machine.name));
                Self::from_podman_machine(&machine.name, &std::fs::read_to_string(file).ok()?)
            }
            RuntimeKind::Wslc => None,
        }
    }

    /// Whether the VM sees `path`. Symlinks are resolved first (`/tmp` is `/private/tmp`, `/var`
    /// is `/private/var`); a path that does not exist yet counts as its nearest existing parent.
    ///
    /// # Arguments
    /// * `path` - folder on the computer
    pub fn covers(&self, path: &Path) -> bool {
        let resolved = Self::resolve(path);
        self.roots.iter().any(|root| {
            let root = Self::resolve(root);
            resolved.starts_with(&root)
        })
    }

    /// The paths among `paths` the VM does not see.
    ///
    /// # Arguments
    /// * `paths` - folders on the computer
    pub fn missing<'a>(&self, paths: impl IntoIterator<Item = &'a Path>) -> Vec<PathBuf> {
        paths
            .into_iter()
            .filter(|path| !self.covers(path))
            .map(Path::to_path_buf)
            .collect()
    }

    /// Who shares the folders, for messages.
    pub fn owner(&self) -> &str {
        &self.owner
    }

    /// What the user can do about a folder that is not shared, for messages.
    pub fn fix(&self) -> &'static str {
        self.fix
    }

    /// `path` with symlinks resolved as far as it exists.
    ///
    /// # Arguments
    /// * `path` - any path
    fn resolve(path: &Path) -> PathBuf {
        let mut existing = path;
        let mut rest = Vec::new();
        loop {
            if let Ok(canonical) = existing.canonicalize() {
                return rest
                    .iter()
                    .rev()
                    .fold(canonical, |joined, part| joined.join(part));
            }
            match (existing.parent(), existing.file_name()) {
                (Some(parent), Some(name)) => {
                    rest.push(name.to_os_string());
                    existing = parent;
                }
                _ => return path.to_path_buf(),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The mounts of a default macOS podman machine.
    const PODMAN: &str = r#"{"Mounts":[
        {"Source":"/Users","Target":"/Users","Type":"virtiofs"},
        {"Source":"/private","Target":"/private","Type":"virtiofs"},
        {"Source":"/var/folders","Target":"/var/folders","Type":"virtiofs"},
        {"Source":"/Users/me/.config/containers","Target":"/etc/containers","Type":"virtiofs"}]}"#;

    /// The default folders of Docker Desktop on macOS.
    const DOCKER: &str =
        r#"{"FilesharingDirectories":["/Users","/Volumes","/private","/tmp","/var/folders"]}"#;

    #[test]
    fn podman_machine_shares_its_identical_mounts_only() {
        let shared = SharedFolders::from_podman_machine("m", PODMAN).unwrap();
        assert!(shared.covers(Path::new("/Users/me/data")));
        assert!(shared.covers(Path::new("/var/folders/ab/cd")));
        assert!(!shared.covers(Path::new("/opt/data")));
        // The configuration mount has another target: it does not make `/etc/containers` the
        // host's `/Users/me/.config/containers` for engines, but the folder is below /Users anyway
        assert!(!shared.covers(Path::new("/Volumes/disk/data")));
        assert_eq!(shared.owner(), "podman machine m");
    }

    #[test]
    fn docker_desktop_shares_its_list() {
        let shared = SharedFolders::from_docker_desktop(DOCKER).unwrap();
        assert!(shared.covers(Path::new("/Volumes/disk/data")));
        assert!(shared.covers(Path::new("/Users/me/project")));
        assert!(!shared.covers(Path::new("/opt/data")));
        assert!(SharedFolders::from_docker_desktop("{}").is_none());
        assert!(SharedFolders::from_docker_desktop("nope").is_none());
    }

    /// A path counts as its nearest existing parent, and a shared folder is not left by `..`.
    #[test]
    fn missing_paths_and_symlinks_are_resolved() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().canonicalize().unwrap();
        let shared = SharedFolders {
            roots: vec![root.clone()],
            owner: String::new(),
            fix: "",
        };
        assert!(shared.covers(&root.join("not/yet/there")));
        assert!(!shared.covers(&root.join("../elsewhere")));
        #[cfg(unix)]
        {
            let outside = tempfile::tempdir().unwrap();
            std::os::unix::fs::symlink(outside.path(), root.join("link")).unwrap();
            assert!(!shared.covers(&root.join("link/data")));
        }
        assert_eq!(
            shared.missing([root.as_path(), Path::new("/definitely/not/shared")]),
            vec![PathBuf::from("/definitely/not/shared")]
        );
    }
}
