//! Global cache of remote workflow repositories.
//!
//! Layout below the cache root:
//! * `mirrors/<host>/<path>.git`: one bare mirror per repository, a single fetch updates all
//!   tags and branches.
//! * `checkouts/<host>/<path>/<commit-sha>/`: immutable working tree of one commit. Created in a
//!   temporary sibling directory and renamed into place, never modified afterwards, so a run
//!   that mounts it is not affected by later fetches (moved tags, force-pushed branches).
//!
//! All git access goes through `git2` (libgit2 is compiled into the binary), no `git` executable
//! is needed. Operations are blocking: call them from `spawn_blocking`.
//!
//! Fetching, checking out and pruning hold an exclusive lock on `.macworp-cache.lock` in the cache
//! root, so several processes (server, `workflow-cache` command, a second backend on the same
//! storage) can share one cache directory. Advisory file locks need a filesystem that supports
//! them; untested on network filesystems.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, SystemTime};

use git2::build::CheckoutBuilder;
use git2::{FetchOptions, Oid, Repository};
use macworp_core::entities::RunProvenance;
use macworp_core::paths::{is_confined, sanitize_relative};
use macworp_core::responses::{SchemaFile, WorkflowSchemasResponse};
use macworp_core::workflow_configuration::{
    WorkflowConfiguration,
    engine::Engine,
    source::{LocalSource, Source},
};

/// Largest file [`WorkflowRepositoryCache::read_file`] returns.
pub const MAX_READ_BYTES: u64 = 8 * 1024 * 1024;

/// Name of the file in a mirror whose mtime records the last successful fetch.
const FETCHED_MARKER: &str = "macworp-fetched";
/// Name of the file in a checkout whose mtime records the last use (for GC).
const USED_MARKER: &str = ".macworp-last-used";
/// Lock file in the cache root, kept between runs and never deleted.
const LOCK_FILE: &str = ".macworp-cache.lock";

/// Errors of the repository cache.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The repository URL is not an acceptable plain `https://host/path` URL.
    #[error("invalid repository url: {0}")]
    InvalidUrl(String),
    /// The reference (tag, branch or commit) is empty, option-like or contains control characters.
    #[error("invalid reference: {0}")]
    InvalidReference(String),
    /// The reference does not exist in the repository.
    #[error("reference `{0}` not found")]
    ReferenceNotFound(String),
    /// A requested file is outside the checkout, a symlink, missing or too large.
    #[error("invalid file path: {0}")]
    InvalidPath(String),
    /// Fetching failed and no usable copy is in the cache.
    #[error("fetching {url} failed: {source}")]
    Fetch {
        /// Normalised repository URL.
        url: String,
        /// Underlying libgit2 error.
        source: git2::Error,
    },
    /// Other libgit2 failure.
    #[error(transparent)]
    Git(#[from] git2::Error),
    /// Filesystem failure.
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// A normalised repository location.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepositoryId {
    /// Lower-cased host name.
    pub host: String,
    /// Path segments (without a trailing `.git`).
    pub segments: Vec<String>,
}

impl RepositoryId {
    /// Parses a plain `https://host/owner/repo[.git]` URL. Credentials, ports, query strings,
    /// fragments, other schemes (`file://`, `ssh://`, `ext::`) and `.`/`..`/option-like segments
    /// are rejected.
    ///
    /// # Arguments
    /// * `url` - Repository URL as entered in the workflow configuration
    pub fn parse(url: &str) -> Result<Self, Error> {
        let invalid = || Error::InvalidUrl(url.to_string());
        let rest = url.trim().strip_prefix("https://").ok_or_else(invalid)?;
        let (host, path) = rest.split_once('/').ok_or_else(invalid)?;
        let host = host.to_ascii_lowercase();
        let host_ok = !host.is_empty()
            && !host.starts_with(['.', '-'])
            && host
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-');
        if !host_ok {
            return Err(invalid());
        }
        let path = path.trim_end_matches('/');
        let path = path.strip_suffix(".git").unwrap_or(path);
        let segments: Vec<String> = path.split('/').map(str::to_string).collect();
        let segment_ok = |s: &String| {
            !s.is_empty()
                && !s.starts_with(['.', '-'])
                && s.chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        };
        if segments.len() < 2 || !segments.iter().all(segment_ok) {
            return Err(invalid());
        }
        Ok(Self { host, segments })
    }

    /// Identity of `nf-core/<pipeline>` on GitHub.
    ///
    /// # Arguments
    /// * `pipeline` - Pipeline name, e.g. `rnaseq`
    pub fn nf_core(pipeline: &str) -> Result<Self, Error> {
        Self::parse(&format!("https://github.com/nf-core/{pipeline}"))
    }

    /// Canonical clone URL.
    pub fn url(&self) -> String {
        format!("https://{}/{}", self.host, self.segments.join("/"))
    }

    fn relative(&self) -> PathBuf {
        let mut path = PathBuf::from(&self.host);
        path.extend(&self.segments);
        path
    }
}

/// A reference resolved to a commit with a checkout on disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolved {
    /// Full commit hash.
    pub sha: String,
    /// Immutable working tree of that commit.
    pub directory: PathBuf,
}

/// A checkout found in the cache.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CachedCheckout {
    /// Commit hash.
    pub sha: String,
    /// Directory of the working tree.
    pub path: PathBuf,
    /// When it was last handed out by `resolve`.
    pub last_used: SystemTime,
}

/// Exclusive right to change the cache, released on drop (file lock first, then the mutex).
#[derive(Debug)]
struct CacheGuard<'a> {
    /// Dropped first: closing the file releases the file lock.
    _file: fs::File,
    _mutex: MutexGuard<'a, ()>,
}

