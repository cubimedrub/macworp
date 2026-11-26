//! Builds the images a local workflow maps to its build files (ADR 0007) with the worker's
//! container runtime, before the engine starts. The engine then finds them under the literal
//! name the workflow uses.
//!
//! Every image gets the maintainer's name and a second tag `<repository>:<content hash>`. An
//! unchanged build context is not built again: the hash tag exists already and only the name is
//! pointed at it. Builds run the workflow author's `Dockerfile`: no privileged flags, no host
//! mounts, and the build file and context must lie inside the workflow directory without
//! symlinks on the way.

use std::{
    io::Read,
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};

use macworp_core::{
    entities::{FailureReason, LogStream},
    paths::sanitize_relative,
    workflow_configuration::{images::ImageBuild, source::Source},
};
use sha2::{Digest, Sha256};
use tokio::{
    process::Command,
    sync::{mpsc, watch},
};

use crate::{
    console_window::HideConsoleWindow,
    container_runtime::{ContainerRuntime, RuntimeKind},
    executors::{Job, LaunchSpec, LogLine, Outcome, run_process},
};

/// Longest time all builds of one run may take.
pub const BUILD_TIMEOUT: Duration = Duration::from_secs(60 * 60);

/// Hex characters of the content hash used in the tag.
const HASH_LENGTH: usize = 16;

/// Most files hashed in one build context; a bigger one is rejected instead of read for ages.
const CONTEXT_FILE_LIMIT: usize = 20_000;

/// Why a build could not even be started.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The build file is missing, not a regular file or reached through a symlink
    #[error("Build file `{0}` is not a regular file inside the workflow directory")]
    BuildFile(String),
    /// The build context cannot be read or is too big
    #[error("Cannot read the build context of `{0}`: {1}")]
    Context(String, String),
    /// The job's workflow is not a local one
    #[error("Image builds need a local workflow")]
    NotLocal,
}

/// Builds images with one container runtime.
#[derive(Debug, Clone)]
pub struct ImageBuilder {
    /// Runtime binary the `build`, `tag` and `image inspect` commands run with
    program: PathBuf,
    /// Which runtime it is, to choose the builder matching an executor's runtime
    kind: RuntimeKind,
    /// Image names in use by active runs, shared by all clones (the slots of one worker use one
    /// runtime and therefore one image store)
    locks: std::sync::Arc<ImageLocks>,
}

/// A build prepared for [`ImageBuilder::build_one`]: checked paths and the content hash.
#[derive(Debug)]
struct PreparedImage {
    /// Name the workflow uses
    name: String,
    /// Build file as the maintainer entered it, for the log
    dockerfile: String,
    /// Build file inside the workflow directory
    file: PathBuf,
    /// Build context directory
    context: PathBuf,
    /// Content hash of the context
    hash: String,
    /// `<repository>:<hash>`, the second tag of the image
    hash_tag: String,
}

/// Which image names are in use by active runs of a worker, and with which content.
///
/// The engine finds an image by the literal name, so two runs must not give one name different
/// content: the second waits until the first ended. Runs with identical content share the name.
#[derive(Debug, Default)]
pub struct ImageLocks {
    /// Name to the content hash in use and the number of runs using it
    in_use: std::sync::Mutex<std::collections::HashMap<String, (String, usize)>>,
    /// Woken whenever a name is released
    released: tokio::sync::Notify,
}

