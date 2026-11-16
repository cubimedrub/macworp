use std::sync::OnceLock;

use macworp_core::workflow_configuration::engine::Engine;
use macworp_ui_core::runtime_status::{RuntimeStatus, StatusCheck, StatusLevel};
use macworp_worker::container_runtime::{
    doctor, Check, CheckStatus, DoctorReport, RuntimeInfo, RuntimePreference,
};

/// Initial status, handed to the UI through the launch context.
#[derive(Clone)]
pub struct InitialRuntimeStatus(pub RuntimeStatus);

/// Snapshot for the UI. With several usable runtimes the kind lists all of them
/// (`podman + docker`) and the first one's version is shown; rootless is only reported when
/// every runtime agrees on it, a single rootful runtime wins.
pub fn to_status(report: &DoctorReport) -> RuntimeStatus {
    let infos: Vec<&RuntimeInfo> = report.runtimes.iter().map(|usable| &usable.info).collect();
    let combined = infos.first().map(|first| RuntimeInfo {
        kind: first.kind,
        version: if infos.len() == 1 {
            first.version.clone()
        } else {
            String::new()
        },
        rootless: if infos.iter().any(|info| info.rootless == Some(false)) {
            Some(false)
        } else if infos.iter().all(|info| info.rootless == Some(true)) {
            Some(true)
        } else {
            None
        },
        storage_root: None,
        socket: None,
    });
    let mut status = from_parts(
        combined.as_ref(),
        &report.checks,
        report.is_ok(),
        report.unavailable_engines(),
    );
    if infos.len() > 1 {
        status.kind = Some(
            infos
                .iter()
                .map(|info| format!("{} {}", info.kind, info.version))
                .collect::<Vec<_>>()
                .join(" + "),
        );
        status.version = None;
    }
    status
}

static PREFERENCE: OnceLock<RuntimePreference> = OnceLock::new();

/// Sets the container runtime the user enforced (`--container-runtime`), once at startup.
pub fn set_preference(preference: RuntimePreference) {
    PREFERENCE.set(preference).ok();
}

/// The enforced container runtime, `Auto` if none was set.
pub fn preference() -> RuntimePreference {
    PREFERENCE.get().copied().unwrap_or_default()
}

/// Runs the doctor check again, with the same runtime preference as at startup.
pub async fn check_runtime() -> RuntimeStatus {
    let report = doctor(preference()).await;
    if report.is_ok() {
        tracing::info!("Container runtime:\n{report}");
    } else {
        tracing::warn!("No usable container runtime, workflows cannot run:\n{report}");
    }
    to_status(&report)
}

fn from_parts(
    info: Option<&RuntimeInfo>,
    checks: &[Check],
    ok: bool,
    unavailable_engines: Vec<Engine>,
) -> RuntimeStatus {
    RuntimeStatus {
        kind: info.map(|i| i.kind.to_string()),
        version: info.map(|i| i.version.clone()),
        rootless: info.and_then(|i| i.rootless),
        ok,
        checks: checks
            .iter()
            .map(|c| StatusCheck {
                name: c.name.clone(),
                level: match c.status {
                    CheckStatus::Ok => StatusLevel::Ok,
                    CheckStatus::Warn => StatusLevel::Warn,
                    CheckStatus::Fail => StatusLevel::Fail,
                },
                message: c.message.clone(),
            })
            .collect(),
        unavailable_engines,
    }
}

#[cfg(test)]
mod tests {
    use macworp_worker::container_runtime::RuntimeKind;

    use super::*;

    fn info(rootless: Option<bool>) -> RuntimeInfo {
        RuntimeInfo {
            kind: RuntimeKind::Podman,
            version: "5.1.0".to_string(),
            rootless,
            storage_root: None,
            socket: None,
        }
    }

    fn check(status: CheckStatus) -> Check {
        Check {
            name: "rootless".to_string(),
            status,
            message: "msg".to_string(),
        }
    }

    #[test]
    fn rootless_runtime() {
        let status = from_parts(
            Some(&info(Some(true))),
            &[check(CheckStatus::Ok)],
            true,
            vec![],
        );
        assert_eq!(status.kind.as_deref(), Some("podman"));
        assert_eq!(status.version.as_deref(), Some("5.1.0"));
        assert_eq!(status.rootless, Some(true));
        assert_eq!(status.level(), StatusLevel::Ok);
    }

    #[test]
    fn rootful_runtime_warns() {
        let status = from_parts(
            Some(&info(Some(false))),
            &[check(CheckStatus::Warn)],
            true,
            vec![],
        );
        assert_eq!(status.level(), StatusLevel::Warn);
        assert_eq!(status.checks[0].level, StatusLevel::Warn);
    }

    #[test]
    fn no_runtime() {
        let status = from_parts(
            None,
            &[check(CheckStatus::Fail)],
            false,
            vec![
                Engine::Nextflow,
                Engine::Snakemake,
                Engine::Cwl,
                Engine::Container,
            ],
        );
        assert_eq!(status.unavailable_engines.len(), 4);
        assert_eq!(status.kind, None);
        assert_eq!(status.level(), StatusLevel::Fail);
        assert_eq!(status.checks[0].message, "msg");
    }

    #[test]
    fn runtime_with_failed_check() {
        let status = from_parts(
            Some(&info(Some(true))),
            &[check(CheckStatus::Fail)],
            false,
            vec![],
        );
        assert_eq!(status.kind.as_deref(), Some("podman"));
        assert_eq!(status.level(), StatusLevel::Fail);
    }
}
