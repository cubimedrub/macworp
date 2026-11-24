use std::{
    fmt,
    future::Future,
    io,
    path::{Path, PathBuf},
    pin::Pin,
    process::Stdio,
    time::Duration,
};

use macworp_core::workflow_configuration::engine::Engine;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;
use tokio::process::Command;

use super::images::ImageInfo;

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(10);
/// Starting a container takes longer than querying the runtime
const PROBE_TIMEOUT: Duration = Duration::from_secs(30);
/// Listing or removing images and reading events may take a while on a big store
pub(super) const LIST_TIMEOUT: Duration = Duration::from_secs(60);

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RuntimeKind {
    Podman,
    Docker,
    /// WSL Containers (`wslc.exe`)
    Wslc,
}

impl RuntimeKind {
    pub fn binary_name(self) -> &'static str {
        match self {
            Self::Podman => "podman",
            Self::Docker => "docker",
            Self::Wslc => "wslc",
        }
    }

    /// Whether workflows of `engine` can run on this runtime.
    ///
    /// Nextflow, Snakemake and CWL run inside an engine container that starts sibling
    /// containers through the runtime socket. Snakemake runs its tool containers with Apptainer
    /// inside the engine container, which needs user namespaces and `/proc` mounts: verified on
    /// Docker (macOS) and on rootless Podman on Linux, but Podman on a macOS machine VM fails
    /// mounting `/proc`, so Podman only counts as Snakemake-capable on Linux hosts, and not on
    /// those enforcing SELinux (see [`Self::podman_runs_snakemake`] and
    /// `docs/container-runtimes.md`). `wslc` offers no sibling-container access, so it only runs
    /// plain `Container` workflows.
    pub fn supports_engine(self, engine: &Engine) -> bool {
        match self {
            Self::Docker => true,
            Self::Podman => !matches!(engine, Engine::Snakemake) || Self::podman_runs_snakemake(),
            Self::Wslc => matches!(engine, Engine::Container),
        }
    }
}

impl RuntimeKind {
    /// Whether rootless Podman can run Snakemake on this host. Apptainer inside the engine
    /// container needs user namespaces and `/proc` mounts: that works on Linux hosts, not in the
    /// VM of a macOS Podman machine (`/proc` cannot be mounted), and not where SELinux enforces:
    /// the images Apptainer builds carry `security.selinux` attributes that it cannot write back
    /// as an unprivileged user when it unpacks them (`write_xattr: could not write xattr
    /// security.selinux`, checked on Fedora 45 with Podman 6.2, 2026-11-20). Docker on such a
    /// host works.
    pub fn podman_runs_snakemake() -> bool {
        cfg!(target_os = "linux") && !crate::executors::selinux_enforcing()
    }
}

impl fmt::Display for RuntimeKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.binary_name())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeInfo {
    pub kind: RuntimeKind,
    pub version: String,
    /// Runtime daemon/engine runs without root privileges, `None` if the runtime does not say
    pub rootless: Option<bool>,
    /// Where images and containers are stored
    pub storage_root: Option<PathBuf>,
    /// API socket of the runtime, if it exposes one
    pub socket: Option<PathBuf>,
}

/// Where the engine container reaches the runtime API, see [`RuntimeInfo::engine_access_args`].
const ENGINE_SOCKET: &str = "/run/container.sock";
/// Docker socket as seen by the daemon (inside the VM on Docker Desktop), `docker info` does not report it.
const DEFAULT_DOCKER_SOCKET: &str = "/var/run/docker.sock";