impl ImageLocks {
    /// Reserves `images` for one run, waiting while any name is used with other content.
    /// Nothing is held while waiting, so a waiting run never blocks another. Names should be
    /// given in a fixed order by every caller anyway.
    ///
    /// # Arguments
    /// * `images` - `(name, content hash)` pairs of the run
    /// * `logs` - run log, told once when the run has to wait
    /// * `cancel` - turns `true` when a user stops the run
    ///
    /// # Returns
    /// The reservation, or [`Outcome::Cancelled`] when the run was stopped while waiting.
    pub async fn acquire(
        self: &std::sync::Arc<Self>,
        images: Vec<(String, String)>,
        logs: &mpsc::Sender<LogLine>,
        mut cancel: watch::Receiver<bool>,
    ) -> Result<ImageHold, Outcome> {
        let mut told = false;
        loop {
            let notified = self.released.notified();
            tokio::pin!(notified);
            // Registers before the check: a release in between is not lost
            notified.as_mut().enable();
            let blocked = {
                let mut in_use = self.in_use.lock().unwrap();
                let blocked = images.iter().find(|(name, hash)| {
                    in_use
                        .get(name)
                        .is_some_and(|(used_hash, _)| used_hash != hash)
                });
                match blocked {
                    Some((name, _)) => Some(name.clone()),
                    None => {
                        for (name, hash) in &images {
                            in_use.entry(name.clone()).or_insert((hash.clone(), 0)).1 += 1;
                        }
                        None
                    }
                }
            };
            let Some(name) = blocked else {
                return Ok(ImageHold {
                    locks: self.clone(),
                    names: images.into_iter().map(|(name, _)| name).collect(),
                });
            };
            if !told {
                told = true;
                let _ = logs
                    .send(LogLine {
                        stream: LogStream::Engine,
                        line: format!(
                            "Waiting: image name {name} is used by another run with different content"
                        ),
                    })
                    .await;
            }
            tokio::select! {
                _ = &mut notified => {}
                stopped = async { cancel.wait_for(|cancelled| *cancelled).await.is_ok() } => {
                    if stopped {
                        return Err(Outcome::Cancelled);
                    }
                    // Nobody can cancel anymore: keep waiting for the release
                    notified.await;
                }
            }
        }
    }
}

/// The image names a run has reserved, released when dropped (when the run ended).
#[derive(Debug)]
pub struct ImageHold {
    /// Where the names are registered
    locks: std::sync::Arc<ImageLocks>,
    /// The reserved names
    names: Vec<String>,
}

impl Drop for ImageHold {
    fn drop(&mut self) {
        let mut in_use = self.locks.in_use.lock().unwrap();
        for name in &self.names {
            if let Some(entry) = in_use.get_mut(name) {
                entry.1 -= 1;
                if entry.1 == 0 {
                    in_use.remove(name);
                }
            }
        }
        drop(in_use);
        self.locks.released.notify_waiters();
    }
}

impl ImageBuilder {
    /// A builder using `runtime`'s binary.
    ///
    /// # Arguments
    /// * `runtime` - the runtime the engines of this worker start their containers with
    pub fn for_runtime(runtime: &dyn ContainerRuntime) -> Self {
        Self::new(runtime.binary().to_path_buf(), runtime.kind())
    }

    /// A builder running `program`.
    ///
    /// # Arguments
    /// * `program` - runtime binary
    /// * `kind` - the runtime it is
    pub fn new(program: PathBuf, kind: RuntimeKind) -> Self {
        Self {
            program,
            kind,
            locks: Default::default(),
        }
    }

    /// The runtime this builder uses.
    pub fn kind(&self) -> RuntimeKind {
        self.kind
    }

