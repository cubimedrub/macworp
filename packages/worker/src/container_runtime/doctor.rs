use std::{fmt, path::Path, sync::Arc};

use clap::ValueEnum;
use macworp_core::workflow_configuration::engine::Engine;
use strum::IntoEnumIterator;
use thiserror::Error;

use crate::selinux::DirectoryLabel;

use super::runtime::{
    CliRuntime, ContainerRuntime, Error as RuntimeError, RuntimeInfo, RuntimeKind,
};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, ValueEnum)]
pub enum RuntimePreference {
    /// Every usable runtime (podman, docker, on Windows wslc); each engine runs on the first one that supports it
    #[default]
    Auto,
    Podman,
    Docker,
    /// WSL Containers, can only run `Container` workflows
    #[cfg(windows)]
    Wslc,
}

impl RuntimePreference {
    fn candidates(self) -> Vec<Arc<dyn ContainerRuntime>> {
        let kinds: &[RuntimeKind] = match self {
            #[cfg(windows)]
            Self::Auto => &[RuntimeKind::Podman, RuntimeKind::Docker, RuntimeKind::Wslc],
            #[cfg(not(windows))]
            Self::Auto => &[RuntimeKind::Podman, RuntimeKind::Docker],
            Self::Podman => &[RuntimeKind::Podman],
            Self::Docker => &[RuntimeKind::Docker],
            #[cfg(windows)]
            Self::Wslc => &[RuntimeKind::Wslc],
        };
        kinds
            .iter()
            .map(|kind| Arc::new(CliRuntime::new(*kind)) as Arc<dyn ContainerRuntime>)
            .collect()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckStatus {
    Ok,
    Warn,
    Fail,
}

#[derive(Debug, Clone)]
pub struct Check {
    pub name: String,
    pub status: CheckStatus,
    pub message: String,
}

#[derive(Debug, Error)]
#[error("{0}")]
pub struct Error(String);

/// A container runtime that passed its checks, with what it reported about itself.
#[derive(Debug, Clone)]
pub struct UsableRuntime {
    /// The runtime, ready to start containers
    pub runtime: Arc<dyn ContainerRuntime>,
    /// Version, rootless state and socket as the runtime reported them
    pub info: RuntimeInfo,
}

/// Result of probing the host for usable container runtimes.
#[derive(Debug)]
pub struct DoctorReport {
    /// Everything the probe found, one line per check
    pub checks: Vec<Check>,
    /// Usable runtimes in preference order (podman, docker, wslc)
    pub runtimes: Vec<UsableRuntime>,
}

impl DoctorReport {
    /// Adds the `selinux-labels` check for a directory MAcWorP keeps its data in (the storage
    /// root): on an enforcing host, containers the engine starts itself need it labelled
    /// `container_file_t`. Nothing is added when SELinux does not enforce or no runtime is usable.
    ///
    /// # Arguments
    /// * `directory` - The directory to check
    pub fn check_label_of(&mut self, directory: &Path) {
        let label = DirectoryLabel::of(directory);
        let current = label.current();
        let check = self.label_check(crate::executors::selinux_enforcing(), directory, current);
        self.checks.extend(check);
    }

    /// The `selinux-labels` check, see [`Self::check_label_of`].
    ///
    /// # Arguments
    /// * `enforcing` - see [`crate::executors::selinux_enforcing`]
    /// * `directory` - The directory checked
    /// * `current` - Its label, `None` if unreadable
    fn label_check(
        &self,
        enforcing: bool,
        directory: &Path,
        current: Option<String>,
    ) -> Option<Check> {
        if !enforcing || self.runtimes.is_empty() {
            return None;
        }
        let name = "selinux-labels".to_string();
        let shown = directory.display();
        Some(match current {
            Some(label) if DirectoryLabel::allows_containers(&label) => Check {
                name,
                status: CheckStatus::Ok,
                message: format!("`{shown}` is labelled {label}"),
            },
            Some(label) => Check {
                name,
                status: CheckStatus::Warn,
                message: format!(
                    "`{shown}` is labelled {label}: containers started by the engine itself (CWL tool containers, Nextflow tasks) cannot use their bind mounts. Label the directory for containers (nothing else is changed): {}",
                    DirectoryLabel::of(directory).admin_command()
                ),
            },
            None => Check {
                name,
                status: CheckStatus::Warn,
                message: format!("the SELinux label of `{shown}` cannot be read"),
            },
        })
    }

    /// Notice that engine containers run without SELinux confinement, `None` when SELinux does
    /// not enforce or no runtime is usable. Without the option every bind mount would be denied.
    ///
    /// # Arguments
    /// * `enforcing` - see [`crate::executors::selinux_enforcing`]
    fn selinux_check(&self, enforcing: bool) -> Option<Check> {
        if !enforcing || self.runtimes.is_empty() {
            return None;
        }
        Some(Check {
            name: "selinux".to_string(),
            status: CheckStatus::Warn,
            message: "SELinux is enforcing: engine containers run with `--security-opt label=disable` (their bind mounts would be denied otherwise); other containers and the host stay confined. Containers the engine itself starts (CWL tool containers, Nextflow tasks) need the data folders labelled `container_file_t`, see the `selinux-labels` check".to_string(),
        })
    }

    /// At least one runtime is usable. Warnings do not fail.
    pub fn is_ok(&self) -> bool {
        !self.runtimes.is_empty()
    }

    /// The usable runtime that runs `engine`: the first in preference order that supports it.
    pub fn runtime_for(&self, engine: &Engine) -> Option<&UsableRuntime> {
        self.runtimes
            .iter()
            .find(|usable| usable.runtime.supports_engine(engine))
    }

    /// Engines no usable runtime can run (all of them if there is no runtime).
    pub fn unavailable_engines(&self) -> Vec<Engine> {
        Engine::iter()
            .filter(|engine| self.runtime_for(engine).is_none())
            .collect()
    }

    /// The usable runtimes in preference order, or the report as error if there is none.
    pub fn into_runtimes(self) -> Result<Vec<Arc<dyn ContainerRuntime>>, Error> {
        if self.is_ok() {
            Ok(self
                .runtimes
                .into_iter()
                .map(|usable| usable.runtime)
                .collect())
        } else {
            Err(Error(self.to_string()))
        }
    }
}

impl fmt::Display for DoctorReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, check) in self.checks.iter().enumerate() {
            if i > 0 {
                writeln!(f)?;
            }
            let tag = match check.status {
                CheckStatus::Ok => "ok",
                CheckStatus::Warn => "warn",
                CheckStatus::Fail => "FAIL",
            };
            write!(f, "[{tag}] {}: {}", check.name, check.message)?;
        }
        Ok(())
    }
}

