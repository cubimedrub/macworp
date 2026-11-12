//! Installing an uploaded workflow archive.
//!
//! Users upload local workflows as a zip file. The archive is untrusted: entries are extracted
//! into a staging directory with path, symlink, file count and size checks, and the result
//! replaces the workflow's directory (`<storage_path>/workflows/local/<workflow_integration_id>`) by
//! renames, so a failed upload leaves the previous version untouched.

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
    /// Extracts `archive` and replaces `target` by the result. A single top-level folder that
    /// holds everything (as made by "compress folder") is stripped; `__MACOSX` metadata is
    /// skipped. Blocking, call from `spawn_blocking`.
    ///
    /// # Arguments
    /// * `archive` - The uploaded zip file
    /// * `target` - Workflow directory to create or replace
    /// * `script` - Main script that must exist in the result (relative to the workflow)
    /// * `limits` - File count and size limits
    pub fn install(
        archive: &Path,
        target: &Path,
        script: &Path,
        limits: &Limits,
    ) -> Result<Installed, Error> {
        let parent = target.parent().expect("workflow directory has a parent");
        fs::create_dir_all(parent)?;
        let name = target.file_name().unwrap().to_string_lossy().into_owned();
        let unique = uuid::Uuid::now_v7();
        let staging = parent.join(format!(".{name}.new-{unique}"));

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

        let old = parent.join(format!(".{name}.old-{unique}"));
        let had_previous = target.exists();
        if had_previous {
            fs::rename(target, &old)?;
        }
        if let Err(error) = fs::rename(&staging, target) {
            if had_previous {
                let _ = fs::rename(&old, target);
            }
            let _ = fs::remove_dir_all(&staging);
            return Err(error.into());
        }
        if had_previous {
            let _ = fs::remove_dir_all(&old);
        }
        Ok(installed)
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

    /// A single wrapper folder is stripped, `__MACOSX` is dropped, and re-uploading replaces
    /// the previous content completely.
    #[test]
    fn wrapper_is_stripped_and_replaced() {
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
        let target = directory.path().join("local/7");
        WorkflowUpload::install(&archive, &target, Path::new("main.nf"), &LIMITS).unwrap();
        assert_eq!(fs::read_to_string(target.join("main.nf")).unwrap(), "v1");
        assert!(!target.join("__MACOSX").exists());

        make_zip(&archive, &[("main.nf", "v2")]);
        WorkflowUpload::install(&archive, &target, Path::new("main.nf"), &LIMITS).unwrap();
        assert_eq!(fs::read_to_string(target.join("main.nf")).unwrap(), "v2");
        assert!(!target.join("old.txt").exists());
        // No staging leftovers next to it.
        let leftovers: Vec<_> = fs::read_dir(target.parent().unwrap())
            .unwrap()
            .flatten()
            .map(|e| e.file_name())
            .collect();
        assert_eq!(leftovers.len(), 1);
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
