//! Keeps the image store of a worker's container runtime from growing without bound.
//!
//! Engines pull images on their own (a Nextflow process names its container, a Snakemake rule
//! too), so the worker cannot know in advance which images a run needs. It learns afterwards:
//! when a run ended, the container `create` events of its time window name the images it used
//! (`ImageStore::end_run`). Those images get a last-used time in a small TOML file. When the
//! limits of the [`ImagePolicy`] are exceeded, the least recently used images are removed, but
//! only ones the worker has seen a run use: images a person pulled for other purposes are never
//! touched. Pinned images (the engine images by default) stay, and so do images a container still
//! uses, because removal never forces.
//!
//! Collection waits until no run of this store is active, since an engine pulls and starts its
//! tool images while it runs and an image it is about to start must not disappear.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, RwLock},
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};

use crate::container_runtime::{ContainerRuntime, ImageInfo, RuntimeError};

/// Image names that are never removed, whatever the configuration says: the engine images
/// (`macworp-nextflow:<version>` and the like). A trailing `*` matches any ending.
pub const BUILT_IN_PINS: &[&str] = &["macworp-*"];

/// When images are removed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ImagePolicy {
    /// Most images the worker keeps that runs used, `None` = no limit
    pub max_images: Option<usize>,
    /// Most bytes those images may take together (layers shared by several images count in each
    /// of them, so this over-estimates), `None` = no limit
    pub max_bytes: Option<u64>,
    /// Image names to keep (exact name, `repository` for every tag of it, or a prefix ending in
    /// `*`), in addition to [`BUILT_IN_PINS`]
    pub pinned: Vec<String>,
}

impl ImagePolicy {
    /// Limits for the desktop app, whose users should not have to know about container images:
    /// 40 images or 40 GiB, whichever is reached first. Generous for the engine images and the
    /// tools of a few workflows, small enough that a disk does not fill up over months.
    pub fn desktop_default() -> Self {
        Self {
            max_images: Some(40),
            max_bytes: Some(40 * 1024 * 1024 * 1024),
            pinned: Vec::new(),
        }
    }

    /// Whether the policy removes anything at all
    pub fn is_enabled(&self) -> bool {
        self.max_images.is_some() || self.max_bytes.is_some()
    }

    /// Whether `image` must be kept because one of its names is pinned.
    ///
    /// # Arguments
    /// * `image` - image to check
    pub fn is_pinned(&self, image: &ImageInfo) -> bool {
        let patterns = BUILT_IN_PINS
            .iter()
            .copied()
            .chain(self.pinned.iter().map(String::as_str));
        patterns.into_iter().any(|pattern| {
            image
                .names
                .iter()
                .any(|name| Self::pattern_matches(pattern, name))
        })
    }

    /// One pin against one image name; `localhost/` and `docker.io/library/` are ignored.
    ///
    /// # Arguments
    /// * `pattern` - exact name, repository, or prefix ending in `*`
    /// * `name` - image name
    fn pattern_matches(pattern: &str, name: &str) -> bool {
        // Podman names locally built images `localhost/<name>`
        let name = name.strip_prefix("localhost/").unwrap_or(name);
        let name = name
            .strip_prefix("docker.io/")
            .map(|rest| rest.strip_prefix("library/").unwrap_or(rest))
            .unwrap_or(name);
        if let Some(prefix) = pattern.strip_suffix('*') {
            return name.starts_with(prefix);
        }
        name == pattern
            || name
                .rsplit_once(':')
                .is_some_and(|(repository, _)| repository == pattern)
    }

