use std::path::{Path, PathBuf};

use macworp_core::{entities::FailureReason, paths::PathMapping};
use rand::{RngExt, distr::Alphanumeric};
use tokio::sync::{mpsc, watch};

use super::{ExecutorError, Job, LaunchSpec, LogLine, Outcome, run_process};

/// `path` (a path on this machine) as the container sees it, see [`PathMapping`]. Every path
/// that is part of a command inside a container goes through this.
///
/// # Arguments
/// * `mapping` - How host paths look inside the runtime's containers
/// * `path` - A path on this machine
pub(super) fn path_arg(mapping: PathMapping, path: &Path) -> String {
    mapping.container(path)
}

/// Value of a `-v` option that mounts the host directory `path` at its container-side path (the
/// same path inside and outside, as far as [`PathMapping`] allows), so mounts the engine
/// creates itself resolve on the runtime's side. The source stays the host form because the
/// runtime's client reads it.
///
/// # Arguments
/// * `mapping` - How host paths look inside the runtime's containers
/// * `path` - Directory on this machine
/// * `read_only` - Add `:ro`
pub(super) fn mount_arg(mapping: PathMapping, path: &Path, read_only: bool) -> String {
    let source = path.to_string_lossy();
    // The runtime's client does not read the verbatim form `canonicalize` gives on Windows
    let source = source.strip_prefix(r"\\?\").unwrap_or(&source);
    let target = mapping.container(path);
    if read_only {
        format!("{source}:{target}:ro")
    } else {
        format!("{source}:{target}")
    }
}

/// Registry (and namespace) the prebuilt engine images are pulled from unless
/// [`REGISTRY_VARIABLE`] says otherwise.
pub const DEFAULT_REGISTRY: &str = "ghcr.io/cubimedrub";

/// Environment variable that replaces [`DEFAULT_REGISTRY`], for a mirror or a private registry.
/// An empty value leaves the image name without a registry, which finds locally built images
/// (`macworp-nextflow:25.04.6`).
pub const REGISTRY_VARIABLE: &str = "MACWORP_ENGINE_REGISTRY";

/// The image `engine` runs on unless its workflow pins a version: the value of `variable` if
/// set and not empty, else `<registry>/macworp-<engine>:<default version>` from the engine
/// catalog ([`macworp_core::workflow_configuration::engine::Engine::catalog`]).
///
/// # Arguments
/// * `engine` - an engine with prebuilt images (not `Container`)
/// * `variable` - name of the environment variable that replaces the image as a whole
pub fn default_engine_image(
    engine: &macworp_core::workflow_configuration::engine::Engine,
    variable: &str,
) -> String {
    if let Some(image) = std::env::var(variable)
        .ok()
        .filter(|image| !image.trim().is_empty())
    {
        return image;
    }
    let catalog = engine
        .catalog()
        .expect("engines with an executor image have a catalog");
    let repository = engine.image_repository().expect("same as the catalog");
    let registry = std::env::var(REGISTRY_VARIABLE).unwrap_or_else(|_| DEFAULT_REGISTRY.into());
    let registry = registry.trim().trim_end_matches('/');
    if registry.is_empty() {
        format!("{repository}:{}", catalog.default)
    } else {
        format!("{registry}/{repository}:{}", catalog.default)
    }
}

/// The engine image for a job: `image` with its tag replaced by the engine version pinned in
/// the workflow (`macworp-nextflow:latest` + `25.04.6` = `macworp-nextflow:25.04.6`), `image`
/// itself when the workflow pins nothing. Fails for a version that is not a valid image tag, so
/// nothing but a tag reaches the runtime's command line.
///
/// # Arguments
/// * `image` - the executor's default engine image
/// * `version` - the version pinned in the workflow configuration, if any
pub fn engine_image(image: &str, version: Option<&str>) -> Result<String, ExecutorError> {
    let Some(version) = version else {
        return Ok(image.to_string());
    };
    macworp_core::workflow_configuration::validate_engine_version(version)
        .map_err(ExecutorError::InvalidEngineVersion)?;
    // A colon after the last slash starts the tag (an earlier one belongs to a registry port);
    // a digest (`@sha256:...`) is dropped with it
    let name = image.split('@').next().unwrap_or(image);
    let name = match name.rfind(':') {
        Some(colon) if !name[colon..].contains('/') => &name[..colon],
        _ => name,
    };
    Ok(format!("{name}:{version}"))
}

/// `<uid>:<gid>` of the worker process, `None` where there is no such thing (not a Unix host).
pub fn worker_user() -> Option<String> {
    #[cfg(unix)]
    {
        // SAFETY: `geteuid` and `getegid` have no preconditions and cannot fail
        let (user, group) = unsafe { (libc::geteuid(), libc::getegid()) };
        Some(format!("{user}:{group}"))
    }
    #[cfg(not(unix))]
    {
        None
    }
}

/// `run` arguments that make an engine container run as the worker's user instead of root, so
/// everything the engine writes below the shared storage belongs to the user that owns it and the
/// worker can manage (clean up) it. `HOME` points at `/tmp` because that user has no home
/// directory in the image. Empty where [`worker_user`] is unknown (the engine then runs as the
/// image's user).
///
/// Reaching the runtime socket as that user is part of
/// [`crate::container_runtime::RuntimeInfo::engine_access_args`], Snakemake adds its own options
/// for Apptainer.
pub fn worker_user_args() -> Vec<String> {
    match worker_user() {
        Some(user) => vec!["--user".into(), user, "-e".into(), "HOME=/tmp".into()],
        None => Vec::new(),
    }
}

/// Whether SELinux enforces its policy on this host (Fedora, RHEL): `/sys/fs/selinux/enforce`
/// reads `1`. `false` where SELinux is absent or permissive, and off Linux.
pub fn selinux_enforcing() -> bool {
    std::fs::read_to_string("/sys/fs/selinux/enforce").is_ok_and(|value| value.trim() == "1")
}

/// `run` arguments that keep SELinux from denying the engine container its bind mounts (project,
/// workflow, reference data, image cache): the host files carry labels the container's type may
/// not touch. Disabling the label confinement of this one container is chosen over `:z`/`:Z`,
/// which would relabel the host directories for good (including folders the desktop user picks
/// and shared network storage without labels). Empty when SELinux does not enforce.
///
/// # Arguments
/// * `enforcing` - see [`selinux_enforcing`]
pub fn selinux_args(enforcing: bool) -> Vec<String> {
    if enforcing {
        vec!["--security-opt".into(), "label=disable".into()]
    } else {
        Vec::new()
    }
}

/// `<runtime> run --rm --name macworp-run-<id>-<random> -v <project>:<project> -w <project> [-v <dir>:<dir>:ro]... <image> <command>`
///
/// The project directory is mounted read-write and every `read_only` directory read-only, all at
/// their container-side path ([`PathMapping`], the host path itself where the runtime runs on
/// the host) so mounts the engine creates itself resolve on the runtime's side. `engine_access` are
/// extra `run` arguments for sibling-container access ([`crate::container_runtime::RuntimeInfo::engine_access_args`]).
/// Hardening is not part of it yet, that is P6.
///
/// # Arguments
/// * `runtime_binary` - The runtime's command line client
/// * `image` - Image to run
/// * `job` - The job, whose project directory is mounted and used as working directory
/// * `mapping` - How host paths look inside the runtime's containers
/// * `read_only` - Further host directories to mount read-only
/// * `engine_access` - Extra `run` arguments (user, runtime socket)
/// * `command` - Command and arguments inside the container, paths already in container form
pub fn containerized_spec(
    runtime_binary: &Path,
    image: &str,
    job: &Job,
    mapping: PathMapping,
    read_only: &[&Path],
    engine_access: &[String],
    command: Vec<String>,
) -> LaunchSpec {
    // Run ids are only unique per database: several backends (or desktop instances) can share one
    // container runtime, so the name carries a random suffix
    let suffix: String = rand::rng()
        .sample_iter(Alphanumeric)
        .take(8)
        .map(char::from)
        .collect();
    let container_name = format!("macworp-run-{}-{suffix}", job.run_id);

    let mut args: Vec<String> = vec![
        "run".into(),
        "--rm".into(),
        "--name".into(),
        container_name.clone(),
        "-v".into(),
        mount_arg(mapping, &job.project_dir, false),
        "-w".into(),
        path_arg(mapping, &job.project_dir),
    ];
    for dir in read_only {
        args.extend(["-v".into(), mount_arg(mapping, dir, true)]);
    }
    args.extend(selinux_args(selinux_enforcing()));
    args.extend(engine_access.iter().cloned());
    if let Some(progress) = &job.progress {
        args.extend(progress.run_args.iter().cloned());
    }
    args.push(image.to_string());
    args.extend(command);

    LaunchSpec {
        program: PathBuf::from(runtime_binary),
        args,
        container_name: Some(container_name),
        scratch_directory: None,
    }
}

/// Runs the spec, or reports why there is none as a failed run. The scratch directory of the
/// spec is removed afterwards.
pub async fn run_spec(
    spec: Result<LaunchSpec, ExecutorError>,
    logs: mpsc::Sender<LogLine>,
    cancel: watch::Receiver<bool>,
) -> Outcome {
    match spec {
        Ok(spec) => {
            let outcome = run_process(&spec, logs, cancel).await;
            if let Some(directory) = &spec.scratch_directory
                && let Err(error) = std::fs::remove_dir_all(directory)
            {
                tracing::warn!("Cannot remove {}: {error}", directory.display());
            }
            outcome
        }
        Err(e) => Outcome::Failed {
            exit_code: None,
            error: Some(e.to_string()),
            reason: Some(FailureReason::StartFailed),
        },
    }
}

/// `args` without the worker's user options ([`worker_user_args`]) and the SELinux option
/// ([`selinux_args`]), so argument lists can be compared on any host (the user differs per
/// machine and is absent off Unix, SELinux enforces only on some Linux hosts).
///
/// # Arguments
/// * `args` - the `run` arguments of a launch spec
#[cfg(test)]
pub(super) fn without_worker_user(mut args: Vec<String>) -> Vec<String> {
    let user = worker_user_args();
    if let Some(start) = args
        .windows(user.len().max(1))
        .position(|part| part == user)
    {
        args.drain(start..start + user.len());
    }
    let label = selinux_args(true);
    if let Some(start) = args.windows(label.len()).position(|part| part == label) {
        args.drain(start..start + label.len());
    }
    args
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The label option is added when SELinux enforces and nothing otherwise, and the test
    /// helper strips it again so argument lists compare equal on every host.
    #[test]
    fn selinux_option_only_when_enforcing() {
        assert_eq!(
            selinux_args(true),
            ["--security-opt".to_string(), "label=disable".to_string()]
        );
        assert!(selinux_args(false).is_empty());
        let args = vec!["run".to_string()]
            .into_iter()
            .chain(selinux_args(true))
            .chain(["img".to_string()])
            .collect();
        assert_eq!(without_worker_user(args), ["run", "img"]);
    }
}

#[cfg(test)]
/// Tests of [`engine_image`].
mod engine_image_tests {
    use super::*;

    /// Without a pinned version the default image is used unchanged.
    #[test]
    fn unpinned_keeps_the_default() {
        assert_eq!(
            engine_image("macworp-nextflow:latest", None).unwrap(),
            "macworp-nextflow:latest"
        );
    }

    /// A pinned version replaces the tag (or digest) and keeps registry ports.
    #[test]
    fn pinned_replaces_the_tag() {
        for (image, expected) in [
            ("macworp-nextflow:latest", "macworp-nextflow:25.04.6"),
            ("macworp-nextflow", "macworp-nextflow:25.04.6"),
            (
                "registry:5000/macworp-nextflow",
                "registry:5000/macworp-nextflow:25.04.6",
            ),
            (
                "registry:5000/macworp-nextflow:old",
                "registry:5000/macworp-nextflow:25.04.6",
            ),
            ("macworp-nextflow@sha256:abc", "macworp-nextflow:25.04.6"),
        ] {
            assert_eq!(
                engine_image(image, Some("25.04.6")).unwrap(),
                expected,
                "{image}"
            );
        }
    }

    /// A version that could be an option is rejected.
    #[test]
    fn invalid_version_is_rejected() {
        assert!(matches!(
            engine_image("macworp-nextflow:latest", Some("--privileged")),
            Err(ExecutorError::InvalidEngineVersion(_))
        ));
    }
}

#[cfg(test)]
/// Tests of [`default_engine_image`].
mod default_engine_image_tests {
    use super::*;
    use macworp_core::workflow_configuration::engine::Engine;

    /// Without overrides the image is the catalog default on the default registry.
    #[test]
    fn default_is_registry_repository_and_catalog_version() {
        let image = default_engine_image(&Engine::Nextflow, "MACWORP_TEST_VARIABLE_NOT_SET");
        let version = &Engine::Nextflow.catalog().unwrap().default;
        // The registry variable may be set on a developer machine
        if std::env::var(REGISTRY_VARIABLE).is_err() {
            assert_eq!(
                image,
                format!("{DEFAULT_REGISTRY}/macworp-nextflow:{version}")
            );
        }
        assert!(image.ends_with(&format!("macworp-nextflow:{version}")));
    }
}