impl RuntimeInfo {
    /// `<runtime> run` arguments that let an engine container start sibling containers through
    /// the runtime socket (docker CLI inside the engine image, `DOCKER_HOST` points at the mount).
    ///
    /// Spiked on macOS with Docker Desktop and a rootless podman machine, see ADR 0004. Nested
    /// podman was rejected: needs extra privileges and has no shared image cache. `None` if the
    /// runtime offers no socket (wslc). A rootful socket is root-equivalent, see doctor.
    pub fn engine_access_args(&self) -> Option<Vec<String>> {
        let (socket, extra): (PathBuf, Vec<String>) = match self.kind {
            // SELinux (Fedora CoreOS machine, RHEL) denies the container access to the socket.
            // `keep-id` maps the worker's user to itself, so the engine, which runs as that
            // user, owns the socket inside the container as it does outside.
            RuntimeKind::Podman => (
                self.socket.clone()?,
                ["--security-opt", "label=disable", "--userns=keep-id"]
                    .map(String::from)
                    .into(),
            ),
            RuntimeKind::Docker => {
                let socket = self
                    .socket
                    .clone()
                    .unwrap_or_else(|| PathBuf::from(DEFAULT_DOCKER_SOCKET));
                // The engine runs as the worker's user, who needs the socket's group
                let group = Self::socket_group(&socket);
                (socket, vec!["--group-add".into(), group])
            }
            RuntimeKind::Wslc => return None,
        };
        let mut args = vec![
            "-v".to_string(),
            format!("{}:{ENGINE_SOCKET}", socket.display()),
            "-e".to_string(),
            format!("DOCKER_HOST=unix://{ENGINE_SOCKET}"),
        ];
        if self.kind == RuntimeKind::Podman {
            // `podman-remote` of the engine image (Toil's Podman mode) reads this one
            args.extend([
                "-e".into(),
                format!("CONTAINER_HOST=unix://{ENGINE_SOCKET}"),
            ]);
        }
        args.extend(extra);
        Some(args)
    }

    /// Group id (as text) that gives the engine container access to a Docker socket.
    ///
    /// On a Linux host that is the group of the socket file (`docker`, or the user's own group
    /// for rootless Docker). Elsewhere the daemon runs in a VM (Docker Desktop) and mounts the
    /// socket as `root:root`, so group `0` is the one that grants access (verified on macOS with
    /// Docker Desktop, the mount is `0:0 660`).
    ///
    /// # Arguments
    /// * `socket` - the socket path on the host
    fn socket_group(socket: &Path) -> String {
        #[cfg(target_os = "linux")]
        {
            use std::os::unix::fs::MetadataExt;
            if let Ok(metadata) = std::fs::metadata(socket) {
                return metadata.gid().to_string();
            }
        }
        let _ = socket;
        "0".into()
    }
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("`{0}` not found on PATH")]
    NotFound(String),
    #[error("failed to run `{binary}`: {source}")]
    Spawn { binary: String, source: io::Error },
    #[error("`{binary}` timed out after {timeout:?}")]
    Timeout { binary: String, timeout: Duration },
    #[error("`{binary}` failed ({status}): {stderr}")]
    Failed {
        binary: String,
        status: String,
        stderr: String,
    },
    #[error("unable to parse output of `{binary}`: {source}")]
    Parse {
        binary: String,
        source: serde_json::Error,
    },
    /// The runtime has no command for the requested operation
    #[error("`{0}` is not supported by this container runtime")]
    Unsupported(&'static str),
}

/// A container runtime the worker can start engine containers with.
///
/// Dyn-compatible on purpose: the concrete runtime is picked at startup.
pub trait ContainerRuntime: fmt::Debug + Send + Sync {
    fn kind(&self) -> RuntimeKind;
    fn binary(&self) -> &Path;
    fn supports_engine(&self, engine: &Engine) -> bool {
        self.kind().supports_engine(engine)
    }
    /// Queries the runtime; failure means it is installed but not usable (e.g. daemon down).
    fn info(&self) -> BoxFuture<'_, Result<RuntimeInfo, Error>>;
    /// Starts a short-lived container (`<runtime> run --rm <args>`) to find out whether the
    /// runtime accepts certain options, for example an AppArmor profile. The default does
    /// nothing and reports success.
    ///
    /// # Arguments
    /// * `args` - everything after `run --rm`: options, image, command
    fn probe_container(&self, _args: Vec<String>) -> BoxFuture<'_, Result<(), Error>> {
        Box::pin(async { Ok(()) })
    }
    /// Lists the images in the runtime's store. The default reports `Unsupported`.
    fn list_images(&self) -> BoxFuture<'_, Result<Vec<ImageInfo>, Error>> {
        Box::pin(async { Err(Error::Unsupported("listing images")) })
    }
    /// Removes an image by untagging all its names (the image goes with the last one) and never
    /// forces: an image a container uses stays and the call fails. The default reports
    /// `Unsupported`.
    ///
    /// # Arguments
    /// * `image` - the image to remove
    fn remove_image(&self, _image: &ImageInfo) -> BoxFuture<'_, Result<(), Error>> {
        Box::pin(async { Err(Error::Unsupported("removing images")) })
    }
    /// Names of the images containers were created from in the time window, read from the
    /// runtime's event log. The default reports `Unsupported`.
    ///
    /// # Arguments
    /// * `since` - window start, seconds since the Unix epoch
    /// * `until` - window end, seconds since the Unix epoch
    fn images_used_between(
        &self,
        _since: u64,
        _until: u64,
    ) -> BoxFuture<'_, Result<Vec<String>, Error>> {
        Box::pin(async { Err(Error::Unsupported("reading container events")) })
    }
}