    /// Which images to remove, least recently used first, until the limits hold again.
    ///
    /// Only images with a recorded use are candidates and counted; pinned ones count towards the
    /// limits but are never chosen, so the limits can stay exceeded.
    ///
    /// # Arguments
    /// * `images` - everything the runtime has
    /// * `usage` - last use per image ID, seconds since the Unix epoch
    pub fn plan(&self, images: &[ImageInfo], usage: &BTreeMap<String, u64>) -> Vec<ImageInfo> {
        let mut tracked: Vec<&ImageInfo> = images
            .iter()
            .filter(|image| usage.contains_key(&image.id))
            .collect();
        let mut count = tracked.len();
        let mut bytes: u64 = tracked.iter().map(|image| image.size_bytes).sum();
        // Oldest use first, the ID makes ties deterministic
        tracked.sort_by_key(|image| (usage[&image.id], image.id.clone()));

        let mut chosen = Vec::new();
        for image in tracked {
            let over_count = self.max_images.is_some_and(|max| count > max);
            let over_bytes = self.max_bytes.is_some_and(|max| bytes > max);
            if !over_count && !over_bytes {
                break;
            }
            if self.is_pinned(image) {
                continue;
            }
            count -= 1;
            bytes = bytes.saturating_sub(image.size_bytes);
            chosen.push(image.clone());
        }
        chosen
    }
}

/// What the state file holds.
#[derive(Debug, Default, Serialize, Deserialize)]
struct State {
    /// Last use per image ID, seconds since the Unix epoch
    images: BTreeMap<String, u64>,
}

/// Mutable part of the store, behind one lock.
#[derive(Debug, Default)]
struct Inner {
    state: State,
    /// Runs between `begin_run` and `end_run`
    active_runs: usize,
}

/// What the tracked images take together.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ImageUsage {
    /// Number of tracked images
    pub images: usize,
    /// Their size in bytes
    pub bytes: u64,
}

/// Result of one collection.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Collected {
    /// Images that were removed
    pub removed: Vec<String>,
    /// Images that were chosen but could not be removed, usually because a container uses them
    pub kept: Vec<String>,
}

/// Proof that a run is being tracked, passed back to [`ImageStore::end_run`].
#[derive(Debug)]
pub struct RunTracking {
    /// Start of the run, seconds since the Unix epoch
    started: u64,
}

/// Usage tracking and garbage collection for the images of one container runtime.
#[derive(Debug)]
pub struct ImageStore {
    runtime: Arc<dyn ContainerRuntime>,
    /// Current limits; replaceable while the worker runs ([`ImageStore::set_policy`])
    policy: RwLock<ImagePolicy>,
    /// Where the usage is persisted (TOML), `None` keeps it in memory only
    state_file: Option<PathBuf>,
    inner: Mutex<Inner>,
}

impl ImageStore {
    /// Opens the store; a missing or unreadable state file means no history yet.
    ///
    /// # Arguments
    /// * `runtime` - runtime whose images are managed
    /// * `policy` - when to remove images
    /// * `state_file` - JSON file for the usage times, `None` = memory only
    pub fn new(
        runtime: Arc<dyn ContainerRuntime>,
        policy: ImagePolicy,
        state_file: Option<PathBuf>,
    ) -> Self {
        let state = state_file.as_deref().map(Self::load).unwrap_or_default();
        Self {
            runtime,
            policy: RwLock::new(policy),
            state_file,
            inner: Mutex::new(Inner {
                state,
                active_runs: 0,
            }),
        }
    }

