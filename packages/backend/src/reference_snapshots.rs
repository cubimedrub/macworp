use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, SystemTime},
};

use macworp_core::entities::ReferenceFingerprint;
use thiserror::Error;

use crate::{
    filesystems::directory_files::DirectoryFiles, reference_directory::ReferenceDirectory,
};

/// Distinguishes snapshots created within the same nanosecond.
static COUNTER: AtomicU64 = AtomicU64::new(0);

/// A snapshot that was just created.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Snapshot {
    /// Folder name below the snapshot root, see [`ReferenceSnapshots::path_of`]
    pub name: String,
    /// What the snapshot contains, stored with the run
    pub fingerprint: ReferenceFingerprint,
}

/// Running totals while a tree is linked.
#[derive(Default)]
struct Totals {
    /// Files linked so far
    file_count: u64,
    /// Their summed size in bytes
    total_bytes: u64,
    /// Newest modification time seen
    last_modified: Option<SystemTime>,
}

/// Errors of creating, removing and sweeping reference snapshots.
#[derive(Error, Debug)]
pub enum Error {
    /// Reading the reference data or writing the snapshot failed
    #[error("Reference snapshot failed: {0}")]
    Io(#[from] std::io::Error),
    /// The reference data and the snapshot folder are on different filesystems, so hard links
    /// between them are impossible. Nothing is copied instead: copying would be slow and silent.
    #[error(
        "Reference data {reference} and reference snapshots {snapshots} are on different \
         filesystems, hard links are impossible: put both below the same filesystem"
    )]
    CrossesDevices {
        /// The live reference data
        reference: PathBuf,
        /// The folder the snapshot was to be created in
        snapshots: PathBuf,
    },
}

/// Point-in-time views of the global reference data, one per run.
///
/// A snapshot is a copy of the folder tree in which every file is a hard link to the live file.
/// That is a consistent view because every write to the reference data replaces a file by a new
/// one (rename over the target) or unlinks it, never writes into an existing file: later uploads
/// and deletions leave the content a snapshot links to untouched. A snapshot costs one link per
/// file and no data space until the live file is replaced. Symlinks and unfinished uploads are
/// not linked. The snapshot folder must be on the same filesystem as the reference data.
///
/// Admins changing the reference folder directly on disk, in place, bypass this.
#[derive(Clone, Debug)]
pub struct ReferenceSnapshots {
    /// Live reference data, followed when it moves
    reference_directory: ReferenceDirectory,
    /// Folder holding one subfolder per snapshot
    root: PathBuf,
}

impl ReferenceSnapshots {
    /// Snapshots of `reference_directory`, stored below `root`.
    ///
    /// # Arguments
    /// * `reference_directory` - Handle to the live reference data
    /// * `root` - Folder for the snapshots, created on demand
    pub fn new(reference_directory: ReferenceDirectory, root: PathBuf) -> Self {
        Self {
            reference_directory,
            root,
        }
    }

    /// Folder holding the snapshots.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Location of the snapshot called `name`.
    ///
    /// # Arguments
    /// * `name` - Name returned by [`Self::create`]
    pub fn path_of(&self, name: &str) -> PathBuf {
        self.root.join(name)
    }

