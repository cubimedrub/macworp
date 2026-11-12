//! Installing an uploaded workflow archive.
//!
//! Users upload local workflows as a zip file. The archive is untrusted: entries are extracted
//! into a staging directory with path, symlink, file count and size checks, and the result
//! becomes a new immutable version of the workflow:
//! `<storage_path>/workflows/local/<workflow_integration_id>/<version>/`, where the version is a
//! time-ordered UUID. A run is pinned to the newest version at its start, so later uploads never
//! change files a queued or running run uses; a failed upload leaves the previous version
//! untouched. Versions that are neither the newest nor used by an active run are deleted.

use std::fs;
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};

/// Why an archive was rejected or could not be installed.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Not a readable zip archive.
    #[error("not a valid zip archive: {0}")]
    Zip(#[from] zip::result::ZipError),
    /// An entry would be written outside the workflow directory (absolute path, `..`).
    #[error("unsafe path in archive: {0}")]
    UnsafePath(String),
    /// Symbolic links are not accepted, they could point out of the workflow directory.
    #[error("symbolic link in archive: {0}")]
    Symlink(String),
    /// The same path occurs twice.
    #[error("duplicate path in archive: {0}")]
    Duplicate(String),
    /// More files than allowed.
    #[error("archive has more than {0} files")]
    TooManyFiles(usize),
    /// More extracted data than allowed (counted while writing, not from the headers).
    #[error("archive extracts to more than {0} bytes")]
    TooLarge(u64),
    /// The main script is not in the archive.
    #[error("main script `{0}` not found in the archive")]
    MissingScript(String),
    /// No file in the archive.
    #[error("archive contains no files")]
    Empty,
    /// Filesystem failure.
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// Limits against oversized or bomb-like archives.
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    /// Most files accepted.
    pub max_files: usize,
    /// Most extracted bytes accepted over all files.
    pub max_extracted_bytes: u64,
}

/// What was installed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Installed {
    /// Files written.
    pub files: usize,
    /// Bytes written.
    pub bytes: u64,
}

/// Installs zip archives as workflow directories.
pub struct WorkflowUpload;

impl WorkflowUpload {
    /// Extracts `archive` into the new version directory `target`. A single top-level folder that
    /// holds everything (as made by "compress folder") is stripped; `__MACOSX` metadata is
    /// skipped. Blocking, call from `spawn_blocking`.
    ///
    /// # Arguments
    /// * `archive` - The uploaded zip file
    /// * `target` - Version directory to create, from [`Self::new_version_directory`]
    /// * `script` - Main script that must exist in the result (relative to the workflow)
    /// * `limits` - File count and size limits
    pub fn install(
        archive: &Path,
        target: &Path,
        script: &Path,
        limits: &Limits,
    ) -> Result<Installed, Error> {
        let parent = target.parent().expect("version directory has a parent");
        fs::create_dir_all(parent)?;
        let unique = uuid::Uuid::now_v7();
        let staging = parent.join(format!(".new-{unique}"));

        let extracted = Self::extract(archive, &staging, script, limits).and_then(|installed| {
            if staging.join(script).is_file() {
                Ok(installed)
            } else {
                Err(Error::MissingScript(script.display().to_string()))
            }
        });
        let installed = match extracted {
            Ok(installed) => installed,
            Err(error) => {
                let _ = fs::remove_dir_all(&staging);
                return Err(error);
            }
        };
        if let Err(error) = fs::rename(&staging, target) {
            let _ = fs::remove_dir_all(&staging);
            return Err(error.into());
        }
        Ok(installed)
    }

    /// Path of a new version below `workflow_directory`; the name is a time-ordered UUID, so the
    /// newest version sorts last.
    ///
    /// # Arguments
    /// * `workflow_directory` - Directory of the workflow, `<storage>/workflows/local/<id>`
    pub fn new_version_directory(workflow_directory: &Path) -> PathBuf {
        workflow_directory.join(uuid::Uuid::now_v7().to_string())
    }

    /// Whether `name` is the name of a version directory (a UUID).
    ///
    /// # Arguments
    /// * `name` - File name inside a workflow directory
    fn is_version_name(name: &str) -> bool {
        name.len() == 36 && uuid::Uuid::parse_str(name).is_ok()
    }

    /// Version directories of a workflow, oldest first. Empty if the directory does not exist.
    ///
    /// # Arguments
    /// * `workflow_directory` - Directory of the workflow
    pub fn versions(workflow_directory: &Path) -> Vec<PathBuf> {
        let Ok(entries) = fs::read_dir(workflow_directory) else {
            return Vec::new();
        };
        let mut versions: Vec<PathBuf> = entries
            .flatten()
            .filter(|entry| {
                entry.path().is_dir() && Self::is_version_name(&entry.file_name().to_string_lossy())
            })
            .map(|entry| entry.path())
            .collect();
        versions.sort();
        versions
    }