    /// The limits in force now.
    pub fn policy(&self) -> ImagePolicy {
        self.policy
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// Replaces the limits; the next collection uses them. A run in progress is not affected
    /// beyond that, since collection only happens while no run is active.
    ///
    /// # Arguments
    /// * `policy` - the new limits
    pub fn set_policy(&self, policy: ImagePolicy) {
        *self.policy.write().unwrap_or_else(|e| e.into_inner()) = policy;
    }

    /// What the images this store tracks take: how many and their size together (layers shared
    /// by several images count in each, as for the limit).
    pub async fn usage(&self) -> Result<ImageUsage, RuntimeError> {
        let images = self.runtime.list_images().await?;
        let inner = self.inner.lock().unwrap();
        let tracked = images
            .iter()
            .filter(|image| inner.state.images.contains_key(&image.id));
        Ok(
            tracked.fold(ImageUsage::default(), |usage, image| ImageUsage {
                images: usage.images + 1,
                bytes: usage.bytes + image.size_bytes,
            }),
        )
    }

    /// Whether no run of this store is active, so a collection may start.
    pub fn is_idle(&self) -> bool {
        self.inner.lock().unwrap().active_runs == 0
    }

    /// The runtime this store manages
    pub fn runtime(&self) -> &Arc<dyn ContainerRuntime> {
        &self.runtime
    }

    /// Default state file for a runtime: `<state directory>/images-<runtime>.toml`, where the
    /// directory is `$XDG_STATE_HOME/macworp`, else `$HOME/.local/state/macworp`. `None` when
    /// neither variable is set.
    ///
    /// # Arguments
    /// * `directory` - explicit state directory, overrides the default
    /// * `runtime` - runtime name, so a worker using several keeps their usage apart
    pub fn state_file_in(directory: Option<&Path>, runtime: &str) -> Option<PathBuf> {
        let directory = match directory {
            Some(directory) => directory.to_path_buf(),
            None => Self::default_state_directory()?,
        };
        Some(directory.join(format!("images-{runtime}.toml")))
    }

    /// `$XDG_STATE_HOME/macworp`, else `$HOME/.local/state/macworp`; `None` when neither
    /// variable is set.
    pub fn default_state_directory() -> Option<PathBuf> {
        std::env::var_os("XDG_STATE_HOME")
            .filter(|value| !value.is_empty())
            .map(|state| PathBuf::from(state).join("macworp"))
            .or_else(|| {
                std::env::var_os("HOME")
                    .filter(|value| !value.is_empty())
                    .map(|home| PathBuf::from(home).join(".local/state/macworp"))
            })
    }

    /// Reads the state file; problems are logged and give an empty state.
    ///
    /// # Arguments
    /// * `path` - the state file
    fn load(path: &Path) -> State {
        match std::fs::read_to_string(path) {
            Ok(text) => toml::from_str(&text).unwrap_or_else(|e| {
                tracing::warn!(
                    "Ignoring the unreadable image usage file {}: {e}",
                    path.display()
                );
                State::default()
            }),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => State::default(),
            Err(e) => {
                tracing::warn!("Cannot read the image usage file {}: {e}", path.display());
                State::default()
            }
        }
    }

    /// Writes the state through a temporary file, so a crash leaves the old or the new content.
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
            tracing::warn!("Cannot write the image usage file {}: {e}", path.display());
        }
    }

    /// Seconds since the Unix epoch
    fn now() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|elapsed| elapsed.as_secs())
            .unwrap_or(0)
    }

    /// Marks the start of a run. No collection happens until it ended.
    pub fn begin_run(&self) -> RunTracking {
        self.inner.lock().unwrap().active_runs += 1;
        RunTracking {
            // A second of slack so a container created in the start second is still inside
            started: Self::now().saturating_sub(1),
        }
    }

    /// Marks the end of a run: records the images its containers used, then collects when no
    /// other run is active. Failures are logged, they never fail the run.
    ///
    /// # Arguments
    /// * `tracking` - what `begin_run` returned
    pub async fn end_run(&self, tracking: RunTracking) {
        let until = Self::now() + 1;
        match self
            .runtime
            .images_used_between(tracking.started, until)
            .await
        {
            Ok(references) => match self.runtime.list_images().await {
                Ok(images) => self.record_use(&images, &references, until),
                Err(e) => tracing::warn!("Cannot list images to record their use: {e}"),
            },
            Err(RuntimeError::Unsupported(_)) => {}
            Err(e) => tracing::warn!("Cannot read the container events of the run: {e}"),
        }
        let idle = {
            let mut inner = self.inner.lock().unwrap();
            inner.active_runs = inner.active_runs.saturating_sub(1);
            inner.active_runs == 0
        };
        if idle && self.policy().is_enabled() {
            match self.collect().await {
                Ok(collected) if !collected.removed.is_empty() => tracing::info!(
                    "Removed {} unused image(s): {}",
                    collected.removed.len(),
                    collected.removed.join(", ")
                ),
                Ok(_) => {}
                Err(e) => tracing::warn!("Image garbage collection failed: {e}"),
            }
        }
    }

    /// Stamps the images that `references` name with `time` and stores the usage.
    ///
    /// # Arguments
    /// * `images` - what the runtime has
    /// * `references` - image names or IDs containers were created from
    /// * `time` - last-use time to record
    fn record_use(&self, images: &[ImageInfo], references: &[String], time: u64) {
        let mut inner = self.inner.lock().unwrap();
        for image in images {
            if references.iter().any(|reference| image.matches(reference)) {
                inner.state.images.insert(image.id.clone(), time);
            }
        }
        self.save(&inner.state);
    }

    /// Removes the least recently used images until the policy holds. Images the runtime no
    /// longer has are forgotten. Removal never forces, so an image a container uses stays.
    pub async fn collect(&self) -> Result<Collected, RuntimeError> {
        let images = self.runtime.list_images().await?;
        let policy = self.policy();
        let (chosen, usage) = {
            let mut inner = self.inner.lock().unwrap();
            let before = inner.state.images.len();
            inner
                .state
                .images
                .retain(|id, _| images.iter().any(|image| &image.id == id));
            if inner.state.images.len() != before {
                self.save(&inner.state);
            }
            (
                policy.plan(&images, &inner.state.images),
                inner.state.images.clone(),
            )
        };
        let mut collected = Collected::default();
        for image in chosen {
            let label = image
                .names
                .first()
                .cloned()
                .unwrap_or_else(|| image.id.clone());
            match self.runtime.remove_image(&image).await {
                Ok(()) => collected.removed.push(label),
                Err(e) => {
                    tracing::debug!("Image {label} stays: {e}");
                    collected.kept.push(label);
                }
            }
        }
        if !collected.removed.is_empty() {
            let mut inner = self.inner.lock().unwrap();
            for image in images.iter().filter(|image| {
                usage.contains_key(&image.id)
                    && image
                        .names
                        .first()
                        .or(Some(&image.id))
                        .is_some_and(|label| collected.removed.contains(label))
            }) {
                inner.state.images.remove(&image.id);
            }
            self.save(&inner.state);
        }
        Ok(collected)
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::container_runtime::{BoxFuture, RuntimeInfo, RuntimeKind};

    fn image(id: &str, names: &[&str], size: u64) -> ImageInfo {
        ImageInfo {
            id: id.into(),
            names: names.iter().map(|name| name.to_string()).collect(),
            size_bytes: size,
        }
    }

    fn usage(entries: &[(&str, u64)]) -> BTreeMap<String, u64> {
        entries
            .iter()
            .map(|(id, time)| (id.to_string(), *time))
            .collect()
    }

    fn ids(images: &[ImageInfo]) -> Vec<&str> {
        images.iter().map(|image| image.id.as_str()).collect()
    }

    /// The oldest tracked images go first until the count holds
    #[test]
    fn plan_evicts_least_recently_used_by_count() {
        let policy = ImagePolicy {
            max_images: Some(2),
            ..Default::default()
        };
        let images = [
            image("a", &["a:1"], 1),
            image("b", &["b:1"], 1),
            image("c", &["c:1"], 1),
            image("d", &["d:1"], 1),
        ];
        let used = usage(&[("a", 40), ("b", 10), ("c", 30), ("d", 20)]);
        assert_eq!(ids(&policy.plan(&images, &used)), ["b", "d"]);
    }

    /// The byte limit works like the count limit; both together evict until both hold
    #[test]
    fn plan_honours_the_byte_limit() {
        let policy = ImagePolicy {
            max_bytes: Some(100),
            ..Default::default()
        };
        let images = [
            image("a", &["a:1"], 80),
            image("b", &["b:1"], 60),
            image("c", &["c:1"], 10),
        ];
        let used = usage(&[("a", 1), ("b", 2), ("c", 3)]);
        // 150 bytes: dropping `a` leaves 70
        assert_eq!(ids(&policy.plan(&images, &used)), ["a"]);
    }

    /// Images no run used are not ours: they are neither counted nor removed
    #[test]
    fn plan_ignores_images_without_recorded_use() {
        let policy = ImagePolicy {
            max_images: Some(1),
            ..Default::default()
        };
        let images = [
            image("mine", &["mine:1"], 1),
            image("other", &["other:1"], 1),
            image("another", &["another:1"], 1),
        ];
        let used = usage(&[("mine", 5)]);
        assert!(policy.plan(&images, &used).is_empty());
    }

    /// Engine images and configured pins stay, the next oldest goes instead
    #[test]
    fn plan_skips_pinned_images() {
        let policy = ImagePolicy {
            max_images: Some(1),
            pinned: vec!["keep/tool".into(), "ghcr.io/org/*".into()],
            ..Default::default()
        };
        let images = [
            image("engine", &["macworp-nextflow:25.04.6"], 1),
            image("tool", &["keep/tool:2"], 1),
            image("org", &["ghcr.io/org/x:1"], 1),
            image("plain", &["plain:1"], 1),
            image("newer", &["newer:1"], 1),
        ];
        let used = usage(&[
            ("engine", 1),
            ("tool", 2),
            ("org", 3),
            ("plain", 4),
            ("newer", 5),
        ]);
        assert_eq!(ids(&policy.plan(&images, &used)), ["plain", "newer"]);
    }

    /// An image with several names is pinned if any of its names is
    #[test]
    fn pin_matches_any_name_and_ignores_the_docker_hub_prefix() {
        let policy = ImagePolicy {
            pinned: vec!["alpine".into()],
            ..Default::default()
        };
        assert!(policy.is_pinned(&image("x", &["other:1", "docker.io/library/alpine:3"], 1)));
        assert!(!policy.is_pinned(&image("x", &["alpine-extra:1"], 1)));
        assert!(!policy.is_pinned(&image("x", &[], 1)));
        // Podman's name for an engine image built locally
        assert!(ImagePolicy::default().is_pinned(&image(
            "x",
            &["localhost/macworp-cwl:latest"],
            1
        )));
    }

    /// A runtime holding a list of images, removing from it unless the image is "in use"
    #[derive(Debug)]
    struct FakeRuntime {
        images: Mutex<Vec<ImageInfo>>,
        events: Vec<String>,
        busy: Vec<String>,
    }

    impl ContainerRuntime for FakeRuntime {
        fn kind(&self) -> RuntimeKind {
            RuntimeKind::Podman
        }
        fn binary(&self) -> &Path {
            Path::new("fake")
        }
        fn info(&self) -> BoxFuture<'_, Result<RuntimeInfo, RuntimeError>> {
            Box::pin(async { Err(RuntimeError::Unsupported("info")) })
        }
        fn list_images(&self) -> BoxFuture<'_, Result<Vec<ImageInfo>, RuntimeError>> {
            Box::pin(async { Ok(self.images.lock().unwrap().clone()) })
        }
        fn remove_image(&self, image: &ImageInfo) -> BoxFuture<'_, Result<(), RuntimeError>> {
            let image = image.clone();
            Box::pin(async move {
                if image.names.iter().any(|name| self.busy.contains(name)) {
                    return Err(RuntimeError::Unsupported("removing an image in use"));
                }
                self.images
                    .lock()
                    .unwrap()
                    .retain(|known| known.id != image.id);
                Ok(())
            })
        }
        fn images_used_between(
            &self,
            _since: u64,
            _until: u64,
        ) -> BoxFuture<'_, Result<Vec<String>, RuntimeError>> {
            Box::pin(async { Ok(self.events.clone()) })
        }
    }

    fn store(
        runtime: FakeRuntime,
        max_images: usize,
        state_file: Option<PathBuf>,
    ) -> (Arc<FakeRuntime>, ImageStore) {
        let runtime = Arc::new(runtime);
        let store = ImageStore::new(
            runtime.clone(),
            ImagePolicy {
                max_images: Some(max_images),
                ..Default::default()
            },
            state_file,
        );
        (runtime, store)
    }

    /// A run records the images its containers used; once idle the oldest surplus image goes,
    /// an image that is still in use stays
    #[tokio::test]
    async fn end_run_records_use_and_collects() {
        let runtime = FakeRuntime {
            images: Mutex::new(vec![
                image("old", &["old:1"], 1),
                image("busy", &["busy:1"], 1),
                image("fresh", &["fresh:1"], 1),
                image("foreign", &["foreign:1"], 1),
            ]),
            events: vec!["fresh:1".into()],
            busy: vec!["busy:1".into()],
        };
        let (runtime, store) = store(runtime, 1, None);
        {
            let mut inner = store.inner.lock().unwrap();
            inner.state.images.insert("old".into(), 1);
            inner.state.images.insert("busy".into(), 2);
        }
        let tracking = store.begin_run();
        store.end_run(tracking).await;
        let remaining: Vec<String> = runtime
            .images
            .lock()
            .unwrap()
            .iter()
            .map(|image| image.id.clone())
            .collect();
        assert_eq!(remaining, ["busy", "fresh", "foreign"]);
    }

    /// While another run is active nothing is collected
    #[tokio::test]
    async fn collection_waits_for_the_last_run() {
        let runtime = FakeRuntime {
            images: Mutex::new(vec![image("a", &["a:1"], 1), image("b", &["b:1"], 1)]),
            events: vec!["a:1".into(), "b:1".into()],
            busy: vec![],
        };
        let (runtime, store) = store(runtime, 1, None);
        let first = store.begin_run();
        let second = store.begin_run();
        store.end_run(first).await;
        assert_eq!(runtime.images.lock().unwrap().len(), 2);
        store.end_run(second).await;
        assert_eq!(runtime.images.lock().unwrap().len(), 1);
    }

    /// A policy replaced while the store lives is used by the next collection, and usage counts
    /// only tracked images
    #[tokio::test]
    async fn policy_change_applies_to_the_next_collection() {
        let runtime = FakeRuntime {
            images: Mutex::new(vec![
                image("a", &["a:1"], 10),
                image("b", &["b:1"], 20),
                image("foreign", &["foreign:1"], 99),
            ]),
            events: vec![],
            busy: vec![],
        };
        let (runtime, store) = store(runtime, 10, None);
        {
            let mut inner = store.inner.lock().unwrap();
            inner.state.images.insert("a".into(), 1);
            inner.state.images.insert("b".into(), 2);
        }
        assert_eq!(
            store.usage().await.unwrap(),
            ImageUsage {
                images: 2,
                bytes: 30
            }
        );
        assert!(store.collect().await.unwrap().removed.is_empty());

        store.set_policy(ImagePolicy {
            max_images: Some(1),
            ..Default::default()
        });
        assert_eq!(store.policy().max_images, Some(1));
        assert_eq!(store.collect().await.unwrap().removed, ["a:1"]);
        assert_eq!(runtime.images.lock().unwrap().len(), 2);
    }

    /// Usage survives a restart (as TOML); a corrupt file is ignored
    #[tokio::test]
    async fn usage_is_persisted() {
        let directory = tempfile::tempdir().unwrap();
        let file = directory.path().join("nested/images.toml");
        let runtime = || FakeRuntime {
            images: Mutex::new(vec![image("a", &["a:1"], 1)]),
            events: vec!["a:1".into()],
            busy: vec![],
        };
        let (_, first) = store(runtime(), 10, Some(file.clone()));
        let tracking = first.begin_run();
        first.end_run(tracking).await;
        let (_, second) = store(runtime(), 10, Some(file.clone()));
        assert!(second.inner.lock().unwrap().state.images.contains_key("a"));

        std::fs::write(&file, "images = [").unwrap();
        let (_, third) = store(runtime(), 10, Some(file));
        assert!(third.inner.lock().unwrap().state.images.is_empty());
    }

    /// Images the runtime lost are dropped from the usage
    #[tokio::test]
    async fn vanished_images_are_forgotten() {
        let runtime = FakeRuntime {
            images: Mutex::new(vec![image("a", &["a:1"], 1)]),
            events: vec![],
            busy: vec![],
        };
        let (_, store) = store(runtime, 10, None);
        store
            .inner
            .lock()
            .unwrap()
            .state
            .images
            .insert("gone".into(), 1);
        store.collect().await.unwrap();
        assert!(store.inner.lock().unwrap().state.images.is_empty());
    }

    /// Against a real runtime (needs podman or docker and the `alpine` image): a unique image is
    /// built, a container is started from it, the run is tracked, and a limit of zero images
    /// removes exactly that image again. Other images are not tracked, so they stay.
    #[tokio::test]
    #[ignore = "needs podman or docker with the alpine image"]
    async fn removes_a_used_image_in_a_real_runtime() {
        use crate::container_runtime::CliRuntime;
        use std::process::Stdio;

        // `MACWORP_CONTAINER_RUNTIME=docker` picks Docker, else Podman is preferred
        let kind = match std::env::var("MACWORP_CONTAINER_RUNTIME").as_deref() {
            Ok("docker") => RuntimeKind::Docker,
            Ok("podman") => RuntimeKind::Podman,
            _ if std::process::Command::new("podman")
                .arg("--version")
                .output()
                .is_ok() =>
            {
                RuntimeKind::Podman
            }
            _ => RuntimeKind::Docker,
        };
        let binary = kind.binary_name();
        let name = format!("macworp-gc-probe-{}:1", std::process::id());
        // `macworp-*` is pinned, so the probe must not carry that prefix
        let name = name.replacen("macworp-", "", 1);
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(
            directory.path().join("Dockerfile"),
            format!("FROM alpine\nRUN echo {name} > /probe\n"),
        )
        .unwrap();
        let built = std::process::Command::new(binary)
            .args(["build", "-q", "-t", &name])
            .arg(directory.path())
            .stdout(Stdio::null())
            .status()
            .unwrap();
        assert!(built.success());

        let runtime: Arc<dyn ContainerRuntime> = Arc::new(CliRuntime::new(kind));
        let state = directory.path().join("images.toml");
        let store = ImageStore::new(
            runtime.clone(),
            ImagePolicy {
                max_images: Some(0),
                ..Default::default()
            },
            Some(state.clone()),
        );
        let tracking = store.begin_run();
        let ran = std::process::Command::new(binary)
            .args(["run", "--rm", &name, "true"])
            .status()
            .unwrap();
        assert!(ran.success());
        // Events are second-granular: make sure the window includes the container
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        store.end_run(tracking).await;

        let remaining = runtime.list_images().await.unwrap();
        let still_there = remaining
            .iter()
            .any(|image| image.names.iter().any(|n| n.ends_with(&name)));
        if still_there {
            let _ = std::process::Command::new(binary)
                .args(["rmi", &name])
                .status();
        }
        assert!(!still_there, "the probe image was not removed");
        assert!(
            std::fs::read_to_string(&state)
                .unwrap()
                .contains("[images]")
        );
    }
}