/// Minimum supported (major, minor) per runtime. wslc is unchecked: its version scheme is
/// not documented and it ships with WSL.
fn min_version(kind: RuntimeKind) -> Option<(u64, u64)> {
    match kind {
        RuntimeKind::Podman => Some((4, 0)),
        RuntimeKind::Docker => Some((20, 10)),
        #[cfg(windows)]
        RuntimeKind::Wslc => None,
    }
}

fn parse_version(version: &str) -> Option<(u64, u64)> {
    let mut parts = version.trim_start_matches('v').split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.split(['-', '+']).next()?.parse().ok()?;
    Some((major, minor))
}

fn hint(kind: RuntimeKind, error: &RuntimeError) -> String {
    match (error, kind) {
        #[cfg(windows)]
        (RuntimeError::NotFound(_), RuntimeKind::Wslc) => {
            format!("{error}; install or update WSL (`wsl --update`, needs WSL 2.9.3 or newer)")
        }
        (RuntimeError::NotFound(_), _) => {
            format!("{error}; install {kind} (podman preferred, rootless)")
        }
        (_, RuntimeKind::Podman) => format!(
            "{error}; check that podman works for your user (`podman info`), on macOS/Windows run `podman machine start`"
        ),
        (_, RuntimeKind::Docker) => {
            format!(
                "{error}; check that the docker daemon is running and reachable (`docker info`)"
            )
        }
        #[cfg(windows)]
        (_, RuntimeKind::Wslc) => {
            format!(
                "{error}; check that WSL containers work (`wsl --update`, needs WSL 2.9.3 or newer)"
            )
        }
    }
}

