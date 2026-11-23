//! Keeps the Apptainer image cache from growing without bound.
//!
//! Snakemake pulls the images of its `container:` rules with Apptainer inside the engine
//! container and keeps them as `<md5>.simg` files in a cache directory shared by all runs (and,
//! on a server, by all workers on the shared storage). Nothing ever removes them. The runtime's
//! image store (see `image_store`) cannot see them: they are plain files.
//!
//! A run counts as having used a file when the file was written or read (modification or access
//! time) after the run started. Access times are only as exact as the file system keeps them
//! (`relatime` updates them at most daily, `noatime` never): then only files written during a run
//! count as used and the cache is trimmed oldest first. The last use of every file is kept in a
//! TOML file. When the cache exceeds the limit and no run of this store is active, the least
//! recently used files go, together with the marker file `apptainer_seed` writes next to a
//! seeded image. Files used within the grace period stay, because another worker may be using
//! them right now. Removing a file that is in use only costs a new download: an opened image
//! stays readable, and Snakemake pulls a missing one again.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::Mutex,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};

/// Files younger than this (by last use) are never removed
pub const DEFAULT_GRACE: Duration = Duration::from_secs(60 * 60);

/// Size limit of the cache in the desktop app, 20 GiB
pub const DESKTOP_MAX_BYTES: u64 = 20 * 1024 * 1024 * 1024;

/// Suffix of the images in the cache
const IMAGE_SUFFIX: &str = ".simg";
/// Suffix of the marker next to a seeded image
const MARKER_SUFFIX: &str = ".macworp-image";

/// What the state file holds.
#[derive(Debug, Default, Serialize, Deserialize)]
struct State {
    /// Last use per image file (absolute path), seconds since the Unix epoch
    files: BTreeMap<String, u64>,
}

/// Mutable part of the store.
#[derive(Debug, Default)]
struct Inner {
    state: State,
    /// Runs between `begin_run` and `end_run`
    active_runs: usize,
}

/// One image file found in the cache.
#[derive(Debug)]
struct CachedFile {
    path: PathBuf,
    /// Image plus marker file, bytes
    size: u64,
    /// Newest of modification and access time, seconds since the Unix epoch
    touched: u64,
}

/// Usage tracking and trimming of one Apptainer image cache.
#[derive(Debug)]
pub struct ApptainerCacheStore {
    /// Most bytes the cache may take, `None` = never remove
    max_bytes: Option<u64>,
    /// Least age of the last use before a file may be removed
    grace: Duration,
    /// Where the usage is persisted (TOML), `None` keeps it in memory only
    state_file: Option<PathBuf>,
    inner: Mutex<Inner>,
}

impl ApptainerCacheStore {
    /// Opens the store; a missing or unreadable state file means no history yet.
    ///
    /// # Arguments
    /// * `max_bytes` - size limit of the cache, `None` = unlimited
    /// * `state_file` - TOML file for the usage times, `None` = memory only
    pub fn new(max_bytes: Option<u64>, state_file: Option<PathBuf>) -> Self {
        let state = state_file.as_deref().map(Self::load).unwrap_or_default();
        Self {
            max_bytes,
            grace: DEFAULT_GRACE,
            state_file,
            inner: Mutex::new(Inner {
                state,
                active_runs: 0,
            }),
        }
    }

    /// Changes the grace period (tests use zero).
    ///
    /// # Arguments
    /// * `grace` - least age of the last use before a file may be removed
    pub fn with_grace(mut self, grace: Duration) -> Self {
        self.grace = grace;
        self
    }

    /// Default state file: `<state directory>/apptainer-cache.toml`.
    ///
    /// # Arguments
    /// * `directory` - explicit state directory, `None` = the default one
    pub fn state_file_in(directory: Option<&Path>) -> Option<PathBuf> {
        let directory = match directory {
            Some(directory) => directory.to_path_buf(),
            None => crate::image_store::ImageStore::default_state_directory()?,
        };
        Some(directory.join("apptainer-cache.toml"))
    }