    /// Makes every image of the job's configuration available under its name, logging to the
    /// run as `Engine` lines plus the build output. First the build contexts are hashed, then
    /// the names are reserved (see [`ImageLocks`]): a run whose name is in use by an active run
    /// with different content waits here, which needs no time limit. Only the builds count
    /// against [`BUILD_TIMEOUT`]. Stops at the first failure or when `cancel` turns `true`.
    ///
    /// # Arguments
    /// * `job` - claimed run with a local source (its location resolved on this machine)
    /// * `logs` - run log
    /// * `cancel` - turns `true` when a user stops the run
    ///
    /// # Returns
    /// The reservation of the names, to keep until the run ended; otherwise how the run ends
    /// instead: the failure ([`FailureReason::StartFailed`]) or [`Outcome::Cancelled`].
    pub async fn build_all(
        &self,
        job: &Job,
        logs: &mpsc::Sender<LogLine>,
        cancel: watch::Receiver<bool>,
    ) -> Result<ImageHold, Outcome> {
        let Source::Local(local) = job.configuration.source() else {
            return Err(failed(Error::NotLocal.to_string()));
        };
        let mut prepared = Vec::new();
        for image in job.configuration.images() {
            prepared.push(Self::prepare(local.location(), image).await?);
        }
        // Always in name order, so two runs needing the same names cannot wait for each other
        prepared.sort_by(|a, b| a.name.cmp(&b.name));
        let hold = self
            .locks
            .acquire(
                prepared
                    .iter()
                    .map(|image| (image.name.clone(), image.hash.clone()))
                    .collect(),
                logs,
                cancel.clone(),
            )
            .await?;
        let work = async {
            for image in &prepared {
                match self.build_one(image, logs, cancel.clone()).await {
                    Outcome::Succeeded => {}
                    other => return other,
                }
            }
            Outcome::Succeeded
        };
        let outcome = match tokio::time::timeout(BUILD_TIMEOUT, work).await {
            Ok(outcome) => outcome,
            Err(_) => failed(format!(
                "Building the images took longer than {} minutes",
                BUILD_TIMEOUT.as_secs() / 60
            )),
        };
        match outcome {
            Outcome::Succeeded => Ok(hold),
            other => Err(other),
        }
    }

    /// Checks the build file of `image` and hashes its context (off the async threads).
    ///
    /// # Arguments
    /// * `workflow` - the workflow directory
    /// * `image` - build file and name
    async fn prepare(workflow: &Path, image: &ImageBuild) -> Result<PreparedImage, Outcome> {
        let (workflow_directory, dockerfile) =
            (workflow.to_path_buf(), image.dockerfile().to_string());
        let prepared =
            tokio::task::spawn_blocking(move || Self::context_of(&workflow_directory, &dockerfile))
                .await;
        let (context, file, hash) = match prepared {
            Ok(Ok(prepared)) => prepared,
            Ok(Err(error)) => return Err(failed(error.to_string())),
            Err(error) => {
                return Err(failed(format!("Hashing the build context failed: {error}")));
            }
        };
        Ok(PreparedImage {
            name: image.name().to_string(),
            dockerfile: image.dockerfile().to_string(),
            hash_tag: format!("{}:{hash}", repository(image.name())),
            file,
            context,
            hash,
        })
    }

    /// Makes one image available under its name.
    ///
    /// # Arguments
    /// * `image` - the prepared build
    /// * `logs` - run log
    /// * `cancel` - turns `true` when a user stops the run
    async fn build_one(
        &self,
        image: &PreparedImage,
        logs: &mpsc::Sender<LogLine>,
        cancel: watch::Receiver<bool>,
    ) -> Outcome {
        let hash_tag = image.hash_tag.clone();
        let say = |text: String| async {
            let _ = logs
                .send(LogLine {
                    stream: LogStream::Engine,
                    line: text,
                })
                .await;
        };

        if self.has_image(&hash_tag).await {
            say(format!(
                "Image {} is up to date ({hash_tag}), not building again",
                image.name
            ))
            .await;
            let mut args = vec!["tag".to_string(), hash_tag, image.name.clone()];
            return self.run(&mut args, logs, cancel).await;
        }
        say(format!(
            "Building image {} from {}",
            image.name, image.dockerfile
        ))
        .await;
        let mut args = vec!["build".to_string()];
        if self.kind == RuntimeKind::Docker {
            // Plain output instead of the terminal animation, readable in a log
            args.extend(["--progress".into(), "plain".into()]);
        }
        // Host paths on purpose: the runtime's client reads the build file and context itself,
        // no container sees them, so no `PathMapping` applies
        args.extend([
            "--file".into(),
            image.file.display().to_string(),
            "--tag".into(),
            image.name.clone(),
            "--tag".into(),
            hash_tag,
            image.context.display().to_string(),
        ]);
        self.run(&mut args, logs, cancel).await
    }