/// Probes the host for a usable runtime according to `preference`.
pub async fn doctor(preference: RuntimePreference) -> DoctorReport {
    doctor_with(preference.candidates()).await
}

/// Like [`doctor`] with explicit candidates, in order of preference. Every candidate is
/// probed; all usable ones are kept.
pub async fn doctor_with(candidates: Vec<Arc<dyn ContainerRuntime>>) -> DoctorReport {
    let mut checks = Vec::new();
    let mut runtimes = Vec::new();
    // Failed candidates only warn as long as another one works, decided at the end.
    let mut failed_indices = Vec::new();

    for runtime in candidates {
        let kind = runtime.kind();
        match runtime.info().await {
            Err(error) => {
                failed_indices.push(checks.len());
                checks.push(Check {
                    name: kind.to_string(),
                    status: CheckStatus::Fail,
                    message: hint(kind, &error),
                });
            }
            Ok(info) => {
                let own = checks_for(&info);
                if own.iter().any(|check| check.status == CheckStatus::Fail) {
                    failed_indices.extend(checks.len()..checks.len() + own.len());
                } else {
                    runtimes.push(UsableRuntime { runtime, info });
                }
                checks.extend(own);
            }
        }
    }

    if checks.is_empty() {
        checks.push(Check {
            name: "runtime".to_string(),
            status: CheckStatus::Fail,
            message: "no container runtime candidate configured".to_string(),
        });
    }
    if !runtimes.is_empty() {
        for index in failed_indices {
            if checks[index].status == CheckStatus::Fail {
                checks[index].status = CheckStatus::Warn;
            }
        }
    }

    let mut report = DoctorReport { checks, runtimes };
    let engines = report.engines_check();
    let apparmor = report
        .apparmor_check(
            host_restricts_userns(),
            crate::executors::snakemake::apparmor_profile().as_deref(),
            &crate::executors::snakemake::image(),
        )
        .await;
    let selinux = report.selinux_check(crate::executors::selinux_enforcing());
    report
        .checks
        .extend([engines, apparmor, selinux].into_iter().flatten());
    report
}

/// Whether the host restricts unprivileged user namespaces through AppArmor (Ubuntu 24.04+),
/// `None` where the setting does not exist (other kernels, other platforms).
fn host_restricts_userns() -> Option<bool> {
    let value =
        std::fs::read_to_string("/proc/sys/kernel/apparmor_restrict_unprivileged_userns").ok()?;
    Some(value.trim() == "1")
}

impl DoctorReport {
    /// The Docker runtime that runs Snakemake, `None` if another runtime does (the AppArmor
    /// profile only concerns Docker).
    fn docker_for_snakemake(&self) -> Option<&UsableRuntime> {
        self.runtime_for(&Engine::Snakemake)
            .filter(|usable| usable.info.kind == RuntimeKind::Docker)
    }

    /// AppArmor check for Snakemake on Docker: warns when the host restricts user namespaces and
    /// no profile is configured, and when a configured profile is not loaded (found by starting
    /// a throwaway container with it). `None` if nothing is wrong or nothing applies.
    ///
    /// # Arguments
    /// * `restricts_userns` - see [`host_restricts_userns`]
    /// * `profile` - the configured profile, see `snakemake::apparmor_profile`
    /// * `image` - engine image the probe container starts from
    async fn apparmor_check(
        &self,
        restricts_userns: Option<bool>,
        profile: Option<&str>,
        image: &str,
    ) -> Option<Check> {
        let docker = self.docker_for_snakemake()?;
        match profile {
            None => Self::apparmor_missing_check(restricts_userns),
            Some(profile) => Some(Self::apparmor_probe(&*docker.runtime, profile, image).await),
        }
    }