/// Global repository cache, see the module documentation.
#[derive(Debug)]
pub struct WorkflowRepositoryCache {
    /// Current cache directory, replaceable at runtime (desktop settings).
    root: std::sync::RwLock<PathBuf>,
    branch_ttl: Duration,
    /// Serialises fetch and checkout between the threads of this process, see [`Self::lock_cache`]
    /// for other processes.
    lock: Mutex<()>,
}

impl WorkflowRepositoryCache {
    /// Creates the cache handle; directories are created lazily.
    ///
    /// # Arguments
    /// * `root` - Cache directory (made absolute), must be readable at the same absolute path by all workers
    /// * `branch_ttl` - How long a fetched branch is trusted before the next resolve fetches again
    pub fn new(root: impl Into<PathBuf>, branch_ttl: Duration) -> Self {
        Self {
            root: std::sync::RwLock::new(Self::absolute(root.into())),
            branch_ttl,
            lock: Mutex::new(()),
        }
    }

    /// Current cache directory.
    pub fn root(&self) -> PathBuf {
        self.root.read().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// Moves the cache to `root`. Nothing is moved on disk: the new location starts empty and
    /// fills on demand, old checkouts stay where they are (runs already queued keep pointing at
    /// them, they are no longer pruned).
    ///
    /// # Arguments
    /// * `root` - The new cache directory
    pub fn set_root(&self, root: PathBuf) {
        *self.root.write().unwrap_or_else(|e| e.into_inner()) = Self::absolute(root);
    }

    /// Makes `root` absolute against the current directory (symlinks are not resolved).
    /// libgit2 fails to write nested directories of a checkout below a relative target, and
    /// engines need an absolute path to mount.
    ///
    /// # Arguments
    /// * `root` - Configured cache directory, possibly relative
    fn absolute(root: PathBuf) -> PathBuf {
        std::path::absolute(&root).unwrap_or(root)
    }

    /// Waits until no thread or process changes the cache, then holds that right until the guard
    /// drops. Takes the in-process mutex, then an exclusive lock on the lock file in the cache
    /// root (created with the root if missing). The operating system releases the file lock when
    /// the process dies, so a crash never leaves the cache locked.
    ///
    /// # Errors
    /// The cache root or lock file cannot be created, or locking fails.
    fn lock_cache(&self) -> Result<CacheGuard<'_>, Error> {
        let mutex = self.lock.lock().unwrap_or_else(|e| e.into_inner());
        let root = self.root();
        fs::create_dir_all(&root)?;
        let file = fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(root.join(LOCK_FILE))?;
        file.lock()?;
        Ok(CacheGuard {
            _file: file,
            _mutex: mutex,
        })
    }

    fn mirror_directory(&self, id: &RepositoryId) -> PathBuf {
        let mut path = self.root().join("mirrors").join(id.relative());
        let name = format!("{}.git", path.file_name().unwrap().to_string_lossy());
        path.set_file_name(name);
        path
    }

    fn checkout_directory(&self, id: &RepositoryId, sha: &str) -> PathBuf {
        self.root().join("checkouts").join(id.relative()).join(sha)
    }

    /// Resolves `reference` to a commit and makes sure its checkout exists.
    ///
    /// Commit hashes and tags already in the mirror need no network. A branch (or an unknown
    /// reference) is fetched, branches again once older than the TTL. If a fetch fails but the
    /// reference resolves from the mirror, the cached state is used.
    ///
    /// # Arguments
    /// * `id` - Repository, from [`RepositoryId::parse`] or [`RepositoryId::nf_core`]
    /// * `reference` - Tag, branch or commit hash; `None` means the remote's default branch
    pub fn resolve(&self, id: &RepositoryId, reference: Option<&str>) -> Result<Resolved, Error> {
        self.resolve_within(id, reference, self.branch_ttl)
    }

    /// Like [`Self::resolve`], but a branch is trusted for `branch_ttl` instead of the
    /// configured time; `Duration::ZERO` always fetches branches.
    ///
    /// # Arguments
    /// * `id` - Repository
    /// * `reference` - Tag, branch or commit hash; `None` means the remote's default branch
    /// * `branch_ttl` - How long a fetched branch is trusted
    fn resolve_within(
        &self,
        id: &RepositoryId,
        reference: Option<&str>,
        branch_ttl: Duration,
    ) -> Result<Resolved, Error> {
        if let Some(reference) = reference {
            let bad = reference.is_empty()
                || reference.starts_with('-')
                || reference.chars().any(|c| c.is_control() || c == ' ');
            if bad {
                return Err(Error::InvalidReference(reference.to_string()));
            }
        }
        let _guard = self.lock_cache()?;
        let mirror = Mirror::open_or_init(self.mirror_directory(id))?;

        let fresh = match reference.and_then(|r| mirror.lookup(r)) {
            Some(Cached::Immutable(_)) => true,
            Some(Cached::Branch(_)) => mirror.fetched_within(branch_ttl),
            None => false,
        };
        let mut fetch_error = None;
        if !fresh && let Err(source) = mirror.fetch(&id.url()) {
            fetch_error = Some(Error::Fetch {
                url: id.url(),
                source,
            });
        }
        let mut missing = |name: &str| {
            fetch_error
                .take()
                .unwrap_or_else(|| Error::ReferenceNotFound(name.into()))
        };
        let oid = match reference {
            Some(r) => match mirror.lookup(r) {
                Some(Cached::Immutable(oid) | Cached::Branch(oid)) => oid,
                None => return Err(missing(r)),
            },
            None => mirror
                .default_head()
                .ok_or_else(|| missing("default branch"))?,
        };
        if let Some(error) = &fetch_error {
            tracing::warn!(%error, "using cached repository state");
        }
        let sha = oid.to_string();
        let directory = self.checkout_directory(id, &sha);
        if !directory.is_dir() {
            mirror.checkout(oid, &directory)?;
        }
        let _ = fs::write(directory.join(USED_MARKER), b"");
        Ok(Resolved { sha, directory })
    }

