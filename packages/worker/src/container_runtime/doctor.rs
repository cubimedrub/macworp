use std::{fmt, sync::Arc};

use clap::ValueEnum;
use macworp_core::workflow_configuration::engine::Engine;
use strum::IntoEnumIterator;
use thiserror::Error;

use super::runtime::{CliRuntime, ContainerRuntime, RuntimeError, RuntimeInfo, RuntimeKind};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, ValueEnum)]
pub enum RuntimePreference {
    /// Podman if usable, then Docker, on Windows finally wslc
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
pub struct DoctorError(String);

/// Result of probing the host for a usable container runtime.
#[derive(Debug)]
pub struct DoctorReport {
    pub checks: Vec<Check>,
    pub info: Option<RuntimeInfo>,
    runtime: Option<Arc<dyn ContainerRuntime>>,
}

impl DoctorReport {
    /// No failed check and a runtime was found. Warnings do not fail.
    pub fn is_ok(&self) -> bool {
        self.runtime.is_some() && self.checks.iter().all(|c| c.status != CheckStatus::Fail)
    }

    /// Engines that cannot run on the selected runtime (all of them if there is none).
    pub fn unavailable_engines(&self) -> Vec<Engine> {
        Engine::iter()
            .filter(|engine| {
                !self
                    .runtime
                    .as_ref()
                    .is_some_and(|runtime| runtime.supports_engine(engine))
            })
            .collect()
    }

    /// The selected runtime, or the report as error.
    pub fn into_runtime(self) -> Result<Arc<dyn ContainerRuntime>, DoctorError> {
        match self.runtime {
            Some(ref runtime) if self.is_ok() => Ok(runtime.clone()),
            _ => Err(DoctorError(self.to_string())),
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

/// Like [`doctor`] with explicit candidates, tried in order. First usable one wins.
pub async fn doctor_with(candidates: Vec<Arc<dyn ContainerRuntime>>) -> DoctorReport {
    let mut unusable = Vec::new();

    for runtime in candidates {
        let kind = runtime.kind();
        match runtime.info().await {
            Err(error) => unusable.push(Check {
                name: kind.to_string(),
                status: CheckStatus::Fail,
                message: hint(kind, &error),
            }),
            Ok(info) => {
                // Candidates tried before the winner are only a heads-up.
                let mut checks: Vec<Check> = unusable
                    .into_iter()
                    .map(|c| Check {
                        status: CheckStatus::Warn,
                        ..c
                    })
                    .collect();
                checks.extend(checks_for(&info));
                return DoctorReport {
                    checks,
                    info: Some(info),
                    runtime: Some(runtime),
                };
            }
        }
    }

    if unusable.is_empty() {
        unusable.push(Check {
            name: "runtime".to_string(),
            status: CheckStatus::Fail,
            message: "no container runtime candidate configured".to_string(),
        });
    }
    DoctorReport {
        checks: unusable,
        info: None,
        runtime: None,
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

    let unsupported: Vec<String> = Engine::iter()
        .filter(|engine| !kind.supports_engine(engine))
        .map(|engine| engine.to_string())
        .collect();
    if !unsupported.is_empty() {
        let hint = match kind {
            RuntimeKind::Podman => "install docker",
            _ => "install podman or docker",
        };
        checks.push(Check {
            name: "engines".to_string(),
            status: CheckStatus::Warn,
            message: format!(
                "{kind} cannot run {} workflows, {hint} for those",
                unsupported.join(", ")
            ),
        });
    }

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
        assert!(report.into_runtime().is_ok());
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
        assert!(report.into_runtime().is_err());
    }

    #[tokio::test]
    async fn falls_back_to_second_candidate() {
        let report = doctor_with(vec![
            Mock::down(RuntimeKind::Podman),
            Mock::ok(RuntimeKind::Docker, "27.1.1", true),
        ])
        .await;
        assert!(report.is_ok());
        assert_eq!(report.info.as_ref().unwrap().kind, RuntimeKind::Docker);
        assert_eq!(report.checks[0].status, CheckStatus::Warn);
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
        assert!(report.into_runtime().is_err());
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn wslc_only_limits_engines() {
        use macworp_core::workflow_configuration::engine::Engine;

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
        use macworp_core::workflow_configuration::engine::Engine;

        let report = doctor_with(vec![
            Mock::ok(RuntimeKind::Podman, "5.2.1", true),
            Mock::ok(RuntimeKind::Wslc, "2.9.4.0", true),
        ])
        .await;
        assert_eq!(report.unavailable_engines(), vec![Engine::Snakemake]);
    }

    #[tokio::test]
    async fn podman_cannot_run_snakemake_docker_can() {
        use macworp_core::workflow_configuration::engine::Engine;

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