    /// The newest version of a workflow, `None` if nothing was uploaded (in the older layout
    /// without versions the files lie directly in the workflow directory).
    ///
    /// # Arguments
    /// * `workflow_directory` - Directory of the workflow
    pub fn current_version(workflow_directory: &Path) -> Option<PathBuf> {
        Self::versions(workflow_directory).pop()
    }

    /// Deletes the versions of a workflow that are not the newest and not in `in_use`. Returns
    /// how many were removed.
    ///
    /// # Arguments
    /// * `workflow_directory` - Directory of the workflow
    /// * `in_use` - Version directories of queued or running runs, never removed
    pub fn prune(workflow_directory: &Path, in_use: &std::collections::HashSet<PathBuf>) -> usize {
        let mut versions = Self::versions(workflow_directory);
        versions.pop();
        Self::remove_all(&versions, in_use)
    }

    /// Deletes the files of a workflow that no longer exists, except versions in `in_use`; the
    /// directory goes away with its last version. Returns how many versions were removed.
    ///
    /// # Arguments
    /// * `workflow_directory` - Directory of the workflow
    /// * `in_use` - Directories of queued or running runs, never removed. A run of the older
    ///   layout points at `workflow_directory` itself, which keeps all of it.
    pub fn remove(workflow_directory: &Path, in_use: &std::collections::HashSet<PathBuf>) -> usize {
        if in_use.iter().any(|used| used == workflow_directory) {
            return 0;
        }
        let removed = Self::remove_all(&Self::versions(workflow_directory), in_use);
        if !in_use
            .iter()
            .any(|used| used.starts_with(workflow_directory))
        {
            let _ = fs::remove_dir_all(workflow_directory);
        }
        removed
    }

    /// Deletes `versions` unless in use; failures are logged and skipped.
    ///
    /// # Arguments
    /// * `versions` - Version directories to delete
    /// * `in_use` - Directories that are kept
    fn remove_all(versions: &[PathBuf], in_use: &std::collections::HashSet<PathBuf>) -> usize {
        let mut removed = 0;
        for version in versions.iter().filter(|version| !in_use.contains(*version)) {
            match fs::remove_dir_all(version) {
                Ok(()) => removed += 1,
                Err(error) => {
                    tracing::warn!(%error, directory = %version.display(), "could not remove workflow version")
                }
            }
        }
        removed
    }