    /// Warning for a restricted host without a configured profile, Apptainer in the engine
    /// container then fails with `Failed to set mount propagation`.
    ///
    /// # Arguments
    /// * `restricts_userns` - see [`host_restricts_userns`]
    fn apparmor_missing_check(restricts_userns: Option<bool>) -> Option<Check> {
        if restricts_userns != Some(true) {
            return None;
        }
        Some(Check {
            name: "apparmor".to_string(),
            status: CheckStatus::Warn,
            message: format!(
                "this host restricts unprivileged user namespaces and Snakemake runs on docker: load docker/apparmor-macworp-snakemake (`sudo apparmor_parser -r -W docker/apparmor-macworp-snakemake`) and set {}=macworp-snakemake, or Snakemake container rules fail",
                crate::executors::snakemake::APPARMOR_PROFILE_VARIABLE
            ),
        })
    }

    /// Starts a container under `profile` to see whether Docker accepts it (it refuses a profile
    /// that is not loaded). Needs the engine image locally: without it nothing is verified.
    ///
    /// # Arguments
    /// * `runtime` - the Docker runtime
    /// * `profile` - name of the configured AppArmor profile
    /// * `image` - engine image the probe container starts from
    async fn apparmor_probe(runtime: &dyn ContainerRuntime, profile: &str, image: &str) -> Check {
        let name = "apparmor".to_string();
        let args = [
            "--pull",
            "never",
            "--security-opt",
            &format!("apparmor={profile}"),
            "--entrypoint",
            "true",
            image,
        ]
        .map(String::from)
        .to_vec();
        match runtime.probe_container(args).await {
            Ok(()) => Check {
                name,
                status: CheckStatus::Ok,
                message: format!("AppArmor profile `{profile}` is loaded"),
            },
            Err(RuntimeError::Failed { stderr, .. })
                if stderr.contains("No such image") || stderr.contains("Unable to find image") =>
            {
                Check {
                    name,
                    status: CheckStatus::Ok,
                    message: format!(
                        "AppArmor profile `{profile}` not verified, engine image `{image}` is not available locally"
                    ),
                }
            }
            Err(error) => Check {
                name,
                status: CheckStatus::Warn,
                message: format!(
                    "docker cannot start a container with AppArmor profile `{profile}`, Snakemake container rules will fail: {error}; load it with `sudo apparmor_parser -r -W docker/apparmor-macworp-snakemake`"
                ),
            },
        }
    }

    /// Warning naming the engines no usable runtime can run, `None` if all can run.
    fn engines_check(&self) -> Option<Check> {
        if !self.is_ok() {
            return None;
        }
        let unavailable: Vec<String> = self
            .unavailable_engines()
            .iter()
            .map(|engine| engine.to_string())
            .collect();
        if unavailable.is_empty() {
            return None;
        }
        let kinds: Vec<String> = self
            .runtimes
            .iter()
            .map(|usable| usable.info.kind.to_string())
            .collect();
        let hint = if kinds.iter().any(|kind| kind == "podman") {
            "install docker"
        } else {
            "install podman or docker"
        };
        Some(Check {
            name: "engines".to_string(),
            status: CheckStatus::Warn,
            message: format!(
                "{} cannot run {} workflows, {hint} for those",
                kinds.join(" and "),
                unavailable.join(", ")
            ),
        })
    }
}

