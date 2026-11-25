//! What the desktop shows about its container runtime, and the request to change its memory.
//! The backend of the desktop app answers with these (`/runtime`), the UI only draws them.

use serde::{Deserialize, Serialize};

use crate::workflow_configuration::engine::Engine;

/// Outcome of a single container runtime check.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StatusLevel {
    Ok,
    Warn,
    Fail,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StatusCheck {
    pub name: String,
    pub level: StatusLevel,
    pub message: String,
}

/// What the user may do about too little memory in the container VM: the limits of the slider and
/// what the change costs. Built by the platform, the UI only draws it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MemoryFix {
    /// Memory the VM has now, `None` if unknown
    pub current_mebibytes: Option<u64>,
    /// Memory of the computer
    pub host_mebibytes: u64,
    /// Lowest value of the slider
    pub minimum_mebibytes: u64,
    /// Highest value of the slider
    pub maximum_mebibytes: u64,
    /// Value the slider starts at
    pub start_mebibytes: u64,
    /// Distance between two positions of the slider
    pub step_mebibytes: u64,
    /// Above this the computer gets slow, the dialog warns
    pub warning_mebibytes: u64,
    /// What the change does to the running system, shown before the user agrees
    pub impact: String,
}

impl MemoryFix {
    /// Text under the slider for the chosen value: what the VM gets and what stays for the system.
    ///
    /// # Arguments
    /// * `mebibytes` - value of the slider
    pub fn summary(&self, mebibytes: u64) -> String {
        let gigabytes = |value: u64| format!("{:.1}", value as f64 / 1024.0);
        format!(
            "{} GB for workflows, {} GB stay for your computer",
            gigabytes(mebibytes),
            gigabytes(self.host_mebibytes.saturating_sub(mebibytes))
        )
    }

    /// Where the slider starts: the memory the VM has now when it is within the slider, else the
    /// recommended start value (the VM has too little).
    pub fn initial_mebibytes(&self) -> u64 {
        match self.current_mebibytes {
            Some(current)
                if (self.minimum_mebibytes..=self.maximum_mebibytes).contains(&current) =>
            {
                current / self.step_mebibytes * self.step_mebibytes
            }
            _ => self.start_mebibytes,
        }
        .clamp(self.minimum_mebibytes, self.maximum_mebibytes)
    }

    /// Whether `mebibytes` is what the VM has already (within one slider step), nothing to restart for.
    ///
    /// # Arguments
    /// * `mebibytes` - value of the slider
    pub fn is_unchanged(&self, mebibytes: u64) -> bool {
        self.current_mebibytes
            .is_some_and(|current| current.abs_diff(mebibytes) < self.step_mebibytes)
    }

    /// Whether `mebibytes` leaves so little for the computer that it may become slow.
    ///
    /// # Arguments
    /// * `mebibytes` - value of the slider
    pub fn is_heavy(&self, mebibytes: u64) -> bool {
        mebibytes > self.warning_mebibytes
    }

    /// Whether workflows with larger tasks may still fail at `mebibytes`.
    ///
    /// # Arguments
    /// * `mebibytes` - value of the slider
    pub fn is_small(&self, mebibytes: u64) -> bool {
        mebibytes < 4 * 1024
    }
}

/// Whether the computer may go to sleep while the app runs workflows. Sleep suspends the
/// container engine and every workflow in it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum SleepStatus {
    /// No workflow is running, nothing to protect
    #[default]
    Idle,
    /// A workflow runs on mains power and the app keeps the computer from sleeping through
    /// inactivity. Closing the lid still puts it to sleep.
    KeepingAwake,
    /// A workflow runs on battery: the app does not keep the computer awake (heat and
    /// battery), so sleep stops the workflow
    OnBattery,
    /// A workflow runs on mains power but the system refused the request to stay awake
    Unavailable,
}

impl SleepStatus {
    /// A workflow is running, so the UI warns about sleep.
    pub fn is_running(&self) -> bool {
        !matches!(self, SleepStatus::Idle)
    }