    /// Reads the state file; problems are logged and give an empty state.
    ///
    /// # Arguments
    /// * `path` - the state file
    fn load(path: &Path) -> State {
        match std::fs::read_to_string(path) {
            Ok(text) => toml::from_str(&text).unwrap_or_else(|e| {
                tracing::warn!(
                    "Ignoring the unreadable cache usage file {}: {e}",
                    path.display()
                );
                State::default()
            }),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => State::default(),
            Err(e) => {
                tracing::warn!("Cannot read the cache usage file {}: {e}", path.display());
                State::default()
            }
        }
    }

    /// Writes the state through a temporary file.
    ///
    /// # Arguments
    /// * `state` - state to store
    fn save(&self, state: &State) {
        let Some(path) = &self.state_file else {
            return;
        };
        let result = (|| -> std::io::Result<()> {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let temporary = path.with_extension("toml.tmp");
            let text = toml::to_string_pretty(state)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
            std::fs::write(&temporary, text)?;
            std::fs::rename(temporary, path)
        })();
        if let Err(e) = result {
            tracing::warn!("Cannot write the cache usage file {}: {e}", path.display());
        }
    }

    /// Seconds since the Unix epoch
    fn now() -> u64 {
        Self::seconds(SystemTime::now())
    }

    /// Seconds since the Unix epoch of `time`, 0 before it.
    ///
    /// # Arguments
    /// * `time` - a point in time
    fn seconds(time: SystemTime) -> u64 {
        time.duration_since(UNIX_EPOCH)
            .map(|elapsed| elapsed.as_secs())
            .unwrap_or(0)
    }

    /// Marks the start of a run; returns its start time to hand to [`Self::end_run`]. No trimming
    /// happens until every run has ended.
    pub fn begin_run(&self) -> u64 {
        self.inner.lock().unwrap().active_runs += 1;
        Self::now().saturating_sub(1)
    }

    /// Marks the end of a run: stamps the files it used, then trims the cache when no other run
    /// is active. Problems are logged, they never fail the run.
    ///
    /// # Arguments
    /// * `directory` - the cache directory the run used
    /// * `started` - what `begin_run` returned
    pub async fn end_run(&self, directory: &Path, started: u64) {
        let idle = {
            let mut inner = self.inner.lock().unwrap();
            inner.active_runs = inner.active_runs.saturating_sub(1);
            inner.active_runs == 0
        };
        match self.sweep(directory, started, idle, Self::now()) {
            Ok(removed) if !removed.is_empty() => tracing::info!(
                "Removed {} unused image file(s) from the Apptainer cache",
                removed.len()
            ),
            Ok(_) => {}
            Err(e) => tracing::warn!(
                "Cannot trim the Apptainer cache {}: {e}",
                directory.display()
            ),
        }
    }