    /// Runs the runtime with `args`, forwarding its output. A failure counts as a failed start.
    ///
    /// # Arguments
    /// * `args` - arguments after the runtime binary
    /// * `logs` - run log
    /// * `cancel` - turns `true` when a user stops the run
    async fn run(
        &self,
        args: &mut Vec<String>,
        logs: &mpsc::Sender<LogLine>,
        cancel: watch::Receiver<bool>,
    ) -> Outcome {
        let spec = LaunchSpec {
            program: self.program.clone(),
            args: std::mem::take(args),
            container_name: None,
            scratch_directory: None,
        };
        match run_process(&spec, logs.clone(), cancel).await {
            Outcome::Failed {
                exit_code, error, ..
            } => Outcome::Failed {
                exit_code,
                error: Some(format!(
                    "Building the images failed: {}",
                    error.unwrap_or_default()
                )),
                reason: Some(FailureReason::StartFailed),
            },
            other => other,
        }
    }

    /// Whether the runtime has an image with this tag.
    ///
    /// # Arguments
    /// * `tag` - `repository:tag`
    async fn has_image(&self, tag: &str) -> bool {
        Command::new(&self.program)
            .hide_console_window()
            .args(["image", "inspect", tag])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await
            .is_ok_and(|status| status.success())
    }

    /// Checks the build file and hashes its context.
    ///
    /// # Arguments
    /// * `workflow` - the workflow directory
    /// * `dockerfile` - build file relative to it
    ///
    /// # Returns
    /// The context directory, the build file path and the content hash (hex).
    pub fn context_of(
        workflow: &Path,
        dockerfile: &str,
    ) -> Result<(PathBuf, PathBuf, String), Error> {
        let invalid = || Error::BuildFile(dockerfile.to_string());
        let relative = sanitize_relative(Path::new(dockerfile));
        if relative.as_os_str().is_empty()
            || !macworp_core::paths::is_confined(Path::new(dockerfile))
        {
            return Err(invalid());
        }
        let mut current = workflow.to_path_buf();
        for component in relative.components() {
            current.push(component);
            let metadata = std::fs::symlink_metadata(&current).map_err(|_| invalid())?;
            if metadata.file_type().is_symlink() {
                return Err(invalid());
            }
        }
        if !std::fs::symlink_metadata(&current)
            .map_err(|_| invalid())?
            .is_file()
        {
            return Err(invalid());
        }
        let context = current.parent().ok_or_else(invalid)?.to_path_buf();
        let hash = Self::hash_context(&context, &current)
            .map_err(|message| Error::Context(dockerfile.to_string(), message))?;
        Ok((context, current, hash))
    }

    /// Hash of everything a build can read: relative path and content of every file below
    /// `context` (sorted, symlinks as their target text), and which file is the build file.
    ///
    /// # Arguments
    /// * `context` - build context directory
    /// * `dockerfile` - the build file inside it
    fn hash_context(context: &Path, dockerfile: &Path) -> Result<String, String> {
        let mut files = Vec::new();
        Self::list_files(context, &mut files)?;
        files.sort();
        let mut hasher = Sha256::new();
        hasher.update(
            dockerfile
                .strip_prefix(context)
                .unwrap_or(dockerfile)
                .to_string_lossy()
                .as_bytes(),
        );
        hasher.update([0]);
        let mut buffer = [0u8; 64 * 1024];
        for file in files {
            let relative = file.strip_prefix(context).unwrap_or(&file);
            hasher.update(relative.to_string_lossy().replace('\\', "/").as_bytes());
            hasher.update([0]);
            let metadata = std::fs::symlink_metadata(&file).map_err(|e| e.to_string())?;
            if metadata.file_type().is_symlink() {
                let target = std::fs::read_link(&file).map_err(|e| e.to_string())?;
                hasher.update(b"link:");
                hasher.update(target.to_string_lossy().as_bytes());
            } else {
                let mut reader = std::fs::File::open(&file).map_err(|e| e.to_string())?;
                loop {
                    let read = reader.read(&mut buffer).map_err(|e| e.to_string())?;
                    if read == 0 {
                        break;
                    }
                    hasher.update(&buffer[..read]);
                }
            }
            hasher.update([0]);
        }
        let digest = hasher.finalize();
        Ok(digest
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()[..HASH_LENGTH]
            .to_string())
    }