    /// Warning for the status bar, `None` when no workflow runs.
    pub fn warning(&self) -> Option<&'static str> {
        match self {
            SleepStatus::Idle => None,
            SleepStatus::KeepingAwake => Some(
                "Keeping the computer awake while workflows run. Closing the lid or unplugging the charger lets it sleep and stops the workflows.",
            ),
            SleepStatus::OnBattery => Some(
                "On battery: do not close the lid or let the computer sleep, it would stop the running workflows. Plug in the charger to keep it awake.",
            ),
            SleepStatus::Unavailable => Some(
                "The computer could not be kept awake. Do not close the lid or let it sleep, it would stop the running workflows.",
            ),
        }
    }
}

/// A stopped container engine the app can start for the user.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StartOffer {
    /// What would be started, for the button ("Docker Desktop", "podman machine default")
    pub name: String,
    /// What starting does, in plain language
    pub impact: String,
}

/// Snapshot of the container runtime check of the machine the backend runs on (desktop).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RuntimeStatus {
    /// Runtime binary name, `None` if no runtime was found
    pub kind: Option<String>,
    pub version: Option<String>,
    /// `None` if the runtime does not report it
    pub rootless: Option<bool>,
    /// A runtime was found and no check failed
    pub ok: bool,
    pub checks: Vec<StatusCheck>,
    /// Engines the runtime cannot run (all of them without a runtime)
    pub unavailable_engines: Vec<Engine>,
    /// The memory of the container VM can be changed (Settings): its limits, `None` if the
    /// platform cannot change it
    pub memory_fix: Option<MemoryFix>,
    /// No runtime answers and one can be started from the app, `None` otherwise
    pub start_offer: Option<StartOffer>,
}

impl RuntimeStatus {
    /// Why `engine` cannot run, `None` if it can.
    pub fn engine_hint(&self, engine: &Engine) -> Option<String> {
        if !self.unavailable_engines.contains(engine) {
            return None;
        }
        let Some(kind) = &self.kind else {
            return Some("No container runtime available".to_string());
        };
        let runtime = match &self.version {
            Some(version) => format!("{kind} {version}"),
            None => kind.clone(),
        };
        Some(match engine {
            Engine::Snakemake => format!("{engine} needs Docker; {runtime} cannot run it"),
            _ => format!("{runtime} cannot run {engine} workflows"),
        })
    }

    pub fn level(&self) -> StatusLevel {
        if self.kind.is_none() || !self.ok {
            StatusLevel::Fail
        } else if self.rootless == Some(false)
            || !self.unavailable_engines.is_empty()
            || self.checks.iter().any(|c| c.level == StatusLevel::Warn)
        {
            StatusLevel::Warn
        } else {
            StatusLevel::Ok
        }
    }

    pub fn label(&self) -> String {
        let Some(kind) = &self.kind else {
            return "No container runtime".to_string();
        };
        let mut label = kind.clone();
        if let Some(version) = &self.version {
            label.push(' ');
            label.push_str(version);
        }
        match self.rootless {
            Some(true) => label.push_str(" · rootless"),
            Some(false) => label.push_str(" · rootful"),
            None => {}
        }
        if self.ok && !self.unavailable_engines.is_empty() {
            label.push_str(" · limited");
        }
        label
    }
}

#[cfg(test)]
mod sleep_tests {
    use super::SleepStatus;

    /// Only an idle status has no warning.
    #[test]
    fn idle_has_no_warning() {
        assert!(SleepStatus::Idle.warning().is_none());
        assert!(!SleepStatus::Idle.is_running());
        for status in [
            SleepStatus::KeepingAwake,
            SleepStatus::OnBattery,
            SleepStatus::Unavailable,
        ] {
            assert!(status.warning().is_some());
            assert!(status.is_running());
        }
    }
}