    /// Extracts into the new directory `staging`.
    ///
    /// # Arguments
    /// * `archive` - The uploaded zip file
    /// * `staging` - Directory to create
    /// * `script` - Main script; a wrapper folder is not stripped if the script is inside it
    /// * `limits` - File count and size limits
    fn extract(
        archive: &Path,
        staging: &Path,
        script: &Path,
        limits: &Limits,
    ) -> Result<Installed, Error> {
        let mut zip = zip::ZipArchive::new(fs::File::open(archive)?)?;

        // First pass: validate names and find a common wrapper folder.
        let mut names: Vec<Option<(PathBuf, bool)>> = Vec::with_capacity(zip.len());
        let mut files = 0usize;
        for index in 0..zip.len() {
            let entry = zip.by_index_raw(index)?;
            let raw = entry.name().map(|n| n.into_owned()).unwrap_or_default();
            let Some(path) = entry.enclosed_name() else {
                return Err(Error::UnsafePath(raw));
            };
            if path
                .components()
                .any(|c| !matches!(c, Component::Normal(_)))
            {
                return Err(Error::UnsafePath(raw));
            }
            if path.starts_with("__MACOSX") {
                names.push(None);
                continue;
            }
            if entry.is_symlink() {
                return Err(Error::Symlink(raw));
            }
            if !entry.is_dir() {
                files += 1;
                if files > limits.max_files {
                    return Err(Error::TooManyFiles(limits.max_files));
                }
            }
            names.push(Some((path, entry.is_dir())));
        }
        if files == 0 {
            return Err(Error::Empty);
        }
        // `workflows/run.nf` as script in an archive of just that folder keeps its folder.
        let strip = if names.iter().flatten().any(|(path, _)| path == script) {
            None
        } else {
            Self::wrapper_folder(&names)
        };

        // Second pass: write.
        fs::create_dir(staging)?;
        let mut seen = std::collections::HashSet::new();
        let mut installed = Installed { files: 0, bytes: 0 };
        for (index, name) in names.into_iter().enumerate() {
            let Some((path, _)) = name else { continue };
            let relative = match &strip {
                Some(prefix) => match path.strip_prefix(prefix) {
                    Ok(rest) => rest.to_path_buf(),
                    Err(_) => path.clone(),
                },
                None => path.clone(),
            };
            if relative.as_os_str().is_empty() {
                continue;
            }
            let mut entry = zip.by_index(index)?;
            let destination = staging.join(&relative);
            if entry.is_dir() {
                fs::create_dir_all(&destination)?;
                continue;
            }
            if !seen.insert(relative.clone()) {
                return Err(Error::Duplicate(relative.display().to_string()));
            }
            fs::create_dir_all(destination.parent().expect("file has a parent"))?;
            let mut out = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&destination)?;
            let remaining = limits.max_extracted_bytes - installed.bytes;
            let written = std::io::copy(&mut (&mut entry).take(remaining + 1), &mut out)?;
            if written > remaining {
                return Err(Error::TooLarge(limits.max_extracted_bytes));
            }
            out.flush()?;
            installed.bytes += written;
            installed.files += 1;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                // Only the executable bit is taken over (scripts in `bin/`), never setuid etc.
                let mode = if entry.unix_mode().is_some_and(|m| m & 0o100 != 0) {
                    0o755
                } else {
                    0o644
                };
                fs::set_permissions(&destination, fs::Permissions::from_mode(mode))?;
            }
        }
        Ok(installed)
    }

    /// The single top-level folder every entry lives in, if there is exactly one and no file
    /// sits next to it.
    ///
    /// # Arguments
    /// * `names` - Validated entry paths with a folder flag, `None` for skipped entries
    fn wrapper_folder(names: &[Option<(PathBuf, bool)>]) -> Option<PathBuf> {
        let mut folder: Option<PathBuf> = None;
        for (path, is_directory) in names.iter().flatten() {
            let mut components = path.components();
            let first = PathBuf::from(components.next()?.as_os_str());
            // A top-level file means there is no wrapper.
            if components.next().is_none() && !is_directory {
                return None;
            }
            match &folder {
                None => folder = Some(first),
                Some(existing) if *existing == first => {}
                Some(_) => return None,
            }
        }
        folder
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zip::write::SimpleFileOptions;

    const LIMITS: Limits = Limits {
        max_files: 100,
        max_extracted_bytes: 1024,
    };

    /// Writes a zip with the given `(name, content)` entries (names ending in `/` are folders).
    ///
    /// # Arguments
    /// * `path` - Archive to create
    /// * `entries` - Names and contents
    fn make_zip(path: &Path, entries: &[(&str, &str)]) {
        let mut writer = zip::ZipWriter::new(fs::File::create(path).unwrap());
        for (name, content) in entries {
            if name.ends_with('/') {
                writer
                    .add_directory(*name, SimpleFileOptions::default())
                    .unwrap();
            } else {
                writer
                    .start_file(*name, SimpleFileOptions::default().unix_permissions(0o755))
                    .unwrap();
                writer.write_all(content.as_bytes()).unwrap();
            }
        }
        writer.finish().unwrap();
    }

    /// A flat archive is installed as is and the executable bit is kept.
    #[test]
    fn flat_archive_is_installed() {
        let directory = tempfile::tempdir().unwrap();
        let archive = directory.path().join("a.zip");
        make_zip(&archive, &[("main.nf", "x"), ("bin/run.sh", "#!/bin/sh")]);
        let target = directory.path().join("local/7");
        let installed =
            WorkflowUpload::install(&archive, &target, Path::new("main.nf"), &LIMITS).unwrap();
        assert_eq!(
            installed,
            Installed {
                files: 2,
                bytes: 10
            }
        );
        assert_eq!(fs::read_to_string(target.join("main.nf")).unwrap(), "x");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(target.join("bin/run.sh"))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o755);
        }
    }

    /// A single wrapper folder is stripped and `__MACOSX` is dropped.
    #[test]
    fn wrapper_is_stripped() {
        let directory = tempfile::tempdir().unwrap();
        let archive = directory.path().join("a.zip");
        make_zip(
            &archive,
            &[
                ("hello/", ""),
                ("hello/main.nf", "v1"),
                ("hello/old.txt", "old"),
                ("__MACOSX/hello/._main.nf", "junk"),
            ],
        );
        let target = WorkflowUpload::new_version_directory(&directory.path().join("local/7"));
        WorkflowUpload::install(&archive, &target, Path::new("main.nf"), &LIMITS).unwrap();
        assert_eq!(fs::read_to_string(target.join("main.nf")).unwrap(), "v1");
        assert!(target.join("old.txt").is_file());
        assert!(!target.join("__MACOSX").exists());
        // No staging leftovers next to it.
        assert_eq!(fs::read_dir(target.parent().unwrap()).unwrap().count(), 1);
    }

    /// Each upload is a new version, the newest is current, and pruning removes the older ones
    /// except those in use; removing a workflow keeps only what is in use.
    #[test]
    fn versions_are_pinned_and_pruned() {
        use std::collections::HashSet;
        let directory = tempfile::tempdir().unwrap();
        let archive = directory.path().join("a.zip");
        let workflow = directory.path().join("local/7");
        assert_eq!(WorkflowUpload::current_version(&workflow), None);
        let mut versions = Vec::new();
        for content in ["1", "2", "3"] {
            make_zip(&archive, &[("main.nf", content)]);
            let target = WorkflowUpload::new_version_directory(&workflow);
            WorkflowUpload::install(&archive, &target, Path::new("main.nf"), &LIMITS).unwrap();
            versions.push(target);
        }
        // A stray file or folder is no version
        fs::write(workflow.join("note.txt"), "x").unwrap();
        fs::create_dir(workflow.join("not-a-version")).unwrap();
        assert_eq!(WorkflowUpload::versions(&workflow), versions);
        assert_eq!(
            WorkflowUpload::current_version(&workflow).as_ref(),
            versions.last()
        );

        let in_use = HashSet::from([versions[0].clone()]);
        assert_eq!(WorkflowUpload::prune(&workflow, &in_use), 1);
        assert!(versions[0].is_dir() && !versions[1].exists() && versions[2].is_dir());
        assert_eq!(WorkflowUpload::prune(&workflow, &in_use), 0);

        assert_eq!(WorkflowUpload::remove(&workflow, &in_use), 1);
        assert!(versions[0].is_dir() && !versions[2].exists());
        assert_eq!(WorkflowUpload::remove(&workflow, &HashSet::new()), 1);
        assert!(!workflow.exists());
    }

    /// A run of the older layout points at the workflow directory itself: nothing is removed.
    #[test]
    fn remove_keeps_a_directory_used_directly() {
        let directory = tempfile::tempdir().unwrap();
        let workflow = directory.path().join("local/7");
        fs::create_dir_all(&workflow).unwrap();
        fs::write(workflow.join("main.nf"), "x").unwrap();
        let in_use = std::collections::HashSet::from([workflow.clone()]);
        assert_eq!(WorkflowUpload::remove(&workflow, &in_use), 0);
        assert!(workflow.join("main.nf").is_file());
        WorkflowUpload::remove(&workflow, &Default::default());
        assert!(!workflow.exists());
    }

    /// Bad archives are rejected and leave the previous version alone.
    #[test]
    fn rejected_archives_keep_the_previous_version() {
        let directory = tempfile::tempdir().unwrap();
        let archive = directory.path().join("a.zip");
        let target = directory.path().join("local/7");
        make_zip(&archive, &[("main.nf", "good")]);
        WorkflowUpload::install(&archive, &target, Path::new("main.nf"), &LIMITS).unwrap();

        make_zip(&archive, &[("../evil.txt", "x")]);
        assert!(matches!(
            WorkflowUpload::install(&archive, &target, Path::new("main.nf"), &LIMITS),
            Err(Error::UnsafePath(_))
        ));
        make_zip(&archive, &[("a", "x"), ("b", "y")]);
        let few = Limits {
            max_files: 1,
            ..LIMITS
        };
        assert!(matches!(
            WorkflowUpload::install(&archive, &target, Path::new("main.nf"), &few),
            Err(Error::TooManyFiles(1))
        ));
        make_zip(&archive, &[("big", &"x".repeat(2000))]);
        assert!(matches!(
            WorkflowUpload::install(&archive, &target, Path::new("main.nf"), &LIMITS),
            Err(Error::TooLarge(_))
        ));
        make_zip(&archive, &[("empty/", "")]);
        assert!(matches!(
            WorkflowUpload::install(&archive, &target, Path::new("main.nf"), &LIMITS),
            Err(Error::Empty)
        ));
        make_zip(&archive, &[("other.nf", "x")]);
        assert!(matches!(
            WorkflowUpload::install(&archive, &target, Path::new("main.nf"), &LIMITS),
            Err(Error::MissingScript(_))
        ));
        fs::write(&archive, "not a zip").unwrap();
        assert!(matches!(
            WorkflowUpload::install(&archive, &target, Path::new("main.nf"), &LIMITS),
            Err(Error::Zip(_))
        ));

        assert_eq!(fs::read_to_string(target.join("main.nf")).unwrap(), "good");
        assert!(!directory.path().join("local/evil.txt").exists());
        assert!(!directory.path().join("evil.txt").exists());
        assert_eq!(fs::read_dir(target.parent().unwrap()).unwrap().count(), 1);
    }
}
