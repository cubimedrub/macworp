use std::sync::{Arc, OnceLock};

use macworp_backend::runtime_control::{BoxFuture, RuntimeControl};
use macworp_core::runtime_status::{
    MemoryFix, RuntimeStatus, StartOffer, StatusCheck, StatusLevel,
};
use macworp_core::workflow_configuration::engine::Engine;
use macworp_worker::container_runtime::{
    doctor, Check, CheckStatus, ContainerRuntime, DoctorReport, EngineStart, MemoryAdjustment,
    MemoryRange, RuntimeInfo, RuntimePreference,
};
use tokio::sync::watch;

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

/// The offer to start a stopped container engine, `None` when a runtime answers or there is
/// nothing the app can start.
///
/// # Arguments
/// * `report` - the doctor report the status is built from
pub async fn start_offer(report: &DoctorReport) -> Option<StartOffer> {
    if report.is_ok() {
        return None;
    }
    let start = EngineStart::detect(&preference().kinds()).await?;
    Some(StartOffer {
        name: start.name(),
        impact: start.impact().to_string(),
    })
}

/// The status of a doctor report with the things the UI can offer (more memory, starting).
///
/// # Arguments
/// * `report` - what the doctor found
pub async fn status_of(report: &DoctorReport) -> RuntimeStatus {
    let mut status = to_status(report);
    status.memory_fix = memory_fix(report).await;
    status.start_offer = start_offer(report).await;
    status
}

/// What MAcWorP keeps on disk and the container VM must see: name for messages and path. Asked
/// again at every check, the user can move the folders in the settings.
pub type Folders = Arc<dyn Fn() -> Vec<(String, std::path::PathBuf)> + Send + Sync>;

/// The container runtime of this computer, served by the backend under `/runtime`. Keeps the
/// status of the last check; the UI never runs the doctor itself.
pub struct DesktopRuntime {
    /// Status of the last check
    status: std::sync::Mutex<RuntimeStatus>,
    /// The usable runtimes, `None` while none answers. The embedded worker waits on it: when a
    /// runtime appears (the user started the engine) it starts working.
    runtimes: watch::Sender<Option<Vec<Arc<dyn ContainerRuntime>>>>,
    /// The folders the VM must see
    folders: Folders,
}

impl DesktopRuntime {
    /// Starts with the status of the check made at startup.
    ///
    /// # Arguments
    /// * `status` - the status to serve until the next check
    /// * `runtimes` - publishes the usable runtimes to the embedded worker
    /// * `folders` - the folders the VM must see, checked with every check
    pub fn new(
        status: RuntimeStatus,
        runtimes: watch::Sender<Option<Vec<Arc<dyn ContainerRuntime>>>>,
        folders: Folders,
    ) -> Self {
        Self {
            status: std::sync::Mutex::new(status),
            runtimes,
            folders,
        }
    }

    /// Checks the runtime, keeps the result as the latest status and hands newly usable
    /// runtimes to the embedded worker.
    async fn checked(&self) -> RuntimeStatus {
        let (status, runtimes) = check(&self.folders).await;
        if runtimes.is_some() {
            // Only the first appearance matters: a worker that runs keeps its runtimes
            self.runtimes.send_if_modified(|current| {
                if current.is_none() {
                    *current = runtimes;
                    true
                } else {
                    false
                }
            });
        }
        *self.status.lock().unwrap_or_else(|e| e.into_inner()) = status.clone();
        status
    }

    /// Starts the stopped engine, then checks again.
    async fn start_engine(&self) -> Result<RuntimeStatus, String> {
        let start = EngineStart::detect(&preference().kinds())
            .await
            .ok_or("There is no container engine the app can start")?;
        start.start().await.map_err(|error| error.to_string())?;
        Ok(self.checked().await)
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
        Ok(self.checked().await)
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
        Box::pin(self.checked())
    }

    fn unavailable_engines(&self) -> Vec<Engine> {
        self.status
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .unavailable_engines
            .clone()
    }

    fn start(&self) -> BoxFuture<'_, Result<RuntimeStatus, String>> {
        Box::pin(self.start_engine())
    }

    fn adjust_memory(&self, mebibytes: u64) -> BoxFuture<'_, Result<RuntimeStatus, String>> {
        Box::pin(self.change_memory(mebibytes))
    }
}

/// Runs the doctor check again, with the same runtime preference as at startup.
///
/// # Arguments
/// * `folders` - the folders the VM must see
///
/// # Returns
/// The status for the UI and the usable runtimes, `None` if there is none.
pub async fn check(folders: &Folders) -> (RuntimeStatus, Option<Vec<Arc<dyn ContainerRuntime>>>) {
    let mut report = doctor(preference()).await;
    report.check_shared_folders(&folders()).await;
    if report.is_ok() {
        tracing::info!("Container runtime:\n{report}");
    } else {
        tracing::warn!("No usable container runtime, workflows cannot run:\n{report}");
    }
    let status = status_of(&report).await;
    (status, report.into_runtimes().ok())
}

/// [`check`] for callers that only need the status.
#[cfg(test)]
pub async fn check_runtime() -> RuntimeStatus {
    check(&(Arc::new(Vec::new) as Folders)).await.0
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
        start_offer: None,
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
        let folders: Folders = Arc::new(Vec::new);
        let (status, runtimes) = super::check(&folders).await;
        let runtime = DesktopRuntime::new(status, watch::channel(runtimes).0, folders);
        let status = runtime.adjust_memory(mebibytes).await.unwrap();
        println!("{:?}", status.memory_fix);
    }

    /// Starts the real stopped engine the way the "Start" button does and prints the new
    /// status. Starts a podman machine or Docker Desktop, so it only runs on request.
    #[tokio::test]
    #[ignore = "starts the container engine"]
    async fn start_button_starts_the_real_engine() {
        let folders: Folders = Arc::new(Vec::new);
        let (status, runtimes) = super::check(&folders).await;
        println!("before: ok={} offer={:?}", status.ok, status.start_offer);
        let (sender, mut receiver) = watch::channel(runtimes);
        let runtime = DesktopRuntime::new(status, sender, folders);
        let after = runtime.start().await.unwrap();
        println!("after: ok={} offer={:?}", after.ok, after.start_offer);
        assert!(after.ok);
        // The embedded worker would be told now
        assert!(receiver.borrow_and_update().is_some());
    }
}
