use std::sync::OnceLock;

use macworp_backend::runtime_control::{BoxFuture, RuntimeControl};
use macworp_core::runtime_status::{MemoryFix, RuntimeStatus, StatusCheck, StatusLevel};
use macworp_core::workflow_configuration::engine::Engine;
use macworp_worker::container_runtime::{
    doctor, Check, CheckStatus, DoctorReport, MemoryAdjustment, MemoryRange, RuntimeInfo,
    RuntimePreference,
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
        socket_exists: None,
        unsupported_socket: None,
        memory_bytes: None,
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

/// What the Settings page needs to let the user change the memory of the container VM, `None`
/// where that is not possible (Linux, wslc, no machine found).
///
/// # Arguments
/// * `report` - the doctor report the status is built from
pub async fn memory_fix(report: &DoctorReport) -> Option<MemoryFix> {
    let info = &report.runtimes.first()?.info;
    let adjustment = MemoryAdjustment::detect(info.kind).await?;
    let range = MemoryRange::of_this_host()?;
    Some(MemoryFix {
        current_mebibytes: info.memory_bytes.map(|bytes| bytes / 1024 / 1024),
        host_mebibytes: MemoryRange::host_mebibytes().unwrap_or(0),
        minimum_mebibytes: range.minimum_mebibytes,
        maximum_mebibytes: range.maximum_mebibytes,
        start_mebibytes: range.start_mebibytes,
        step_mebibytes: range.step_mebibytes,
        warning_mebibytes: range.warning_mebibytes,
        impact: adjustment.impact().to_string(),
    })
}

/// The container runtime of this computer, served by the backend under `/runtime`. Keeps the
/// status of the last check; the UI never runs the doctor itself.
pub struct DesktopRuntime {
    /// Status of the last check
    status: std::sync::Mutex<RuntimeStatus>,
}

impl DesktopRuntime {
    /// Starts with the status of the check made at startup.
    ///
    /// # Arguments
    /// * `status` - the status to serve until the next check
    pub fn new(status: RuntimeStatus) -> Self {
        Self {
            status: std::sync::Mutex::new(status),
        }
    }

    /// Keeps `status` as the latest and returns it.
    ///
    /// # Arguments
    /// * `status` - the result of a check
    fn remember(&self, status: RuntimeStatus) -> RuntimeStatus {
        *self.status.lock().unwrap_or_else(|e| e.into_inner()) = status.clone();
        status
    }

    /// Raises the memory of the container VM and restarts it, then checks again.
    ///
    /// # Arguments
    /// * `mebibytes` - the memory the user chose
    async fn change_memory(&self, mebibytes: u64) -> Result<RuntimeStatus, String> {
        let report = doctor(preference()).await;
        let kind = report
            .runtimes
            .first()
            .map(|usable| usable.info.kind)
            .ok_or("No container runtime found")?;
        let adjustment = MemoryAdjustment::detect(kind)
            .await
            .ok_or("The memory of this container engine cannot be changed from here")?;
        adjustment
            .apply(mebibytes)
            .await
            .map_err(|error| error.to_string())?;
        Ok(self.remember(check_runtime().await))
    }
}

impl RuntimeControl for DesktopRuntime {
    fn status(&self) -> BoxFuture<'_, RuntimeStatus> {
        Box::pin(async {
            self.status
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .clone()
        })
    }

    fn refresh(&self) -> BoxFuture<'_, RuntimeStatus> {
        Box::pin(async { self.remember(check_runtime().await) })
    }

    fn adjust_memory(&self, mebibytes: u64) -> BoxFuture<'_, Result<RuntimeStatus, String>> {
        Box::pin(self.change_memory(mebibytes))
    }
}

/// Runs the doctor check again, with the same runtime preference as at startup.
pub async fn check_runtime() -> RuntimeStatus {
    let report = doctor(preference()).await;
    if report.is_ok() {
        tracing::info!("Container runtime:\n{report}");
    } else {
        tracing::warn!("No usable container runtime, workflows cannot run:\n{report}");
    }
    let mut status = to_status(&report);
    status.memory_fix = memory_fix(&report).await;
    status
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
        memory_fix: None,
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
            socket_exists: None,
            unsupported_socket: None,
            memory_bytes: None,
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

    /// Runs the whole adjustment the Settings button asks the backend for, against the real
    /// runtime: restarts the container engine, so it only runs on request
    /// (`MACWORP_ADJUST_MEBIBYTES=4608`). The active-run check is the backend's, not tested here.
    #[tokio::test]
    #[ignore = "restarts the container engine"]
    async fn settings_button_adjusts_the_real_runtime() {
        let mebibytes: u64 = std::env::var("MACWORP_ADJUST_MEBIBYTES")
            .expect("set MACWORP_ADJUST_MEBIBYTES")
            .parse()
            .unwrap();
        let runtime = DesktopRuntime::new(check_runtime().await);
        let status = runtime.adjust_memory(mebibytes).await.unwrap();
        println!("{:?}", status.memory_fix);
    }
}
