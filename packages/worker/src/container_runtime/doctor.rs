use std::{fmt, sync::Arc};

use clap::ValueEnum;
use macworp_core::workflow_configuration::engine::Engine;
use strum::IntoEnumIterator;
use thiserror::Error;

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

    let report = DoctorReport { checks, runtimes };
    if let Some(check) = report.engines_check() {
        let mut report = report;
        report.checks.push(check);
        return report;
    }
    report
}

impl DoctorReport {
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
            })
        }

        fn down(kind: RuntimeKind) -> Arc<dyn ContainerRuntime> {
            Arc::new(Self {
                kind,
                result: Err("daemon unreachable".to_string()),
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
        // only the engines check warns (Snakemake is Docker-only)
        assert!(
            report
                .checks
                .iter()
                .all(|c| c.status == CheckStatus::Ok || c.name == "engines")
        );
        assert!(report.into_runtimes().is_ok());
    }

    #[tokio::test]
    async fn rootful_docker_warns_but_passes() {
        let report = doctor_with(vec![Mock::ok(RuntimeKind::Docker, "27.1.1", false)]).await;
        assert!(report.is_ok());
        assert!(report.checks.iter().any(|c| c.status == CheckStatus::Warn));
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
            RuntimeKind::Docker
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
    async fn podman_cannot_run_snakemake_docker_can() {
        let podman = doctor_with(vec![Mock::ok(RuntimeKind::Podman, "5.2.1", true)]).await;
        assert_eq!(podman.unavailable_engines(), vec![Engine::Snakemake]);
        assert!(podman.to_string().contains("cannot run Snakemake"));

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