    /// Lists the image files of `directory` with size and last touch.
    ///
    /// # Arguments
    /// * `directory` - the cache directory
    fn scan(directory: &Path) -> std::io::Result<Vec<CachedFile>> {
        let mut files = Vec::new();
        for entry in std::fs::read_dir(directory)? {
            let entry = entry?;
            let path = entry.path();
            let is_image = path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.ends_with(IMAGE_SUFFIX));
            // `symlink_metadata`: a link is never followed out of the cache
            let metadata = std::fs::symlink_metadata(&path)?;
            if !is_image || !metadata.is_file() {
                continue;
            }
            let modified = metadata.modified().map(Self::seconds).unwrap_or(0);
            let accessed = metadata.accessed().map(Self::seconds).unwrap_or(0);
            let marker = Self::marker_of(&path);
            let marker_size = std::fs::metadata(&marker).map(|m| m.len()).unwrap_or(0);
            files.push(CachedFile {
                path,
                size: metadata.len() + marker_size,
                touched: modified.max(accessed),
            });
        }
        Ok(files)
    }

    /// The marker file of an image: `<hash>.simg` has `<hash>.simg.macworp-image`.
    ///
    /// # Arguments
    /// * `image` - path of the image file
    fn marker_of(image: &Path) -> PathBuf {
        let mut name = image.as_os_str().to_os_string();
        name.push(MARKER_SUFFIX);
        PathBuf::from(name)
    }

    /// Stamps used files and, when `trim` is set, removes the least recently used ones until the
    /// limit holds.
    ///
    /// # Arguments
    /// * `directory` - the cache directory
    /// * `started` - start of the run that ended; files touched since were used by it
    /// * `trim` - whether to remove files (no run is active)
    /// * `now` - current time, seconds since the Unix epoch
    ///
    /// # Returns
    /// The removed image files.
    fn sweep(
        &self,
        directory: &Path,
        started: u64,
        trim: bool,
        now: u64,
    ) -> std::io::Result<Vec<PathBuf>> {
        let files = Self::scan(directory)?;
        let mut inner = self.inner.lock().unwrap();
        let prefix = directory.to_string_lossy().to_string();
        let present: Vec<String> = files
            .iter()
            .map(|file| file.path.to_string_lossy().to_string())
            .collect();
        // Forget files of this directory that are gone
        inner
            .state
            .files
            .retain(|path, _| !path.starts_with(&prefix) || present.contains(path));
        for file in &files {
            let key = file.path.to_string_lossy().to_string();
            if file.touched >= started {
                inner.state.files.insert(key, now);
            } else {
                // First sight of a file nobody used in this run: it is as old as it looks
                inner.state.files.entry(key).or_insert(file.touched);
            }
        }

        let mut removed = Vec::new();
        if let (true, Some(max)) = (trim, self.max_bytes) {
            let mut total: u64 = files.iter().map(|file| file.size).sum();
            let mut order: Vec<&CachedFile> = files.iter().collect();
            order.sort_by_key(|file| {
                (
                    inner.state.files[&file.path.to_string_lossy().to_string()],
                    file.path.clone(),
                )
            });
            for file in order {
                if total <= max {
                    break;
                }
                let last_use = inner.state.files[&file.path.to_string_lossy().to_string()];
                if last_use + self.grace.as_secs() > now {
                    continue;
                }
                match std::fs::remove_file(&file.path) {
                    Ok(()) => {}
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                    Err(e) => {
                        tracing::warn!("Cannot remove {}: {e}", file.path.display());
                        continue;
                    }
                }
                let _ = std::fs::remove_file(Self::marker_of(&file.path));
                inner
                    .state
                    .files
                    .remove(&file.path.to_string_lossy().to_string());
                total = total.saturating_sub(file.size);
                removed.push(file.path.clone());
            }
        }
        self.save(&inner.state);
        Ok(removed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Writes an image of `size` bytes with a marker and sets its modification time.
    fn image(directory: &Path, name: &str, size: usize, age_seconds: u64) -> PathBuf {
        let path = directory.join(format!("{name}.simg"));
        std::fs::write(&path, vec![0u8; size]).unwrap();
        std::fs::write(ApptainerCacheStore::marker_of(&path), "src").unwrap();
        let time = SystemTime::now() - Duration::from_secs(age_seconds);
        std::fs::File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_times(
                std::fs::FileTimes::new()
                    .set_modified(time)
                    .set_accessed(time),
            )
            .unwrap();
        path
    }

    fn store(max: u64, state: Option<PathBuf>) -> ApptainerCacheStore {
        ApptainerCacheStore::new(Some(max), state).with_grace(Duration::from_secs(10))
    }

    /// The oldest files go first, with their markers, until the limit holds; other files stay
    #[test]
    fn trims_oldest_first_and_keeps_foreign_files() {
        let directory = tempfile::tempdir().unwrap();
        let oldest = image(directory.path(), "a", 100, 5000);
        let middle = image(directory.path(), "b", 100, 4000);
        let newest = image(directory.path(), "c", 100, 3000);
        std::fs::write(directory.path().join("notes.txt"), "x").unwrap();
        let store = store(250, None);
        let removed = store
            .sweep(
                directory.path(),
                ApptainerCacheStore::now(),
                true,
                ApptainerCacheStore::now(),
            )
            .unwrap();
        assert_eq!(removed, std::slice::from_ref(&oldest));
        assert!(!oldest.exists() && !ApptainerCacheStore::marker_of(&oldest).exists());
        assert!(middle.exists() && newest.exists());
        assert!(directory.path().join("notes.txt").exists());
    }

    /// A file touched since the run started is stamped as used now, so an older untouched one
    /// goes first even though it was written later than another
    #[test]
    fn files_used_by_the_run_are_kept() {
        let directory = tempfile::tempdir().unwrap();
        // Written long ago, but read by the run (touched after `started`)
        let used = image(directory.path(), "used", 100, 1000);
        let unused = image(directory.path(), "unused", 100, 5000);
        let store = store(150, None);
        let now = ApptainerCacheStore::now();
        let removed = store
            .sweep(directory.path(), now - 3000, true, now)
            .unwrap();
        assert_eq!(removed, [unused]);
        assert!(used.exists());
    }

    /// Nothing is removed inside the grace period, with no limit, or while a run is active
    #[test]
    fn grace_limit_and_active_runs_protect_files() {
        let directory = tempfile::tempdir().unwrap();
        let recent = image(directory.path(), "recent", 100, 2);
        let old = image(directory.path(), "old", 100, 5000);
        let now = ApptainerCacheStore::now();
        let busy = store(10, None);
        assert!(
            busy.sweep(directory.path(), now, false, now)
                .unwrap()
                .is_empty()
        );
        let unlimited = ApptainerCacheStore::new(None, None);
        assert!(
            unlimited
                .sweep(directory.path(), now, true, now)
                .unwrap()
                .is_empty()
        );
        let limited = store(10, None);
        assert_eq!(
            limited.sweep(directory.path(), now, true, now).unwrap(),
            [old]
        );
        assert!(recent.exists());
    }

    /// Usage is kept in a TOML file and survives a restart; a damaged file is ignored
    #[test]
    fn usage_is_persisted_as_toml() {
        let directory = tempfile::tempdir().unwrap();
        let cache = directory.path().join("cache");
        std::fs::create_dir(&cache).unwrap();
        let file = directory.path().join("state/apptainer-cache.toml");
        image(&cache, "a", 10, 5000);
        let first = store(1000, Some(file.clone()));
        let now = ApptainerCacheStore::now();
        first.sweep(&cache, now, false, now).unwrap();
        let text = std::fs::read_to_string(&file).unwrap();
        assert!(text.contains("[files]"), "{text}");
        let second = store(1000, Some(file.clone()));
        assert_eq!(second.inner.lock().unwrap().state.files.len(), 1);
        std::fs::write(&file, "files = [").unwrap();
        assert!(
            store(1000, Some(file))
                .inner
                .lock()
                .unwrap()
                .state
                .files
                .is_empty()
        );
    }

    /// Entries of files that disappeared are dropped
    #[test]
    fn vanished_files_are_forgotten() {
        let directory = tempfile::tempdir().unwrap();
        let gone = image(directory.path(), "gone", 10, 5000);
        let store = store(1000, None);
        let now = ApptainerCacheStore::now();
        store.sweep(directory.path(), now, false, now).unwrap();
        std::fs::remove_file(&gone).unwrap();
        store.sweep(directory.path(), now, false, now).unwrap();
        assert!(store.inner.lock().unwrap().state.files.is_empty());
    }
}