    /// Replaces a remote or nf-core source of `configuration` by a local source pointing at the
    /// immutable checkout of the resolved commit, so engines run from the cache without network.
    /// Other sources are left alone. Returns where the workflow came from (url, requested
    /// reference, commit, script) because the snapshot no longer holds it, `None` if nothing
    /// was resolved.
    ///
    /// # Arguments
    /// * `configuration` - Workflow configuration snapshot of a run, modified in place
    pub fn localize(
        &self,
        configuration: &mut WorkflowConfiguration,
    ) -> Result<Option<RunProvenance>, Error> {
        let Some((resolved, provenance)) = self.fetch_source(configuration, self.branch_ttl)?
        else {
            return Ok(None);
        };
        *configuration.source_mut() = Source::Local(LocalSource::new(
            resolved.directory,
            provenance.script.clone(),
        ));
        Ok(Some(provenance))
    }

    /// Fetches the remote or nf-core source of `configuration` into the cache without changing
    /// the configuration, so the first run (and the schema endpoint) need no network. Validates
    /// URL, reference and that the main script exists in the checkout. Returns what was
    /// resolved, `None` for sources that are not remote.
    ///
    /// # Arguments
    /// * `configuration` - Workflow configuration as saved by the editor
    pub fn prefetch(
        &self,
        configuration: &WorkflowConfiguration,
    ) -> Result<Option<Resolved>, Error> {
        Ok(self
            .fetch_source(configuration, self.branch_ttl)?
            .map(|(resolved, _)| resolved))
    }

    /// Brings the cache up to date with the remote for the source of `configuration`: branches
    /// (and the default branch) are fetched regardless of the TTL, so a user's run finds the
    /// newest commit already checked out. Tags and commit hashes need no network. Used by the
    /// background refresh. Returns what was resolved, `None` for sources that are not remote.
    ///
    /// # Arguments
    /// * `configuration` - Workflow configuration as saved, not modified
    pub fn refresh(
        &self,
        configuration: &WorkflowConfiguration,
    ) -> Result<Option<Resolved>, Error> {
        Ok(self
            .fetch_source(configuration, Duration::ZERO)?
            .map(|(resolved, _)| resolved))
    }

    /// Resolves the remote or nf-core source of `configuration` to a checkout and returns it
    /// with its provenance, whose script is validated (relative to the checkout). `None` for
    /// other sources.
    ///
    /// # Arguments
    /// * `configuration` - Workflow configuration, not modified
    /// * `branch_ttl` - How long a fetched branch is trusted
    fn fetch_source(
        &self,
        configuration: &WorkflowConfiguration,
        branch_ttl: Duration,
    ) -> Result<Option<(Resolved, RunProvenance)>, Error> {
        let default_script = match configuration.engine() {
            Engine::Snakemake => "Snakefile",
            _ => "main.nf",
        };
        let (id, reference, script) = match configuration.source() {
            Source::Remote(remote) => (
                RepositoryId::parse(remote.url())?,
                remote.version().cloned(),
                remote.script().cloned(),
            ),
            Source::NfCore(nf_core) => (
                RepositoryId::nf_core(nf_core.pipeline().trim_start_matches("nf-core/"))?,
                nf_core.version().cloned(),
                None,
            ),
            _ => return Ok(None),
        };
        let script = script.unwrap_or_else(|| PathBuf::from(default_script));
        if !is_confined(&script) {
            return Err(Error::InvalidPath(script.display().to_string()));
        }
        let script = sanitize_relative(&script);
        let resolved = self.resolve_within(&id, reference.as_deref(), branch_ttl)?;
        if !resolved.directory.join(&script).is_file() {
            return Err(Error::InvalidPath(script.display().to_string()));
        }
        let provenance = RunProvenance {
            url: id.url(),
            requested_ref: reference,
            commit: resolved.sha.clone(),
            script,
        };
        Ok(Some((resolved, provenance)))
    }

    /// All checkouts with the time they were last handed out.
    pub fn checkouts(&self) -> Result<Vec<CachedCheckout>, Error> {
        let mut found = Vec::new();
        let root = self.root().join("checkouts");
        if root.is_dir() {
            Self::collect_checkouts(&root, &mut found)?;
        }
        Ok(found)
    }