/// Body of the request to change the memory of the container VM.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdjustMemoryRequest {
    /// New memory in mebibytes, within the limits of [`MemoryFix`]
    pub mebibytes: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflow_configuration::engine::Engine;

    fn status(kind: Option<&str>, rootless: Option<bool>, ok: bool) -> RuntimeStatus {
        RuntimeStatus {
            kind: kind.map(str::to_string),
            version: kind.map(|_| "5.1.0".to_string()),
            rootless,
            ok,
            checks: vec![],
            unavailable_engines: vec![],
            memory_fix: None,
            start_offer: None,
        }
    }

    fn fix() -> MemoryFix {
        MemoryFix {
            current_mebibytes: Some(2048),
            host_mebibytes: 32 * 1024,
            minimum_mebibytes: 2048,
            maximum_mebibytes: 24 * 1024,
            start_mebibytes: 16 * 1024,
            step_mebibytes: 512,
            warning_mebibytes: 24 * 1024,
            impact: String::new(),
        }
    }

    /// The slider text names both shares, the warnings follow the limits.
    #[test]
    fn memory_fix_texts() {
        let fix = fix();
        assert_eq!(
            fix.summary(8 * 1024),
            "8.0 GB for workflows, 24.0 GB stay for your computer"
        );
        assert!(!fix.is_heavy(24 * 1024));
        assert!(fix.is_heavy(24 * 1024 + 512));
        assert!(fix.is_small(3 * 1024));
        // Starts at what the VM has when that fits the slider, else at the recommendation
        assert_eq!(fix.initial_mebibytes(), 2048);
        let big = MemoryFix {
            current_mebibytes: Some(16 * 1024 + 100),
            ..fix.clone()
        };
        assert_eq!(big.initial_mebibytes(), 16 * 1024);
        assert!(big.is_unchanged(16 * 1024));
        assert!(!big.is_unchanged(8 * 1024));
        let tiny = MemoryFix {
            current_mebibytes: Some(1900),
            ..fix.clone()
        };
        assert_eq!(tiny.initial_mebibytes(), 16 * 1024);
        assert!(!fix.is_small(4 * 1024));
    }

    #[test]
    fn rootless_is_ok() {
        let s = status(Some("podman"), Some(true), true);
        assert_eq!(s.level(), StatusLevel::Ok);
        assert_eq!(s.label(), "podman 5.1.0 · rootless");
    }

    #[test]
    fn rootful_warns() {
        let s = status(Some("docker"), Some(false), true);
        assert_eq!(s.level(), StatusLevel::Warn);
        assert_eq!(s.label(), "docker 5.1.0 · rootful");
    }

    #[test]
    fn warn_check_warns() {
        let mut s = status(Some("podman"), None, true);
        s.checks.push(StatusCheck {
            name: "x".into(),
            level: StatusLevel::Warn,
            message: String::new(),
        });
        assert_eq!(s.level(), StatusLevel::Warn);
        assert_eq!(s.label(), "podman 5.1.0");
    }

    #[test]
    fn missing_runtime_fails() {
        let s = status(None, None, false);
        assert_eq!(s.level(), StatusLevel::Fail);
        assert_eq!(s.label(), "No container runtime");
    }

    #[test]
    fn failed_check_with_runtime_fails() {
        assert_eq!(
            status(Some("podman"), Some(true), false).level(),
            StatusLevel::Fail
        );
    }

    #[test]
    fn limited_runtime_warns() {
        let mut s = status(Some("podman"), Some(true), true);
        s.unavailable_engines = vec![Engine::Snakemake];
        assert_eq!(s.level(), StatusLevel::Warn);
        assert_eq!(s.label(), "podman 5.1.0 · rootless · limited");
    }

    #[test]
    fn engine_hints() {
        let mut s = status(Some("podman"), Some(true), true);
        s.unavailable_engines = vec![Engine::Snakemake, Engine::Cwl];
        assert_eq!(s.engine_hint(&Engine::Nextflow), None);
        assert_eq!(
            s.engine_hint(&Engine::Snakemake).as_deref(),
            Some("Snakemake needs Docker; podman 5.1.0 cannot run it")
        );
        assert_eq!(
            s.engine_hint(&Engine::Cwl).as_deref(),
            Some("podman 5.1.0 cannot run CWL workflows")
        );
        let none = RuntimeStatus {
            unavailable_engines: vec![Engine::Container],
            ..status(None, None, false)
        };
        assert_eq!(
            none.engine_hint(&Engine::Container).as_deref(),
            Some("No container runtime available")
        );
    }
}