    /// Creates a snapshot of the reference data as it is now and returns its name and
    /// fingerprint. Blocks on the filesystem, call it from `spawn_blocking`. A failed snapshot
    /// leaves nothing behind.
    pub fn create(&self) -> Result<Snapshot, Error> {
        let nanoseconds = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_nanos());
        let name = format!(
            "{nanoseconds:x}-{}",
            COUNTER.fetch_add(1, Ordering::Relaxed)
        );
        let destination = self.path_of(&name);
        std::fs::create_dir_all(&self.root)?;
        std::fs::create_dir(&destination)?;
        let reference = self.reference_directory.get();
        let mut totals = Totals::default();
        if let Err(error) = self.link_tree(&reference, &destination, &mut totals) {
            let _ = std::fs::remove_dir_all(&destination);
            return Err(error);
        }
        Ok(Snapshot {
            name,
            fingerprint: ReferenceFingerprint {
                file_count: totals.file_count,
                total_bytes: totals.total_bytes,
                last_modified: totals.last_modified.map(chrono::DateTime::from),
                snapshot_at: chrono::Utc::now(),
            },
        })
    }

    /// Recreates the folder tree below `source` in `destination` with hard links for files.
    ///
    /// # Arguments
    /// * `source` - Live folder to read
    /// * `destination` - Existing, empty folder to fill
    /// * `totals` - Counts every linked file
    fn link_tree(
        &self,
        source: &Path,
        destination: &Path,
        totals: &mut Totals,
    ) -> Result<(), Error> {
        for entry in std::fs::read_dir(source)? {
            let entry = entry?;
            let file_name = entry.file_name();
            if DirectoryFiles::is_unfinished_name(&file_name.to_string_lossy()) {
                continue;
            }
            // `file_type` does not follow symlinks: they are skipped, like the API rejects them
            let file_type = entry.file_type()?;
            let target = destination.join(&file_name);
            if file_type.is_dir() {
                std::fs::create_dir(&target)?;
                self.link_tree(&entry.path(), &target, totals)?;
            } else if file_type.is_file() {
                let metadata = entry.metadata()?;
                totals.file_count += 1;
                totals.total_bytes += metadata.len();
                totals.last_modified = totals.last_modified.max(metadata.modified().ok());
                std::fs::hard_link(entry.path(), &target)
                    .map_err(|error| self.link_error(error))?;
            }
        }
        Ok(())
    }

    /// Turns the failure of a hard link into an [`Error`]: a link across filesystems gets the
    /// dedicated error that says what to change.
    ///
    /// # Arguments
    /// * `error` - What `std::fs::hard_link` returned
    fn link_error(&self, error: std::io::Error) -> Error {
        if error.kind() == std::io::ErrorKind::CrossesDevices {
            Error::CrossesDevices {
                reference: self.reference_directory.get(),
                snapshots: self.root.clone(),
            }
        } else {
            Error::Io(error)
        }
    }

    /// Deletes the snapshot called `name`; a missing one is fine. Blocks on the filesystem.
    ///
    /// # Arguments
    /// * `name` - Name returned by [`Self::create`]
    pub fn remove(&self, name: &str) -> Result<(), Error> {
        match std::fs::remove_dir_all(self.path_of(name)) {
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error.into()),
            _ => Ok(()),
        }
    }

    /// Deletes every snapshot that is not in `in_use` and was created more than `grace` ago
    /// (a snapshot is made before its run is stored, the grace period protects that window).
    /// Returns how many were removed. Blocks on the filesystem.
    ///
    /// # Arguments
    /// * `in_use` - Names of the snapshots some run still needs
    /// * `grace` - Minimum age before an unreferenced snapshot is removed
    pub fn sweep(&self, in_use: &HashSet<String>, grace: Duration) -> Result<usize, Error> {
        let Ok(entries) = std::fs::read_dir(&self.root) else {
            return Ok(0);
        };
        let mut removed = 0;
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if in_use.contains(&name) {
                continue;
            }
            let old_enough = entry
                .metadata()
                .and_then(|metadata| metadata.modified())
                .ok()
                .and_then(|modified| modified.elapsed().ok())
                .is_some_and(|age| age >= grace);
            if old_enough && entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                self.remove(&name)?;
                removed += 1;
            }
        }
        Ok(removed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A reference folder with a nested file, a symlink and an unfinished upload, plus a
    /// snapshot handle whose folder is next to it.
    ///
    /// # Arguments
    /// * `directory` - Temporary folder everything lives in
    fn fixture(directory: &Path) -> (PathBuf, ReferenceSnapshots) {
        let reference = directory.join("reference");
        std::fs::create_dir_all(reference.join("genome")).unwrap();
        std::fs::write(reference.join("genome/genome.fa"), "fasta").unwrap();
        std::fs::write(
            reference.join("genome/.genome.fa.macworp_partial_upload"),
            "x",
        )
        .unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink("/etc/hosts", reference.join("link")).unwrap();
        let snapshots = ReferenceSnapshots::new(
            ReferenceDirectory::new(reference.clone()),
            directory.join("snapshots"),
        );
        (reference, snapshots)
    }

    /// The snapshot has the files, hard linked, and skips symlinks and unfinished uploads.
    #[test]
    fn snapshot_links_files_and_skips_symlinks_and_partial_uploads() {
        let directory = tempfile::tempdir().unwrap();
        let (_, snapshots) = fixture(directory.path());
        let name = snapshots.create().unwrap().name;
        let snapshot = snapshots.path_of(&name);
        assert_eq!(
            std::fs::read(snapshot.join("genome/genome.fa")).unwrap(),
            b"fasta"
        );
        assert!(!snapshot.join("link").exists());
        assert!(
            !snapshot
                .join("genome/.genome.fa.macworp_partial_upload")
                .exists()
        );
    }

    /// Replacing or deleting live files afterwards does not change the snapshot.
    #[test]
    fn snapshot_is_unaffected_by_later_replacement_and_deletion() {
        let directory = tempfile::tempdir().unwrap();
        let (reference, snapshots) = fixture(directory.path());
        let name = snapshots.create().unwrap().name;
        let snapshot = snapshots.path_of(&name);
        // replace like an upload does: new file renamed over the target
        std::fs::write(reference.join("genome/new"), "replaced").unwrap();
        std::fs::rename(
            reference.join("genome/new"),
            reference.join("genome/genome.fa"),
        )
        .unwrap();
        assert_eq!(
            std::fs::read(snapshot.join("genome/genome.fa")).unwrap(),
            b"fasta"
        );
        std::fs::remove_dir_all(reference.join("genome")).unwrap();
        assert_eq!(
            std::fs::read(snapshot.join("genome/genome.fa")).unwrap(),
            b"fasta"
        );
    }

    /// Files added after the snapshot are not in it.
    #[test]
    fn snapshot_does_not_see_later_files() {
        let directory = tempfile::tempdir().unwrap();
        let (reference, snapshots) = fixture(directory.path());
        let name = snapshots.create().unwrap().name;
        std::fs::write(reference.join("later.txt"), "x").unwrap();
        assert!(!snapshots.path_of(&name).join("later.txt").exists());
    }

    /// Two snapshots in a row get different names.
    #[test]
    fn snapshots_have_unique_names() {
        let directory = tempfile::tempdir().unwrap();
        let (_, snapshots) = fixture(directory.path());
        assert_ne!(
            snapshots.create().unwrap().name,
            snapshots.create().unwrap().name
        );
    }

    /// An unreadable reference folder fails and leaves no half-made snapshot.
    #[test]
    fn failed_snapshot_leaves_nothing_behind() {
        let directory = tempfile::tempdir().unwrap();
        let snapshots = ReferenceSnapshots::new(
            ReferenceDirectory::new(directory.path().join("missing")),
            directory.path().join("snapshots"),
        );
        assert!(snapshots.create().is_err());
        assert_eq!(std::fs::read_dir(snapshots.root()).unwrap().count(), 0);
    }

    /// Removing is idempotent and deletes the content.
    #[test]
    fn remove_deletes_the_snapshot_and_tolerates_a_missing_one() {
        let directory = tempfile::tempdir().unwrap();
        let (_, snapshots) = fixture(directory.path());
        let name = snapshots.create().unwrap().name;
        snapshots.remove(&name).unwrap();
        assert!(!snapshots.path_of(&name).exists());
        snapshots.remove(&name).unwrap();
    }

    /// The sweep keeps snapshots in use and young ones, removes old unreferenced ones.
    #[test]
    fn sweep_removes_only_old_unreferenced_snapshots() {
        let directory = tempfile::tempdir().unwrap();
        let (_, snapshots) = fixture(directory.path());
        let in_use = snapshots.create().unwrap().name;
        let unreferenced = snapshots.create().unwrap().name;
        let keep: HashSet<String> = [in_use.clone()].into();
        // young: protected by the grace period
        assert_eq!(
            snapshots.sweep(&keep, Duration::from_secs(3600)).unwrap(),
            0
        );
        assert!(snapshots.path_of(&unreferenced).exists());
        // no grace: only the unreferenced one goes
        assert_eq!(snapshots.sweep(&keep, Duration::ZERO).unwrap(), 1);
        assert!(snapshots.path_of(&in_use).exists());
        assert!(!snapshots.path_of(&unreferenced).exists());
    }

    /// The fingerprint counts files and bytes of the linked tree (not the skipped entries) and
    /// reports the newest modification time; an empty folder has no modification time.
    #[test]
    fn fingerprint_describes_the_snapshot() {
        let directory = tempfile::tempdir().unwrap();
        let (reference, snapshots) = fixture(directory.path());
        std::fs::write(reference.join("second.txt"), "123").unwrap();
        let fingerprint = snapshots.create().unwrap().fingerprint;
        // genome.fa ("fasta" = 5 bytes) and second.txt (3 bytes); symlink and partial skipped
        assert_eq!(fingerprint.file_count, 2);
        assert_eq!(fingerprint.total_bytes, 8);
        assert!(fingerprint.last_modified.unwrap() <= fingerprint.snapshot_at);

        let empty = ReferenceSnapshots::new(
            ReferenceDirectory::new(directory.path().join("empty")),
            directory.path().join("snapshots_of_empty"),
        );
        std::fs::create_dir(directory.path().join("empty")).unwrap();
        let fingerprint = empty.create().unwrap().fingerprint;
        assert_eq!(fingerprint.file_count, 0);
        assert_eq!(fingerprint.last_modified, None);
    }

    /// A link across filesystems is reported as [`Error::CrossesDevices`] naming both folders
    /// (the error the OS gives is mapped, so this holds on every platform), other failures stay
    /// plain I/O errors.
    #[test]
    fn link_across_filesystems_has_its_own_error() {
        let directory = tempfile::tempdir().unwrap();
        let (reference, snapshots) = fixture(directory.path());
        let crossing =
            snapshots.link_error(std::io::Error::from(std::io::ErrorKind::CrossesDevices));
        match crossing {
            Error::CrossesDevices {
                reference: shown_reference,
                snapshots: shown_snapshots,
            } => {
                assert_eq!(shown_reference, reference);
                assert_eq!(shown_snapshots, snapshots.root());
            }
            other => panic!("unexpected error: {other}"),
        }
        let message = snapshots
            .link_error(std::io::Error::from(std::io::ErrorKind::CrossesDevices))
            .to_string();
        assert!(message.contains("different filesystems"), "{message}");
        assert!(matches!(
            snapshots.link_error(std::io::Error::from(std::io::ErrorKind::PermissionDenied)),
            Error::Io(_)
        ));
    }

    /// With a snapshot folder on a really different filesystem (`/dev/shm` where it exists and
    /// is not the temporary folder's device) creating a snapshot is refused and leaves nothing
    /// behind, no silent copy. Skipped where no second filesystem is available.
    #[cfg(unix)]
    #[test]
    fn snapshot_on_another_filesystem_is_refused_without_copying() {
        use std::os::unix::fs::MetadataExt;

        let directory = tempfile::tempdir().unwrap();
        let (_, snapshots) = fixture(directory.path());
        let other = Path::new("/dev/shm");
        let same_device = |path: &Path| {
            std::fs::metadata(path).map(|metadata| metadata.dev()).ok()
                == std::fs::metadata(directory.path())
                    .map(|metadata| metadata.dev())
                    .ok()
        };
        if !other.is_dir() || same_device(other) {
            eprintln!("skipped: no second filesystem at /dev/shm");
            return;
        }
        let foreign_root = tempfile::tempdir_in(other).unwrap();
        let across = ReferenceSnapshots::new(
            snapshots.reference_directory.clone(),
            foreign_root.path().to_path_buf(),
        );
        let error = across.create().unwrap_err();
        assert!(matches!(error, Error::CrossesDevices { .. }), "{error}");
        assert_eq!(std::fs::read_dir(foreign_root.path()).unwrap().count(), 0);
    }
}