fn checks_for(info: &RuntimeInfo) -> Vec<Check> {
    let kind = info.kind;
    let mut checks = vec![Check {
        name: kind.to_string(),
        status: CheckStatus::Ok,
        message: format!("reachable, version {}", info.version),
    }];

    if let Some((min_major, min_minor)) = min_version(kind) {
        checks.push(match parse_version(&info.version) {
            Some(version) if version >= (min_major, min_minor) => Check {
                name: "version".to_string(),
                status: CheckStatus::Ok,
                message: format!("{} >= {min_major}.{min_minor}", info.version),
            },
            Some(_) => Check {
                name: "version".to_string(),
                status: CheckStatus::Fail,
                message: format!(
                    "{kind} {} is too old, need >= {min_major}.{min_minor}",
                    info.version
                ),
            },
            None => Check {
                name: "version".to_string(),
                status: CheckStatus::Warn,
                message: format!("unable to parse version `{}`", info.version),
            },
        });
    }

    checks.push(match info.rootless {
        Some(true) => Check {
            name: "rootless".to_string(),
            status: CheckStatus::Ok,
            message: "runtime runs rootless".to_string(),
        },
        Some(false) => Check {
            name: "rootless".to_string(),
            status: CheckStatus::Warn,
            message: format!(
                "{kind} runs as root; mounting its socket into engine containers is root-equivalent, prefer rootless podman/docker"
            ),
        },
        None => Check {
            name: "rootless".to_string(),
            status: CheckStatus::Ok,
            message: format!("{kind} does not report whether it runs rootless"),
        },
    });

    checks
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::*;
    use crate::container_runtime::runtime::BoxFuture;

    #[derive(Debug)]
    struct Mock {
        kind: RuntimeKind,
        result: Result<RuntimeInfo, String>,
        /// Outcome of `probe_container`, error = stderr
        probe: Result<(), String>,
    }

    impl Mock {
        fn ok(kind: RuntimeKind, version: &str, rootless: bool) -> Arc<dyn ContainerRuntime> {
            Arc::new(Self {
                kind,
                result: Ok(RuntimeInfo {
                    kind,
                    version: version.to_string(),
                    rootless: Some(rootless),
                    storage_root: None,
                    socket: None,
                }),
                probe: Ok(()),
            })
        }

        fn down(kind: RuntimeKind) -> Arc<dyn ContainerRuntime> {
            Arc::new(Self {
                kind,
                result: Err("daemon unreachable".to_string()),
                probe: Ok(()),
            })
        }
    }

    impl ContainerRuntime for Mock {
        fn kind(&self) -> RuntimeKind {
            self.kind
        }
        fn binary(&self) -> &Path {
            Path::new("mock")
        }
        fn info(&self) -> BoxFuture<'_, Result<RuntimeInfo, RuntimeError>> {
            let result = match &self.result {
                Ok(info) => Ok(info.clone()),
                Err(stderr) => Err(RuntimeError::Failed {
                    binary: PathBuf::from("mock").display().to_string(),
                    status: "exit status: 1".to_string(),
                    stderr: stderr.clone(),
                }),
            };
            Box::pin(async move { result })
        }
        fn probe_container(&self, _args: Vec<String>) -> BoxFuture<'_, Result<(), RuntimeError>> {
            let result = self.probe.clone().map_err(|stderr| RuntimeError::Failed {
                binary: "mock".to_string(),
                status: "exit status: 125".to_string(),
                stderr,
            });
            Box::pin(async move { result })
        }
    }

    #[test]
    fn version_parsing() {
        assert_eq!(parse_version("5.2.1"), Some((5, 2)));
        assert_eq!(parse_version("v4.9.3-dev"), Some((4, 9)));
        assert_eq!(parse_version("27.1.1"), Some((27, 1)));
        assert_eq!(parse_version("dev"), None);
    }

    #[tokio::test]
    async fn rootless_podman_is_ok() {
        let report = doctor_with(vec![Mock::ok(RuntimeKind::Podman, "5.2.1", true)]).await;
        assert!(report.is_ok());
        // only the engines check warns (Snakemake is Docker-only), and the notice of an enforcing
        // SELinux host
        assert!(
            report.checks.iter().all(|c| c.status == CheckStatus::Ok
                || ["engines", "selinux"].contains(&c.name.as_str()))
        );
        assert!(report.into_runtimes().is_ok());
    }

    /// The SELinux notice shows only when SELinux enforces and a runtime is usable.
    #[tokio::test]
    async fn selinux_notice_needs_enforcing_and_a_runtime() {
        let report = doctor_with(vec![Mock::ok(RuntimeKind::Docker, "27.1.1", true)]).await;
        let notice = report.selinux_check(true).unwrap();
        assert_eq!(notice.status, CheckStatus::Warn);
        assert!(notice.message.contains("label=disable"));
        assert!(report.selinux_check(false).is_none());
        let none = doctor_with(vec![]).await;
        assert!(none.selinux_check(true).is_none());
    }

    /// The label check passes for `container_file_t`, warns with the administrator command for
    /// any other or unreadable label and is silent without enforcement.
    #[tokio::test]
    async fn directory_label_check_needs_container_type() {
        let report = doctor_with(vec![Mock::ok(RuntimeKind::Docker, "27.1.1", true)]).await;
        let dir = Path::new("/srv/macworp");
        let check = |enforcing, label: Option<&str>| {
            report.label_check(enforcing, dir, label.map(str::to_string))
        };
        let ok = check(true, Some("system_u:object_r:container_file_t:s0")).unwrap();
        assert_eq!(ok.status, CheckStatus::Ok);
        let home = check(true, Some("unconfined_u:object_r:user_home_t:s0")).unwrap();
        assert_eq!(home.status, CheckStatus::Warn);
        assert!(home.message.contains("semanage fcontext"));
        assert_eq!(check(true, None).unwrap().status, CheckStatus::Warn);
        assert!(check(false, None).is_none());
        let none = doctor_with(vec![]).await;
        assert!(none.label_check(true, dir, None).is_none());
    }

    #[tokio::test]
    async fn rootful_docker_warns_but_passes() {
        let report = doctor_with(vec![Mock::ok(RuntimeKind::Docker, "27.1.1", false)]).await;
        assert!(report.is_ok());
        assert!(report.checks.iter().any(|c| c.status == CheckStatus::Warn));
    }

    #[tokio::test]
    async fn apparmor_warning_needs_docker_restriction_and_no_profile() {
        let docker = doctor_with(vec![Mock::ok(RuntimeKind::Docker, "27.1.1", true)]).await;
        let warns = async |report: &DoctorReport, restricted, profile| {
            report
                .apparmor_check(restricted, profile, "img")
                .await
                .is_some_and(|check| check.status == CheckStatus::Warn)
        };
        assert!(warns(&docker, Some(true), None).await);
        assert!(!warns(&docker, Some(true), Some("macworp-snakemake")).await);
        assert!(!warns(&docker, Some(false), None).await);
        assert!(!warns(&docker, None, None).await);
        // Podman runs Snakemake on Linux hosts, so Docker is not the Snakemake runtime there
        let both = doctor_with(vec![
            Mock::ok(RuntimeKind::Podman, "5.2.1", true),
            Mock::ok(RuntimeKind::Docker, "27.1.1", true),
        ])
        .await;
        assert_eq!(
            warns(&both, Some(true), None).await,
            !cfg!(target_os = "linux")
        );
    }

    /// A configured profile is probed with a container: loaded, not loaded, image missing.
    #[tokio::test]
    async fn apparmor_probe_reports_unloaded_profiles() {
        let probe = async |result: Result<(), &str>| {
            let docker = Arc::new(Mock {
                kind: RuntimeKind::Docker,
                result: Ok(RuntimeInfo {
                    kind: RuntimeKind::Docker,
                    version: "27.1.1".into(),
                    rootless: Some(true),
                    storage_root: None,
                    socket: None,
                }),
                probe: result.map_err(String::from),
            });
            DoctorReport::apparmor_probe(&*docker, "macworp-snakemake", "img").await
        };
        assert_eq!(probe(Ok(())).await.status, CheckStatus::Ok);
        let unloaded = probe(Err("apparmor profile not found")).await;
        assert_eq!(unloaded.status, CheckStatus::Warn);
        assert!(unloaded.message.contains("apparmor_parser"));
        let no_image = probe(Err("Unable to find image 'img' locally")).await;
        assert_eq!(no_image.status, CheckStatus::Ok);
        assert!(no_image.message.contains("not verified"));
    }

    #[tokio::test]
    async fn old_version_fails() {
        let report = doctor_with(vec![Mock::ok(RuntimeKind::Podman, "3.4.4", true)]).await;
        assert!(!report.is_ok());
        assert!(report.into_runtimes().is_err());
    }

    #[tokio::test]
    async fn falls_back_to_second_candidate() {
        let report = doctor_with(vec![
            Mock::down(RuntimeKind::Podman),
            Mock::ok(RuntimeKind::Docker, "27.1.1", true),
        ])
        .await;
        assert!(report.is_ok());
        assert_eq!(report.runtimes.len(), 1);
        assert_eq!(report.runtimes[0].info.kind, RuntimeKind::Docker);
        assert_eq!(report.checks[0].status, CheckStatus::Warn);
        assert!(report.unavailable_engines().is_empty());
    }

    #[tokio::test]
    async fn podman_and_docker_together_run_every_engine() {
        let report = doctor_with(vec![
            Mock::ok(RuntimeKind::Podman, "5.2.1", true),
            Mock::ok(RuntimeKind::Docker, "27.1.1", true),
        ])
        .await;
        assert!(report.is_ok());
        assert!(report.unavailable_engines().is_empty());
        assert_eq!(
            report.runtime_for(&Engine::Nextflow).unwrap().info.kind,
            RuntimeKind::Podman
        );
        assert_eq!(
            report.runtime_for(&Engine::Snakemake).unwrap().info.kind,
            // the first runtime supporting the engine wins, Podman does on Linux hosts
            if cfg!(target_os = "linux") {
                RuntimeKind::Podman
            } else {
                RuntimeKind::Docker
            }
        );
        assert!(report.checks.iter().all(|c| c.name != "engines"));
        assert_eq!(report.into_runtimes().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn too_old_candidate_only_warns_next_to_a_working_one() {
        let report = doctor_with(vec![
            Mock::ok(RuntimeKind::Podman, "3.4.4", true),
            Mock::ok(RuntimeKind::Docker, "27.1.1", true),
        ])
        .await;
        assert!(report.is_ok());
        assert_eq!(report.runtimes.len(), 1);
        assert!(report.checks.iter().all(|c| c.status != CheckStatus::Fail));
        assert!(
            report
                .checks
                .iter()
                .any(|c| c.name == "version" && c.status == CheckStatus::Warn)
        );
    }

    #[tokio::test]
    async fn nothing_usable_fails_with_hints() {
        let report = doctor_with(vec![
            Mock::down(RuntimeKind::Podman),
            Mock::down(RuntimeKind::Docker),
        ])
        .await;
        assert!(!report.is_ok());
        let text = report.to_string();
        assert!(text.contains("podman machine start"));
        assert!(text.contains("docker daemon"));
        assert!(report.into_runtimes().is_err());
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn wslc_only_limits_engines() {
        let report = doctor_with(vec![Mock::ok(RuntimeKind::Wslc, "2.9.4.0", false)]).await;
        // wslc reports no rootless state in reality, the mock only needs a usable runtime
        assert!(report.is_ok());
        assert_eq!(
            report.unavailable_engines(),
            vec![Engine::Nextflow, Engine::Snakemake, Engine::Cwl]
        );
        assert!(
            report
                .to_string()
                .contains("cannot run Nextflow, Snakemake, CWL")
        );
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn podman_only_loses_snakemake_and_wslc_loses_to_it() {
        let report = doctor_with(vec![
            Mock::ok(RuntimeKind::Podman, "5.2.1", true),
            Mock::ok(RuntimeKind::Wslc, "2.9.4.0", true),
        ])
        .await;
        assert_eq!(report.unavailable_engines(), vec![Engine::Snakemake]);
    }

    #[tokio::test]
    async fn podman_runs_snakemake_on_linux_hosts_only_docker_everywhere() {
        let podman = doctor_with(vec![Mock::ok(RuntimeKind::Podman, "5.2.1", true)]).await;
        if cfg!(target_os = "linux") {
            assert!(podman.unavailable_engines().is_empty());
        } else {
            assert_eq!(podman.unavailable_engines(), vec![Engine::Snakemake]);
            assert!(podman.to_string().contains("cannot run Snakemake"));
        }

        let docker = doctor_with(vec![Mock::ok(RuntimeKind::Docker, "27.0.1", true)]).await;
        assert!(docker.unavailable_engines().is_empty());
    }

    #[tokio::test]
    async fn no_runtime_makes_all_engines_unavailable() {
        assert_eq!(doctor_with(vec![]).await.unavailable_engines().len(), 4);
    }

    #[tokio::test]
    async fn no_candidates_fails() {
        assert!(!doctor_with(vec![]).await.is_ok());
    }
}