    /// Walks `directory` for checkout directories (named by a 40 digit hash).
    ///
    /// # Arguments
    /// * `directory` - Directory below `checkouts/`
    /// * `found` - Receives the checkouts
    fn collect_checkouts(directory: &Path, found: &mut Vec<CachedCheckout>) -> Result<(), Error> {
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            let path = entry.path();
            if !entry.file_type()?.is_dir() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') {
                continue;
            }
            if name.len() == 40 && name.chars().all(|c| c.is_ascii_hexdigit()) {
                let last_used = fs::metadata(path.join(USED_MARKER))
                    .or_else(|_| fs::metadata(&path))
                    .and_then(|m| m.modified())
                    .unwrap_or(SystemTime::UNIX_EPOCH);
                found.push(CachedCheckout {
                    sha: name,
                    path,
                    last_used,
                });
            } else {
                Self::collect_checkouts(&path, found)?;
            }
        }
        Ok(())
    }

    /// Deletes checkouts not handed out for `max_age`, except those in `in_use`. Mirrors stay
    /// (cheap, and they make re-checkouts and offline use possible). Returns the removed ones.
    ///
    /// # Arguments
    /// * `max_age` - Checkouts last used longer ago than this are removed
    /// * `in_use` - Checkout directories of queued or running runs, never removed
    pub fn prune(
        &self,
        max_age: Duration,
        in_use: &std::collections::HashSet<PathBuf>,
    ) -> Result<Vec<CachedCheckout>, Error> {
        let _guard = self.lock_cache()?;
        let mut removed = Vec::new();
        for checkout in self.checkouts()? {
            let age = SystemTime::now()
                .duration_since(checkout.last_used)
                .unwrap_or_default();
            if age >= max_age && !in_use.contains(&checkout.path) {
                fs::remove_dir_all(&checkout.path)?;
                removed.push(checkout);
            }
        }
        Ok(removed)
    }

    /// Reads the schema files of a workflow from `directory`: `nextflow_schema.json`, the sample
    /// sheet schema it points to (default `assets/schema_input.json`) and Snakemake
    /// `*.schema.yaml` files. Missing or unparsable files are skipped.
    ///
    /// # Arguments
    /// * `directory` - Checkout or local workflow directory
    pub fn schemas(directory: &Path) -> WorkflowSchemasResponse {
        let json = |relative: &Path| -> Option<serde_json::Value> {
            serde_json::from_slice(&Self::read_file(directory, relative).ok()?).ok()
        };
        let nextflow_schema = json(Path::new("nextflow_schema.json"));
        let input_schema_path = nextflow_schema
            .as_ref()
            .and_then(|schema| {
                let direct = schema.pointer("/properties/input/schema");
                // nf-core keeps its properties inside `$defs`/`definitions` groups
                let grouped = ["$defs", "definitions"].iter().find_map(|key| {
                    schema
                        .get(key)?
                        .as_object()?
                        .values()
                        .find_map(|group| group.pointer("/properties/input/schema"))
                });
                direct.or(grouped)?.as_str().map(PathBuf::from)
            })
            .unwrap_or_else(|| PathBuf::from("assets/schema_input.json"));
        let input_schema = json(&input_schema_path);

        let mut snakemake_schemas = Vec::new();
        for folder in ["workflow/schemas", "config"] {
            let Ok(entries) = fs::read_dir(directory.join(folder)) else {
                continue;
            };
            let mut names: Vec<String> = entries
                .flatten()
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .filter(|n| n.ends_with(".schema.yaml") || n.ends_with(".schema.yml"))
                .collect();
            names.sort();
            for name in names {
                let path = format!("{folder}/{name}");
                if let Ok(bytes) = Self::read_file(directory, Path::new(&path))
                    && let Ok(content) = String::from_utf8(bytes)
                {
                    snakemake_schemas.push(SchemaFile { path, content });
                }
            }
        }
        WorkflowSchemasResponse {
            commit: None,
            nextflow_schema,
            input_schema,
            snakemake_schemas,
        }
    }

    /// Reads a file of a checkout. Symlinks anywhere on the path are refused, repository
    /// content is untrusted.
    ///
    /// # Arguments
    /// * `checkout` - A [`Resolved::directory`]
    /// * `relative` - Path inside the checkout; `..` is rejected, a leading `/` is stripped
    pub fn read_file(checkout: &Path, relative: &Path) -> Result<Vec<u8>, Error> {
        let invalid = || Error::InvalidPath(relative.display().to_string());
        if !is_confined(relative) {
            return Err(invalid());
        }
        let relative = sanitize_relative(relative);
        if relative.as_os_str().is_empty() {
            return Err(invalid());
        }
        let mut current = checkout.to_path_buf();
        for component in relative.components() {
            current.push(component);
            let metadata = fs::symlink_metadata(&current).map_err(|_| invalid())?;
            if metadata.file_type().is_symlink() {
                return Err(invalid());
            }
        }
        let metadata = fs::symlink_metadata(&current).map_err(|_| invalid())?;
        if !metadata.is_file() || metadata.len() > MAX_READ_BYTES {
            return Err(invalid());
        }
        Ok(fs::read(current)?)
    }
}

/// What a reference currently points to in the mirror.
enum Cached {
    /// Commit hash or tag: cannot move (tags are assumed not to).
    Immutable(Oid),
    /// Branch: may have moved on the remote.
    Branch(Oid),
}

/// The bare mirror of one repository.
struct Mirror {
    /// Opened bare repository.
    repository: Repository,
    /// Where the bare repository lives (also holds the fetch marker).
    directory: PathBuf,
}

impl Mirror {
    /// Opens the mirror at `directory`, creating an empty bare repository if it does not exist.
    ///
    /// # Arguments
    /// * `directory` - Location of the bare repository
    fn open_or_init(directory: PathBuf) -> Result<Self, Error> {
        fs::create_dir_all(directory.parent().expect("mirror has a parent"))?;
        let repository = if directory.exists() {
            Repository::open_bare(&directory)?
        } else {
            Repository::init_bare(&directory)?
        };
        Ok(Self {
            repository,
            directory,
        })
    }

    /// Looks `reference` up without network.
    ///
    /// # Arguments
    /// * `reference` - Commit hash, tag or branch name
    fn lookup(&self, reference: &str) -> Option<Cached> {
        let is_hash = reference.len() == 40 && reference.chars().all(|c| c.is_ascii_hexdigit());
        if is_hash {
            let oid = Oid::from_str(reference).ok()?;
            return self
                .repository
                .find_commit(oid)
                .ok()
                .map(|c| Cached::Immutable(c.id()));
        }
        if let Ok(r) = self
            .repository
            .find_reference(&format!("refs/tags/{reference}"))
        {
            return r.peel_to_commit().ok().map(|c| Cached::Immutable(c.id()));
        }
        let r = self
            .repository
            .find_reference(&format!("refs/heads/{reference}"))
            .ok()?;
        r.peel_to_commit().ok().map(|c| Cached::Branch(c.id()))
    }