/// Runtime driven through its command line client (portable, also works inside WSL).
#[derive(Debug, Clone)]
pub struct CliRuntime {
    kind: RuntimeKind,
    binary: PathBuf,
    timeout: Duration,
}

impl CliRuntime {
    /// Uses the binary named after the runtime, resolved through `PATH`.
    pub fn new(kind: RuntimeKind) -> Self {
        Self::with_binary(kind, kind.binary_name())
    }

    pub fn with_binary(kind: RuntimeKind, binary: impl Into<PathBuf>) -> Self {
        Self {
            kind,
            binary: binary.into(),
            timeout: DEFAULT_TIMEOUT,
        }
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    pub(super) fn binary_display(&self) -> String {
        self.binary.display().to_string()
    }

    /// Runs the binary with `args` and returns its output if it exits successfully.
    pub(super) async fn run_checked<S: AsRef<std::ffi::OsStr>>(
        &self,
        args: &[S],
        timeout: Duration,
    ) -> Result<std::process::Output, Error> {
        let binary = self.binary_display();
        let output = Command::new(&self.binary)
            .args(args)
            .stdin(Stdio::null())
            .kill_on_drop(true)
            .output();

        let output = match tokio::time::timeout(timeout, output).await {
            Err(_) => return Err(Error::Timeout { binary, timeout }),
            Ok(Err(source)) if source.kind() == io::ErrorKind::NotFound => {
                return Err(Error::NotFound(binary));
            }
            Ok(Err(source)) => return Err(Error::Spawn { binary, source }),
            Ok(Ok(output)) => output,
        };

        if !output.status.success() {
            return Err(Error::Failed {
                binary,
                status: output.status.to_string(),
                stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
            });
        }
        Ok(output)
    }

    async fn query(&self) -> Result<RuntimeInfo, Error> {
        let binary = self.binary_display();
        let args: &[&str] = match self.kind {
            RuntimeKind::Podman => &["info", "--format", "json"],
            RuntimeKind::Docker => &["info", "--format", "{{json .}}"],
            // `info` and a machine readable format are not documented for wslc, `version` is
            RuntimeKind::Wslc => &["version"],
        };

        let output = self.run_checked(args, self.timeout).await?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let parsed = match self.kind {
            RuntimeKind::Podman => parse_podman_info(&stdout),
            RuntimeKind::Docker => parse_docker_info(&stdout),
            RuntimeKind::Wslc => {
                return Ok(RuntimeInfo {
                    kind: RuntimeKind::Wslc,
                    version: parse_wslc_version(&stdout),
                    rootless: None,
                    storage_root: None,
                    socket: None,
                });
            }
        };
        parsed.map_err(|source| Error::Parse { binary, source })
    }
}

impl ContainerRuntime for CliRuntime {
    fn kind(&self) -> RuntimeKind {
        self.kind
    }

    fn binary(&self) -> &Path {
        &self.binary
    }

    fn info(&self) -> BoxFuture<'_, Result<RuntimeInfo, Error>> {
        Box::pin(self.query())
    }

    fn probe_container(&self, args: Vec<String>) -> BoxFuture<'_, Result<(), Error>> {
        Box::pin(async move {
            let mut full = vec!["run".to_string(), "--rm".to_string()];
            full.extend(args);
            self.run_checked(&full, PROBE_TIMEOUT).await.map(drop)
        })
    }

    fn list_images(&self) -> BoxFuture<'_, Result<Vec<ImageInfo>, Error>> {
        Box::pin(self.list_images_cli())
    }

    fn remove_image(&self, image: &ImageInfo) -> BoxFuture<'_, Result<(), Error>> {
        Box::pin(self.remove_image_cli(image.clone()))
    }

    fn images_used_between(
        &self,
        since: u64,
        until: u64,
    ) -> BoxFuture<'_, Result<Vec<String>, Error>> {
        Box::pin(self.images_used_between_cli(since, until))
    }
}