    /// Collects the files and symlinks below `directory`, not following links.
    ///
    /// # Arguments
    /// * `directory` - folder to read
    /// * `files` - receives the paths, at most [`CONTEXT_FILE_LIMIT`]
    fn list_files(directory: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
        for entry in std::fs::read_dir(directory).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let kind = entry.file_type().map_err(|e| e.to_string())?;
            if kind.is_dir() {
                Self::list_files(&entry.path(), files)?;
            } else {
                files.push(entry.path());
            }
            if files.len() > CONTEXT_FILE_LIMIT {
                return Err(format!("more than {CONTEXT_FILE_LIMIT} files"));
            }
        }
        Ok(())
    }
}

/// `name` without its tag.
///
/// # Arguments
/// * `name` - validated image name `<prefix>/<name>:<version>`
fn repository(name: &str) -> &str {
    name.rsplit_once(':')
        .map_or(name, |(repository, _)| repository)
}

/// A start failure with `message`.
///
/// # Arguments
/// * `message` - what went wrong, shown as the run's error
pub(crate) fn failed(message: String) -> Outcome {
    Outcome::Failed {
        exit_code: None,
        error: Some(message),
        reason: Some(FailureReason::StartFailed),
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use macworp_core::workflow_configuration::{WorkflowConfiguration, source::LocalSource};

    /// A fake runtime: a script that records its calls in `calls` and answers
    /// `image inspect` by the presence of `known`.
    fn fake_runtime(directory: &Path) -> (ImageBuilder, PathBuf) {
        use std::os::unix::fs::PermissionsExt;
        let calls = directory.join("calls");
        let script = directory.join("runtime");
        std::fs::write(
            &script,
            format!(
                "#!/bin/sh\necho \"$@\" >> '{calls}'\n\
                 if [ \"$1\" = image ]; then [ -e '{known}' ]; exit $?; fi\n\
                 if [ \"$1\" = build ]; then echo building; [ ! -e '{broken}' ]; exit $?; fi\n\
                 exit 0\n",
                calls = calls.display(),
                known = directory.join("known").display(),
                broken = directory.join("broken").display(),
            ),
        )
        .unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        (ImageBuilder::new(script, RuntimeKind::Podman), calls)
    }

    /// A job for the local workflow in `workflow` with the given builds.
    fn job(workflow: &Path, builds: &[(&str, &str)]) -> Job {
        let mut configuration = WorkflowConfiguration::default();
        *configuration.source_mut() =
            Source::Local(LocalSource::new(workflow.to_path_buf(), "main.nf".into()));
        *configuration.images_mut() = builds
            .iter()
            .map(|(file, name)| ImageBuild::new((*file).into(), (*name).into()))
            .collect();
        Job {
            run_id: 1,
            project_dir: PathBuf::new(),
            reference_dir: None,
            apptainer_cache: None,
            cwl_cache: None,
            configuration,
            progress: None,
        }
    }

    async fn build(
        builder: &ImageBuilder,
        job: &Job,
        cancel: watch::Receiver<bool>,
    ) -> (Outcome, Vec<LogLine>) {
        let (tx, mut rx) = mpsc::channel(256);
        let outcome = match builder.build_all(job, &tx, cancel).await {
            Ok(_hold) => Outcome::Succeeded,
            Err(outcome) => outcome,
        };
        drop(tx);
        let mut lines = Vec::new();
        while let Some(line) = rx.recv().await {
            lines.push(line);
        }
        (outcome, lines)
    }

    /// An unknown image is built with both tags, a known content hash is only tagged.
    #[tokio::test]
    async fn builds_once_per_content() {
        let directory = tempfile::tempdir().unwrap();
        let workflow = directory.path().join("wf");
        std::fs::create_dir_all(workflow.join("docker")).unwrap();
        std::fs::write(workflow.join("docker/Dockerfile"), "FROM scratch").unwrap();
        let (builder, calls) = fake_runtime(directory.path());
        let job = job(&workflow, &[("docker/Dockerfile", "team/tool:1.0")]);
        let (_tx, cancel) = watch::channel(false);

        let (outcome, lines) = build(&builder, &job, cancel.clone()).await;
        assert_eq!(outcome, Outcome::Succeeded);
        assert!(lines.iter().any(|l| l.line == "building"));
        let recorded = std::fs::read_to_string(&calls).unwrap();
        let build_line = recorded.lines().find(|l| l.starts_with("build")).unwrap();
        assert!(build_line.contains("--tag team/tool:1.0 --tag team/tool:"));
        assert!(build_line.ends_with(&format!("{}", workflow.join("docker").display())));
        assert!(!build_line.contains("--privileged") && !build_line.contains("--volume"));

        // Same content known to the runtime: tag only
        std::fs::write(directory.path().join("known"), "").unwrap();
        std::fs::remove_file(&calls).unwrap();
        let (outcome, _) = build(&builder, &job, cancel).await;
        assert_eq!(outcome, Outcome::Succeeded);
        let recorded = std::fs::read_to_string(&calls).unwrap();
        assert!(!recorded.lines().any(|l| l.starts_with("build")));
        assert!(recorded.lines().any(|l| l.starts_with("tag team/tool:")));
    }

    /// Changing a file of the context changes the hash tag.
    #[test]
    fn hash_follows_the_context() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join("Dockerfile"), "FROM scratch").unwrap();
        std::fs::write(directory.path().join("script.sh"), "a").unwrap();
        let hash = |_: ()| {
            ImageBuilder::context_of(directory.path(), "Dockerfile")
                .unwrap()
                .2
        };
        let first = hash(());
        assert_eq!(first, hash(()));
        assert_eq!(first.len(), HASH_LENGTH);
        std::fs::write(directory.path().join("script.sh"), "b").unwrap();
        assert_ne!(first, hash(()));
    }

    /// A failed build fails the start with the output in the log; unsafe build files are rejected.
    #[tokio::test]
    async fn failures_and_unsafe_files() {
        let directory = tempfile::tempdir().unwrap();
        let workflow = directory.path().join("wf");
        std::fs::create_dir_all(&workflow).unwrap();
        std::fs::write(workflow.join("Dockerfile"), "FROM scratch").unwrap();
        std::os::unix::fs::symlink(directory.path(), workflow.join("linked")).unwrap();
        let (builder, _) = fake_runtime(directory.path());
        let (_tx, cancel) = watch::channel(false);

        std::fs::write(directory.path().join("broken"), "").unwrap();
        let (outcome, lines) = build(
            &builder,
            &job(&workflow, &[("Dockerfile", "a/b:1")]),
            cancel.clone(),
        )
        .await;
        assert!(matches!(
            outcome,
            Outcome::Failed { reason: Some(FailureReason::StartFailed), error: Some(e), .. }
                if e.contains("Building the images failed")
        ));
        assert!(lines.iter().any(|l| l.line == "building"));

        for file in [
            "linked/wf/Dockerfile",
            "missing/Dockerfile",
            "../Dockerfile",
        ] {
            let (outcome, _) = build(
                &builder,
                &job(&workflow, &[(file, "a/b:1")]),
                cancel.clone(),
            )
            .await;
            assert!(matches!(outcome, Outcome::Failed { .. }), "{file}");
        }
    }

    /// Two runs with the same name and different content do not overlap: the second waits
    /// until the first released the name. Equal content is shared, other names do not wait.
    #[tokio::test]
    async fn same_name_different_content_waits() {
        let locks = std::sync::Arc::new(ImageLocks::default());
        let (logs, mut log_rx) = mpsc::channel(16);
        let (_keep, cancel) = watch::channel(false);
        let image = |name: &str, hash: &str| vec![(name.to_string(), hash.to_string())];

        let first = locks
            .acquire(image("a/b:1", "h1"), &logs, cancel.clone())
            .await
            .unwrap();
        // Same content and another name go through at once
        let shared = locks
            .acquire(image("a/b:1", "h1"), &logs, cancel.clone())
            .await
            .unwrap();
        let other = locks
            .acquire(image("a/c:1", "h2"), &logs, cancel.clone())
            .await
            .unwrap();

        // Different content waits, and tells the run log once
        let waiter = tokio::spawn({
            let (locks, logs, cancel) = (locks.clone(), logs.clone(), cancel.clone());
            async move {
                locks
                    .acquire(image("a/b:1", "h3"), &logs, cancel)
                    .await
                    .map(drop)
            }
        });
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert!(!waiter.is_finished());
        assert!(log_rx.recv().await.unwrap().line.starts_with("Waiting"));

        // Still blocked while one of the two sharing runs is active
        drop(first);
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert!(!waiter.is_finished());
        drop(shared);
        tokio::time::timeout(Duration::from_secs(5), waiter)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        drop(other);
        assert!(locks.in_use.lock().unwrap().is_empty());
    }

    /// A run that waits for a name can be cancelled and then holds nothing.
    #[tokio::test]
    async fn waiting_run_can_be_cancelled() {
        let locks = std::sync::Arc::new(ImageLocks::default());
        let (logs, _log_rx) = mpsc::channel(16);
        let (tx, cancel) = watch::channel(false);
        let image = |hash: &str| vec![("a/b:1".to_string(), hash.to_string())];
        let _first = locks
            .acquire(image("h1"), &logs, cancel.clone())
            .await
            .unwrap();
        let waiter = tokio::spawn({
            let (locks, logs) = (locks.clone(), logs.clone());
            async move { locks.acquire(image("h2"), &logs, cancel).await.map(drop) }
        });
        tokio::time::sleep(Duration::from_millis(100)).await;
        tx.send(true).unwrap();
        let result = tokio::time::timeout(Duration::from_secs(5), waiter)
            .await
            .unwrap()
            .unwrap();
        assert!(matches!(result, Err(Outcome::Cancelled)));
        assert_eq!(locks.in_use.lock().unwrap().get("a/b:1").unwrap().1, 1);
    }

    /// Through the builder: a second run of another content waits for the first run's hold.
    #[tokio::test]
    async fn builder_holds_names_until_the_hold_is_dropped() {
        let directory = tempfile::tempdir().unwrap();
        let (builder, _) = fake_runtime(directory.path());
        let make = |content: &str| {
            let workflow = directory.path().join(format!("wf-{content}"));
            std::fs::create_dir_all(&workflow).unwrap();
            std::fs::write(workflow.join("Dockerfile"), content).unwrap();
            job(&workflow, &[("Dockerfile", "team/tool:1.0")])
        };
        let (tx, _rx) = mpsc::channel(256);
        let (_keep, cancel) = watch::channel(false);
        let first = builder
            .build_all(&make("one"), &tx, cancel.clone())
            .await
            .unwrap();
        let second = tokio::spawn({
            let (builder, tx, cancel, job) =
                (builder.clone(), tx.clone(), cancel.clone(), make("two"));
            async move { builder.build_all(&job, &tx, cancel).await.map(drop) }
        });
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert!(!second.is_finished());
        drop(first);
        tokio::time::timeout(Duration::from_secs(5), second)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
    }

    /// A cancel during the build stops it.
    #[tokio::test]
    async fn cancel_stops_the_build() {
        let directory = tempfile::tempdir().unwrap();
        let workflow = directory.path().join("wf");
        std::fs::create_dir_all(&workflow).unwrap();
        std::fs::write(workflow.join("Dockerfile"), "FROM scratch").unwrap();
        let slow = directory.path().join("slow");
        std::fs::write(
            &slow,
            "#!/bin/sh\nif [ \"$1\" = build ]; then sleep 30; fi\nexit 1\n",
        )
        .unwrap();
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&slow, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let builder = ImageBuilder::new(slow, RuntimeKind::Podman);
        let (tx, cancel) = watch::channel(false);
        let job = job(&workflow, &[("Dockerfile", "a/b:1")]);
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(300)).await;
            let _ = tx.send(true);
        });
        let (outcome, _) = build(&builder, &job, cancel).await;
        assert_eq!(outcome, Outcome::Cancelled);
    }
}