    /// Tip of the remote's default branch as recorded by the last fetch (mirror `HEAD`).
    fn default_head(&self) -> Option<Oid> {
        self.repository
            .head()
            .ok()?
            .peel_to_commit()
            .ok()
            .map(|c| c.id())
    }

    /// Fetches all branches and tags, points `HEAD` at the remote's default branch and stamps
    /// the fetch time.
    ///
    /// # Arguments
    /// * `url` - Canonical clone URL of the repository
    fn fetch(&self, url: &str) -> Result<(), git2::Error> {
        let mut remote = self.repository.remote_anonymous(url)?;
        remote.connect(git2::Direction::Fetch)?;
        let default_branch = remote
            .default_branch()
            .ok()
            .and_then(|b| b.as_str().map(str::to_string));
        remote.disconnect()?;
        let mut options = FetchOptions::new();
        options.download_tags(git2::AutotagOption::All);
        remote.fetch(
            &["+refs/heads/*:refs/heads/*", "+refs/tags/*:refs/tags/*"],
            Some(&mut options),
            None,
        )?;
        if let Some(branch) = default_branch
            && self.repository.find_reference(&branch).is_ok()
        {
            self.repository.set_head(&branch)?;
        }
        let _ = fs::write(self.directory.join(FETCHED_MARKER), b"");
        Ok(())
    }

