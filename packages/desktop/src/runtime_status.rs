use macworp_ui_core::runtime_status::{RuntimeStatus, StatusCheck, StatusLevel};
use macworp_worker::container_runtime::{
    Check, CheckStatus, DoctorReport, RuntimeInfo, RuntimePreference, doctor,
};

/// Initial status, handed to the UI through the launch context.
#[derive(Clone)]
pub struct InitialRuntimeStatus(pub RuntimeStatus);

pub fn to_status(report: &DoctorReport) -> RuntimeStatus {
    from_parts(report.info.as_ref(), &report.checks, report.is_ok())
}

/// Runs the doctor check again.
pub async fn check_runtime() -> RuntimeStatus {
    let report = doctor(RuntimePreference::default()).await;
    if report.is_ok() {
        tracing::info!("Container runtime:\n{report}");
    } else {
        tracing::warn!("No usable container runtime, workflows cannot run:\n{report}");
    }
    to_status(&report)
}

fn from_parts(info: Option<&RuntimeInfo>, checks: &[Check], ok: bool) -> RuntimeStatus {
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
        let status = from_parts(Some(&info(Some(true))), &[check(CheckStatus::Ok)], true);
        assert_eq!(status.kind.as_deref(), Some("podman"));
        assert_eq!(status.version.as_deref(), Some("5.1.0"));
        assert_eq!(status.rootless, Some(true));
        assert_eq!(status.level(), StatusLevel::Ok);
    }

    #[test]
    fn rootful_runtime_warns() {
        let status = from_parts(Some(&info(Some(false))), &[check(CheckStatus::Warn)], true);
        assert_eq!(status.level(), StatusLevel::Warn);
        assert_eq!(status.checks[0].level, StatusLevel::Warn);
    }

    #[test]
    fn no_runtime() {
        let status = from_parts(None, &[check(CheckStatus::Fail)], false);
        assert_eq!(status.kind, None);
        assert_eq!(status.level(), StatusLevel::Fail);
        assert_eq!(status.checks[0].message, "msg");
    }

    #[test]
    fn runtime_with_failed_check() {
        let status = from_parts(Some(&info(Some(true))), &[check(CheckStatus::Fail)], false);
        assert_eq!(status.kind.as_deref(), Some("podman"));
        assert_eq!(status.level(), StatusLevel::Fail);
    }
}