fn string_at(value: &Value, pointer: &str) -> Option<String> {
    value
        .pointer(pointer)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

pub(crate) fn parse_podman_info(json: &str) -> Result<RuntimeInfo, serde_json::Error> {
    let value: Value = serde_json::from_str(json)?;
    Ok(RuntimeInfo {
        kind: RuntimeKind::Podman,
        version: string_at(&value, "/version/Version").unwrap_or_default(),
        rootless: value
            .pointer("/host/security/rootless")
            .and_then(Value::as_bool),
        storage_root: string_at(&value, "/store/graphRoot").map(PathBuf::from),
        socket: string_at(&value, "/host/remoteSocket/path")
            .map(|path| PathBuf::from(path.strip_prefix("unix://").unwrap_or(&path))),
    })
}

pub(crate) fn parse_docker_info(json: &str) -> Result<RuntimeInfo, serde_json::Error> {
    let value: Value = serde_json::from_str(json)?;
    let rootless = value
        .pointer("/SecurityOptions")
        .and_then(Value::as_array)
        .map(|options| {
            options
                .iter()
                .filter_map(Value::as_str)
                .any(|option| option.split(',').any(|part| part == "name=rootless"))
        });
    Ok(RuntimeInfo {
        kind: RuntimeKind::Docker,
        version: string_at(&value, "/ServerVersion").unwrap_or_default(),
        rootless,
        storage_root: string_at(&value, "/DockerRootDir").map(PathBuf::from),
        socket: None,
    })
}

/// The output format of `wslc version` is not documented, so take the first token that looks
/// like a dotted version number and fall back to the first line.
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn parse_wslc_version(output: &str) -> String {
    output
        .split_whitespace()
        .map(|token| token.trim_matches(|c: char| !c.is_ascii_alphanumeric()))
        .find(|token| token.starts_with(|c: char| c.is_ascii_digit()) && token.contains('.'))
        .or_else(|| output.lines().next())
        .unwrap_or_default()
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_rootless_podman() {
        let info = parse_podman_info(
            r#"{"host":{"security":{"rootless":true},"remoteSocket":{"path":"/run/user/1000/podman/podman.sock"}},
                "store":{"graphRoot":"/home/u/.local/share/containers/storage"},
                "version":{"Version":"5.2.1"}}"#,
        )
        .unwrap();
        assert_eq!(info.kind, RuntimeKind::Podman);
        assert_eq!(info.version, "5.2.1");
        assert_eq!(info.rootless, Some(true));
        assert_eq!(
            info.socket.as_deref(),
            Some(Path::new("/run/user/1000/podman/podman.sock"))
        );
        assert!(info.storage_root.is_some());
    }

    #[test]
    fn podman_socket_scheme_is_stripped() {
        let info = parse_podman_info(
            r#"{"host":{"remoteSocket":{"path":"unix:///run/user/501/podman/podman.sock"}}}"#,
        )
        .unwrap();
        assert_eq!(
            info.socket.as_deref(),
            Some(Path::new("/run/user/501/podman/podman.sock"))
        );
    }

    #[test]
    fn engine_access_args_per_runtime() {
        let mut info = parse_podman_info(
            r#"{"host":{"remoteSocket":{"path":"/run/user/1000/podman/podman.sock"}}}"#,
        )
        .unwrap();
        assert_eq!(
            info.engine_access_args().unwrap(),
            [
                "-v",
                "/run/user/1000/podman/podman.sock:/run/container.sock",
                "-e",
                "DOCKER_HOST=unix:///run/container.sock",
                "-e",
                "CONTAINER_HOST=unix:///run/container.sock",
                "--security-opt",
                "label=disable",
                "--userns=keep-id"
            ]
        );
        info.socket = None;
        assert!(info.engine_access_args().is_none());

        let docker = parse_docker_info(r#"{"ServerVersion":"27"}"#).unwrap();
        assert_eq!(
            docker.engine_access_args().unwrap()[1],
            "/var/run/docker.sock:/run/container.sock"
        );
        // The engine runs as the worker's user and needs the socket's group
        assert!(
            docker
                .engine_access_args()
                .unwrap()
                .windows(2)
                .any(|pair| pair[0] == "--group-add")
        );
    }

    #[test]
    fn parses_rootful_podman() {
        let info = parse_podman_info(
            r#"{"host":{"security":{"rootless":false}},"version":{"Version":"4.9.3"}}"#,
        )
        .unwrap();
        assert_eq!(info.rootless, Some(false));
        assert!(info.socket.is_none());
    }

    #[test]
    fn parses_rootless_docker() {
        let info = parse_docker_info(
            r#"{"ServerVersion":"27.1.1","DockerRootDir":"/home/u/.local/share/docker",
                "SecurityOptions":["name=seccomp,profile=builtin","name=rootless"]}"#,
        )
        .unwrap();
        assert_eq!(info.kind, RuntimeKind::Docker);
        assert_eq!(info.version, "27.1.1");
        assert_eq!(info.rootless, Some(true));
    }

    #[test]
    fn parses_rootful_docker() {
        let info = parse_docker_info(
            r#"{"ServerVersion":"26.0.0","SecurityOptions":["name=seccomp,profile=builtin"]}"#,
        )
        .unwrap();
        assert_eq!(info.rootless, Some(false));
    }

    #[test]
    fn parses_wslc_version_leniently() {
        assert_eq!(
            parse_wslc_version("wslc version: 2.9.4.0\nWSL: 2.9.4\n"),
            "2.9.4.0"
        );
        assert_eq!(parse_wslc_version("preview build\n"), "preview build");
        assert_eq!(parse_wslc_version(""), "");
    }

    #[test]
    fn wslc_only_runs_container_engine() {
        use strum::IntoEnumIterator;
        for engine in Engine::iter() {
            assert_eq!(
                RuntimeKind::Wslc.supports_engine(&engine),
                engine == Engine::Container
            );
        }
    }

    #[test]
    fn snakemake_runs_on_docker_and_on_podman_for_linux_hosts() {
        use strum::IntoEnumIterator;
        for engine in Engine::iter() {
            assert!(RuntimeKind::Docker.supports_engine(&engine));
            assert_eq!(
                RuntimeKind::Podman.supports_engine(&engine),
                engine != Engine::Snakemake || RuntimeKind::podman_runs_snakemake()
            );
        }
    }

    #[test]
    fn rejects_garbage() {
        assert!(parse_podman_info("not json").is_err());
    }

    #[tokio::test]
    async fn missing_binary_is_not_found() {
        let runtime = CliRuntime::with_binary(RuntimeKind::Podman, "/nonexistent/macworp-podman");
        assert!(matches!(runtime.info().await, Err(Error::NotFound(_))));
    }

    #[cfg(unix)]
    pub(crate) mod fake {
        use std::os::unix::fs::PermissionsExt;

        use super::*;

        pub fn script(dir: &Path, body: &str) -> PathBuf {
            let path = dir.join("fake-runtime");
            std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
            path
        }
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn fake_binary_success_failure_and_timeout() {
        let dir = tempfile::tempdir().unwrap();

        let ok = fake::script(
            dir.path(),
            r#"echo '{"host":{"security":{"rootless":true}},"version":{"Version":"5.0.0"}}'"#,
        );
        let info = CliRuntime::with_binary(RuntimeKind::Podman, ok)
            .info()
            .await
            .unwrap();
        assert_eq!(info.version, "5.0.0");

        let failing = fake::script(dir.path(), "echo 'cannot connect' >&2; exit 1");
        match CliRuntime::with_binary(RuntimeKind::Docker, failing)
            .info()
            .await
        {
            Err(Error::Failed { stderr, .. }) => assert_eq!(stderr, "cannot connect"),
            other => panic!("unexpected: {other:?}"),
        }

        let hanging = fake::script(dir.path(), "exec sleep 30");
        let runtime = CliRuntime::with_binary(RuntimeKind::Docker, hanging)
            .with_timeout(Duration::from_millis(200));
        assert!(matches!(runtime.info().await, Err(Error::Timeout { .. })));
    }
}