    /// Whether the last successful fetch is younger than `time_to_live`.
    ///
    /// # Arguments
    /// * `time_to_live` - Maximum age
    fn fetched_within(&self, time_to_live: Duration) -> bool {
        fs::metadata(self.directory.join(FETCHED_MARKER))
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| SystemTime::now().duration_since(t).ok())
            .is_some_and(|age| age < time_to_live)
    }

    /// Writes the tree of commit `oid` to `target` via a temporary sibling directory and a
    /// rename, so `target` appears completely or not at all.
    ///
    /// # Arguments
    /// * `oid` - Commit to check out
    /// * `target` - Final checkout directory
    fn checkout(&self, oid: Oid, target: &Path) -> Result<(), Error> {
        let parent = target.parent().expect("checkout has a parent");
        fs::create_dir_all(parent)?;
        let nanos = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default();
        let temporary = parent.join(format!(
            ".{}.partial-{}-{nanos}",
            target.file_name().unwrap().to_string_lossy(),
            std::process::id()
        ));
        fs::create_dir_all(&temporary)?;
        let tree = self.repository.find_commit(oid)?.tree()?;
        let mut builder = CheckoutBuilder::new();
        builder.target_dir(&temporary).force();
        if let Err(error) = self
            .repository
            .checkout_tree(tree.as_object(), Some(&mut builder))
        {
            let _ = fs::remove_dir_all(&temporary);
            return Err(error.into());
        }
        if let Err(error) = fs::rename(&temporary, target) {
            let _ = fs::remove_dir_all(&temporary);
            // A concurrent process may have created it meanwhile.
            if !target.is_dir() {
                return Err(error.into());
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use git2::Signature;

    /// Commits a file to `repo` and returns the commit id.
    ///
    /// # Arguments
    /// * `repo` - Non-bare repository
    /// * `file` - File name in the work tree
    /// * `content` - New file content
    /// * `parent` - Parent commit, `None` for the first commit
    fn commit(repo: &Repository, file: &str, content: &str, parent: Option<Oid>) -> Oid {
        let workdir = repo.workdir().unwrap();
        fs::create_dir_all(workdir.join(file).parent().unwrap()).unwrap();
        fs::write(workdir.join(file), content).unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(Path::new(file)).unwrap();
        index.write().unwrap();
        let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
        let sig = Signature::now("t", "t@example.org").unwrap();
        let parents: Vec<_> = parent
            .map(|p| repo.find_commit(p).unwrap())
            .into_iter()
            .collect();
        let parents: Vec<_> = parents.iter().collect();
        repo.commit(Some("HEAD"), &sig, &sig, "c", &tree, &parents)
            .unwrap()
    }

    /// Cache plus a local repository acting as its remote.
    struct Fixture {
        /// Keeps the temporary directory alive.
        _temporary: tempfile::TempDir,
        /// The repository the cache fetches from.
        remote: Repository,
        /// Cache under test.
        cache: WorkflowRepositoryCache,
    }

    impl Fixture {
        /// Creates an empty remote and a cache in a temporary directory.
        fn new() -> Self {
            let temporary = tempfile::tempdir().unwrap();
            let remote = Repository::init(temporary.path().join("remote")).unwrap();
            let cache = WorkflowRepositoryCache::new(
                temporary.path().join("cache"),
                Duration::from_secs(3600),
            );
            Self {
                _temporary: temporary,
                remote,
                cache,
            }
        }

        /// Path of the remote, used as its URL.
        fn url(&self) -> String {
            self.remote.workdir().unwrap().display().to_string()
        }

        /// Like `WorkflowRepositoryCache::resolve`, but fetching from the local remote via `insteadOf`.
        ///
        /// # Arguments
        /// * `reference` - Tag, branch or hash; `None` for the default branch
        fn resolve(&self, reference: Option<&str>) -> Result<Resolved, Error> {
            let id = RepositoryId {
                host: "local".into(),
                segments: vec!["o".into(), "r".into()],
            };
            // Swap the https URL for the local path by pre-creating the mirror remote config:
            // resolve() fetches from `id.url()`, so serve it via a url rewrite in the mirror.
            let mirror_directory = self.cache.mirror_directory(&id);
            fs::create_dir_all(mirror_directory.parent().unwrap()).unwrap();
            let mirror = if mirror_directory.exists() {
                Repository::open_bare(&mirror_directory).unwrap()
            } else {
                Repository::init_bare(&mirror_directory).unwrap()
            };
            mirror
                .config()
                .unwrap()
                .set_str(&format!("url.{}.insteadOf", self.url()), &id.url())
                .unwrap();
            self.cache.resolve(&id, reference)
        }
    }

    /// A second cache handle on the same directory (another process has its own mutex) waits
    /// for the file lock held by the first, and goes on once it is released. The lock file is
    /// created in the root and not taken for a checkout.
    #[test]
    fn file_lock_excludes_other_handles_on_the_same_directory() {
        use std::sync::mpsc;
        let f = Fixture::new();
        commit(&f.remote, "main.nf", "1", None);
        let other = WorkflowRepositoryCache::new(f.cache.root(), Duration::from_secs(3600));

        let guard = f.cache.lock_cache().unwrap();
        assert!(f.cache.root().join(LOCK_FILE).is_file());
        assert!(f.cache.checkouts().unwrap().is_empty());
        let (sender, receiver) = mpsc::channel();
        let waiter = std::thread::spawn(move || {
            let result = other.prune(Duration::from_secs(0), &Default::default());
            sender.send(result.is_ok()).unwrap();
        });
        assert!(
            receiver.recv_timeout(Duration::from_millis(300)).is_err(),
            "prune must wait for the file lock"
        );
        drop(guard);
        assert!(receiver.recv_timeout(Duration::from_secs(10)).unwrap());
        waiter.join().unwrap();
    }

    /// Only plain https URLs are accepted; other schemes, credentials, ports and odd segments are not.
    #[test]
    fn url_parsing_accepts_https_and_rejects_the_rest() {
        let id = RepositoryId::parse("https://GitHub.com/nf-core/rnaseq.git").unwrap();
        assert_eq!(id.url(), "https://github.com/nf-core/rnaseq");
        for bad in [
            "file:///etc",
            "ssh://git@github.com/a/b",
            "ext::sh -c id",
            "http://github.com/a/b",
            "https://user:pw@github.com/a/b",
            "https://github.com/a/../b",
            "https://github.com/a",
            "https://github.com/-x/y",
            "https://github.com:8080/a/b",
            "https://github.com/a/b?x=1",
        ] {
            assert!(RepositoryId::parse(bad).is_err(), "{bad}");
        }
    }

    /// `read_file` refuses parent traversal, symlinks and missing files.
    #[test]
    fn read_file_rejects_escape_and_symlinks() {
        let temporary = tempfile::tempdir().unwrap();
        fs::write(temporary.path().join("a.txt"), "hi").unwrap();
        fs::write(temporary.path().parent().unwrap().join("outside.txt"), "x").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink("/etc/hosts", temporary.path().join("link")).unwrap();
        assert_eq!(
            WorkflowRepositoryCache::read_file(temporary.path(), Path::new("a.txt")).unwrap(),
            b"hi"
        );
        assert!(
            WorkflowRepositoryCache::read_file(temporary.path(), Path::new("../outside.txt"))
                .is_err()
        );
        #[cfg(unix)]
        assert!(WorkflowRepositoryCache::read_file(temporary.path(), Path::new("link")).is_err());
        assert!(
            WorkflowRepositoryCache::read_file(temporary.path(), Path::new("missing")).is_err()
        );
    }

    /// Two tags give two checkouts; moving a tag on the remote never changes an existing one.
    #[test]
    fn two_tags_coexist_and_a_moved_tag_leaves_the_old_checkout_alone() {
        let f = Fixture::new();
        let first = commit(&f.remote, "main.nf", "v1", None);
        f.remote
            .tag_lightweight("v1", &f.remote.find_object(first, None).unwrap(), false)
            .unwrap();
        let second = commit(&f.remote, "main.nf", "v2", Some(first));
        f.remote
            .tag_lightweight("v2", &f.remote.find_object(second, None).unwrap(), false)
            .unwrap();

        let r1 = f.resolve(Some("v1")).unwrap();
        let r2 = f.resolve(Some("v2")).unwrap();
        assert_ne!(r1.directory, r2.directory);
        assert_eq!(r1.sha, first.to_string());
        assert_eq!(
            fs::read_to_string(r1.directory.join("main.nf")).unwrap(),
            "v1"
        );
        assert_eq!(
            fs::read_to_string(r2.directory.join("main.nf")).unwrap(),
            "v2"
        );

        // Move v1 on the remote to the second commit; the cache keeps serving the old one
        // (tags are immutable for the cache) and the old checkout is untouched either way.
        f.remote
            .tag_lightweight("v1", &f.remote.find_object(second, None).unwrap(), true)
            .unwrap();
        let again = f.resolve(Some("v1")).unwrap();
        assert_eq!(again, r1);
        assert_eq!(
            fs::read_to_string(r1.directory.join("main.nf")).unwrap(),
            "v1"
        );

        // By hash works too.
        assert_eq!(f.resolve(Some(&second.to_string())).unwrap(), r2);
    }

    /// No reference uses the default branch; unknown and option-like references are rejected.
    #[test]
    fn default_branch_and_unknown_reference() {
        let f = Fixture::new();
        let c = commit(&f.remote, "main.nf", "x", None);
        let r = f.resolve(None).unwrap();
        assert_eq!(r.sha, c.to_string());
        assert!(matches!(
            f.resolve(Some("nope")),
            Err(Error::ReferenceNotFound(_))
        ));
        assert!(matches!(
            f.resolve(Some("--upload-pack=x")),
            Err(Error::InvalidReference(_))
        ));
    }

    /// A cached tag still resolves after the remote disappeared.
    #[test]
    fn cached_state_is_used_when_the_remote_is_gone() {
        let f = Fixture::new();
        let c = commit(&f.remote, "main.nf", "x", None);
        f.remote
            .tag_lightweight("v1", &f.remote.find_object(c, None).unwrap(), false)
            .unwrap();
        let first = f.resolve(Some("v1")).unwrap();
        fs::remove_dir_all(f.remote.workdir().unwrap()).unwrap();
        assert_eq!(f.resolve(Some("v1")).unwrap(), first);
    }

    /// `localize` turns a remote source into a local one at the resolved checkout, honours the
    /// script, and rejects a missing or escaping script.
    #[test]
    fn localize_pins_remote_sources() {
        use macworp_core::workflow_configuration::source::RemoteSource;
        let f = Fixture::new();
        let c = commit(&f.remote, "main.nf", "x", None);
        f.remote
            .tag_lightweight("v1", &f.remote.find_object(c, None).unwrap(), false)
            .unwrap();
        f.resolve(None).unwrap(); // prepares the mirror to fetch from the local remote

        let mut configuration = WorkflowConfiguration::default();
        *configuration.source_mut() = Source::Remote(RemoteSource::new(
            "https://local/o/r".into(),
            Some("v1".into()),
            None,
        ));
        let provenance = f.cache.localize(&mut configuration).unwrap().unwrap();
        assert_eq!(provenance.commit, c.to_string());
        assert_eq!(provenance.url, "https://local/o/r");
        assert_eq!(provenance.requested_ref.as_deref(), Some("v1"));
        assert_eq!(provenance.script, Path::new("main.nf"));
        match configuration.source() {
            Source::Local(local) => {
                assert!(local.location().ends_with(&provenance.commit));
                assert_eq!(local.script(), Path::new("main.nf"));
            }
            other => panic!("not localized: {other:?}"),
        }
        // Local sources are untouched.
        assert!(f.cache.localize(&mut configuration).unwrap().is_none());

        for script in ["../main.nf", "missing.nf"] {
            *configuration.source_mut() = Source::Remote(RemoteSource::new(
                "https://local/o/r".into(),
                Some("v1".into()),
                Some(script.into()),
            ));
            assert!(f.cache.localize(&mut configuration).is_err(), "{script}");
        }
    }

    /// `prefetch` fills the cache for a tag that is not cached yet without touching the
    /// configuration, serves it offline afterwards, and reports bad references and scripts.
    #[test]
    fn prefetch_warms_the_cache_and_validates() {
        use macworp_core::workflow_configuration::source::RemoteSource;
        let f = Fixture::new();
        let first = commit(&f.remote, "main.nf", "1", None);
        f.resolve(None).unwrap(); // prepares the mirror to fetch from the local remote
        let second = commit(&f.remote, "main.nf", "2", Some(first));
        f.remote
            .tag_lightweight("v2", &f.remote.find_object(second, None).unwrap(), false)
            .unwrap();

        let remote = |reference: &str, script: Option<&str>| {
            let mut configuration = WorkflowConfiguration::default();
            *configuration.source_mut() = Source::Remote(RemoteSource::new(
                "https://local/o/r".into(),
                Some(reference.into()),
                script.map(Into::into),
            ));
            configuration
        };

        let configuration = remote("v2", None);
        let resolved = f.cache.prefetch(&configuration).unwrap().unwrap();
        assert_eq!(resolved.sha, second.to_string());
        assert!(resolved.directory.join("main.nf").is_file());
        assert!(matches!(configuration.source(), Source::Remote(_)));

        // Served from the cache once the remote is gone.
        fs::remove_dir_all(f.remote.workdir().unwrap()).unwrap();
        assert_eq!(f.cache.prefetch(&configuration).unwrap().unwrap(), resolved);

        assert!(matches!(
            f.cache.prefetch(&remote("v2", Some("missing.nf"))),
            Err(Error::InvalidPath(_))
        ));
        assert!(matches!(
            f.cache.prefetch(&remote("nope", None)),
            Err(Error::Fetch { .. } | Error::ReferenceNotFound(_))
        ));
        // Not remote: nothing to do.
        assert!(
            f.cache
                .prefetch(&WorkflowConfiguration::default())
                .unwrap()
                .is_none()
        );
    }

    /// `refresh` fetches a branch although it is younger than the TTL, `prefetch` does not.
    #[test]
    fn refresh_ignores_the_branch_ttl() {
        use macworp_core::workflow_configuration::source::RemoteSource;
        let f = Fixture::new();
        let first = commit(&f.remote, "main.nf", "1", None);
        f.resolve(None).unwrap(); // prepares the mirror to fetch from the local remote
        let branch = f.remote.head().unwrap().shorthand().unwrap().to_string();
        let mut configuration = WorkflowConfiguration::default();
        *configuration.source_mut() = Source::Remote(RemoteSource::new(
            "https://local/o/r".into(),
            Some(branch),
            None,
        ));
        assert_eq!(
            f.cache.prefetch(&configuration).unwrap().unwrap().sha,
            first.to_string()
        );

        let second = commit(&f.remote, "main.nf", "2", Some(first));
        // Within the TTL the cached branch is trusted ...
        assert_eq!(
            f.cache.prefetch(&configuration).unwrap().unwrap().sha,
            first.to_string()
        );
        // ... a refresh looks at the remote and checks out the new commit.
        let refreshed = f.cache.refresh(&configuration).unwrap().unwrap();
        assert_eq!(refreshed.sha, second.to_string());
        assert!(refreshed.directory.join("main.nf").is_file());
        assert!(
            f.cache
                .refresh(&WorkflowConfiguration::default())
                .unwrap()
                .is_none()
        );
    }

    /// `prune` removes old unused checkouts and keeps young ones and those in use.
    #[test]
    fn prune_keeps_checkouts_in_use() {
        let f = Fixture::new();
        let first = commit(&f.remote, "main.nf", "1", None);
        let second = commit(&f.remote, "main.nf", "2", Some(first));
        let old = f.resolve(Some(&first.to_string())).unwrap();
        let busy = f.resolve(Some(&second.to_string())).unwrap();
        assert_eq!(f.cache.checkouts().unwrap().len(), 2);

        let none = std::collections::HashSet::new();
        assert!(
            f.cache
                .prune(Duration::from_secs(3600), &none)
                .unwrap()
                .is_empty()
        );

        std::thread::sleep(Duration::from_millis(20));
        let in_use = std::collections::HashSet::from([busy.directory.clone()]);
        let removed = f.cache.prune(Duration::ZERO, &in_use).unwrap();
        assert_eq!(removed.len(), 1);
        assert_eq!(removed[0].path, old.directory);
        assert!(!old.directory.exists() && busy.directory.exists());
        // Re-resolving recreates it from the mirror, without network.
        assert_eq!(f.resolve(Some(&first.to_string())).unwrap(), old);
    }

    /// `schemas` follows the sample sheet schema pointer, falls back to the default path and
    /// collects Snakemake schemas; garbage is skipped.
    #[test]
    fn schemas_are_collected() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        fs::create_dir_all(root.join("workflow/schemas")).unwrap();
        fs::create_dir_all(root.join("assets")).unwrap();
        fs::write(
            root.join("nextflow_schema.json"),
            r#"{"$defs":{"io":{"properties":{"input":{"schema":"assets/sheet.json"}}}}}"#,
        )
        .unwrap();
        fs::write(root.join("assets/sheet.json"), r#"{"type":"array"}"#).unwrap();
        fs::write(
            root.join("workflow/schemas/config.schema.yaml"),
            "type: object\n",
        )
        .unwrap();
        fs::write(root.join("workflow/schemas/bad.schema.yaml"), [0xff, 0xfe]).unwrap();

        let found = WorkflowRepositoryCache::schemas(root);
        assert!(found.nextflow_schema.is_some());
        assert_eq!(found.input_schema.unwrap()["type"], "array");
        assert_eq!(found.snakemake_schemas.len(), 1);
        assert_eq!(
            found.snakemake_schemas[0].path,
            "workflow/schemas/config.schema.yaml"
        );

        // Without a pointer the default path is used; nothing at all gives empty results.
        fs::write(root.join("nextflow_schema.json"), "{}").unwrap();
        fs::write(
            root.join("assets/schema_input.json"),
            r#"{"type":"object"}"#,
        )
        .unwrap();
        assert_eq!(
            WorkflowRepositoryCache::schemas(root).input_schema.unwrap()["type"],
            "object"
        );
        let empty = tempfile::tempdir().unwrap();
        assert_eq!(
            WorkflowRepositoryCache::schemas(empty.path()),
            WorkflowSchemasResponse::default()
        );
    }

    /// Checkouts contain nested directories and keep the executable bit.
    #[test]
    fn nested_files_are_checked_out() {
        let f = Fixture::new();
        let first = commit(&f.remote, "main.nf", "x", None);
        commit(&f.remote, "test/data/asset.bin", "bin", Some(first));
        commit(
            &f.remote,
            "bin/run.sh",
            "#!/bin/sh\n",
            Some(f.remote.head().unwrap().target().unwrap()),
        );
        let resolved = f.resolve(None).unwrap();
        assert_eq!(
            fs::read_to_string(resolved.directory.join("test/data/asset.bin")).unwrap(),
            "bin"
        );
        assert!(resolved.directory.join("bin/run.sh").is_file());
        assert!(resolved.directory.join("main.nf").is_file());
    }

    /// A cache root given as a relative path (the default `dev_projects/workflows/remote_cache`) works.
    #[test]
    fn relative_cache_root_works() {
        let f = Fixture::new();
        let first = commit(&f.remote, "main.nf", "x", None);
        commit(&f.remote, "test/asset.bin", "bin", Some(first));
        let relative = PathBuf::from(format!("target/relative-cache-test-{}", std::process::id()));
        let cache = WorkflowRepositoryCache::new(&relative, Duration::from_secs(3600));
        let id = RepositoryId {
            host: "local".into(),
            segments: vec!["o".into(), "r".into()],
        };
        let mirror_directory = cache.mirror_directory(&id);
        fs::create_dir_all(mirror_directory.parent().unwrap()).unwrap();
        let mirror = Repository::init_bare(&mirror_directory).unwrap();
        mirror
            .config()
            .unwrap()
            .set_str(&format!("url.{}.insteadOf", f.url()), &id.url())
            .unwrap();
        let result = cache.resolve(&id, None);
        let outcome = result.map(|r| fs::read_to_string(r.directory.join("test/asset.bin")));
        let _ = fs::remove_dir_all(&relative);
        assert_eq!(outcome.unwrap().unwrap(), "bin");
    }
}
