use dioxus::prelude::*;
use macworp_core::{
    responses::EnginesResponse,
    workflow_configuration::engine::{Engine, ExecutionHost},
};

/// Outcome of a single container runtime check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusLevel {
    Ok,
    Warn,
    Fail,
}

impl StatusLevel {
    fn tag(self) -> &'static str {
        match self {
            StatusLevel::Ok => "ok",
            StatusLevel::Warn => "warn",
            StatusLevel::Fail => "FAIL",
        }
    }

    fn badge_class(self) -> &'static str {
        match self {
            StatusLevel::Ok => "text-bg-success",
            StatusLevel::Warn => "text-bg-warning",
            StatusLevel::Fail => "text-bg-danger",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct StatusCheck {
    pub name: String,
    pub level: StatusLevel,
    pub message: String,
}

/// What the user may do about too little memory in the container VM: the limits of the slider and
/// what the change costs. Built by the platform, the UI only draws it.
#[derive(Debug, Clone, PartialEq)]
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

/// Snapshot of the container runtime check, built by the platform (desktop) from its doctor report.
#[derive(Debug, Clone, PartialEq)]
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

/// Hint why `engine` cannot run, `None` if it can or the platform provides no runtime status.
/// Reactive: follows refreshes of the status.
pub fn engine_hint(engine: &Engine) -> Option<String> {
    if let Some(handle) = try_consume_context::<RuntimeStatusHandle>() {
        let hint = handle.status.read().engine_hint(engine);
        return hint;
    }
    let availability = try_consume_context::<EngineAvailability>()?;
    let response = availability.response.read();
    worker_engine_hint(response.as_ref()?, engine)
}

/// Why no worker can run `engine`, `None` if one can.
///
/// # Arguments
/// * `response` - engine availability reported by the backend
/// * `engine` - engine to check
pub fn worker_engine_hint(response: &EnginesResponse, engine: &Engine) -> Option<String> {
    if !response.unavailable.contains(engine) {
        None
    } else if response.workers_online == 0 {
        Some("No worker online".to_string())
    } else {
        Some(format!("No online worker can run {engine} workflows"))
    }
}

/// Engine availability of the workers behind a remote backend (web). Provided as context by the
/// layout when the platform has no [`RuntimeStatusHandle`]; `None` until the first answer.
#[derive(Clone, Copy)]
pub struct EngineAvailability {
    pub response: Signal<Option<EnginesResponse>>,
}

/// Provided as context by platforms that run workflows locally (desktop). `RuntimeStatusBar`
/// renders nothing without it.
#[derive(Clone, Copy)]
pub struct RuntimeStatusHandle {
    pub status: Signal<RuntimeStatus>,
    pub refreshing: Signal<bool>,
    /// Re-runs the check and updates `status`
    pub refresh: Callback<()>,
    /// Sets the memory of the container VM (mebibytes) and restarts it; the platform refreshes
    /// `status` afterwards
    pub adjust_memory: Callback<u64>,
    /// An adjustment is running: the engine is restarting, this takes a while
    pub adjusting: Signal<bool>,
    /// Why the last adjustment did not work, `None` otherwise
    pub adjust_error: Signal<Option<String>>,
}

#[component]
pub fn RuntimeStatusBar() -> Element {
    let mut details_open = use_signal(|| false);
    let Some(handle) = try_use_context::<RuntimeStatusHandle>() else {
        return rsx! {};
    };

    let status = handle.status.read().clone();
    let level = status.level();
    let label = status.label();
    let refreshing = *handle.refreshing.read();

    rsx! {
        div {
            class: "runtime-status-bar d-flex flex-row align-items-center gap-2 px-2 py-1 border-top",
            button {
                r#type: "button",
                class: "badge border-0 {level.badge_class()}",
                title: "Show container runtime details",
                onclick: move |_| details_open.set(true),
                "{label}"
            }
            button {
                r#type: "button",
                class: "btn btn-sm btn-outline-secondary",
                title: "Check container runtime again",
                disabled: refreshing,
                onclick: move |_| handle.refresh.call(()),
                if refreshing {
                    span { class: "spinner-border spinner-border-sm", role: "status" }
                } else {
                    i { class: "fa-solid fa-rotate" }
                }
            }
        }

        div {
            class: if *details_open.read() { "modal show" } else { "modal" },
            style: if *details_open.read() { "display: block;" } else { "display: none;" },
            tabindex: "-1",
            div {
                class: "modal-dialog modal-lg",
                div {
                    class: "modal-content",
                    div {
                        class: "modal-header",
                        h5 { class: "modal-title", "Container runtime" }
                        button {
                            r#type: "button",
                            class: "btn-close",
                            aria_label: "Close",
                            onclick: move |_| details_open.set(false),
                        }
                    }
                    div {
                        class: "modal-body",
                        if !status.unavailable_engines.is_empty() {
                            p {
                                class: "mb-2",
                                strong { "Unavailable engines: " }
                                {status.unavailable_engines.iter().map(|e| e.to_string()).collect::<Vec<_>>().join(", ")}
                            }
                        }
                        if status.memory_fix.is_some() && status.checks.iter().any(|c| c.name == "memory" && c.level == StatusLevel::Warn) {
                            p {
                                class: "alert alert-warning",
                                "Workflows with larger tasks need more memory than the container engine has. You can raise it under Settings."
                            }
                        }
                        ul {
                            class: "list-unstyled mb-0",
                            // Two runtimes report the same check names: the position is part of the key
                            for (index, check) in status.checks.iter().enumerate() {
                                li {
                                    key: "{index}-{check.name}",
                                    span { class: "badge me-2 {check.level.badge_class()}", "{check.level.tag()}" }
                                    strong { "{check.name}: " }
                                    "{check.message}"
                                }
                            }
                        }
                    }
                    div {
                        class: "modal-footer",
                        button {
                            r#type: "button",
                            class: "btn btn-secondary",
                            onclick: move |_| details_open.set(false),
                            "Close"
                        }
                    }
                }
            }
        }
    }
}

/// Settings section that lets the user change the memory of the container VM with a slider
/// (podman machine, Docker Desktop, WSL2). Renders nothing without a [`RuntimeStatusHandle`] or
/// when the platform cannot change the memory. Nothing changes before the user presses the button.
#[component]
pub fn MemorySetting() -> Element {
    // Hooks first: the offer comes and goes with the status, the hooks must not
    let mut chosen = use_signal(|| None::<u64>);
    let Some(handle) = try_use_context::<RuntimeStatusHandle>() else {
        return rsx! {};
    };
    let Some(fix) = handle.status.read().memory_fix.clone() else {
        return rsx! {};
    };
    let value = chosen.read().unwrap_or_else(|| fix.initial_mebibytes());
    let summary = fix.summary(value);
    let busy = *handle.adjusting.read();
    let unchanged = fix.is_unchanged(value);
    let current = fix
        .current_mebibytes
        .map(|current| format!("{:.1} GB", current as f64 / 1024.0))
        .unwrap_or_else(|| "an unknown amount of memory".to_string());
    let label_class = if fix.is_heavy(value) {
        "text-warning-emphasis"
    } else {
        "text-body-secondary"
    };
    let low = fix
        .current_mebibytes
        .is_some_and(|current| fix.is_small(current));
    rsx! {
        h2 { class: "h4 mt-4", "Container engine memory" }
        p {
            "Workflows run in a container engine that has {current} of memory. Workflows with larger tasks are stopped when it runs out. "
            "{fix.impact}"
        }
        if low {
            p { class: "text-warning-emphasis", "That is little: raising it to 4 GB or more is recommended." }
        }
        input {
            r#type: "range",
            class: "form-range",
            min: "{fix.minimum_mebibytes}",
            max: "{fix.maximum_mebibytes}",
            step: "{fix.step_mebibytes}",
            value: "{value}",
            disabled: busy,
            aria_label: "Memory for workflows",
            oninput: move |event| {
                if let Ok(mebibytes) = event.value().parse::<u64>() {
                    chosen.set(Some(mebibytes));
                }
            },
        }
        p { class: "mb-1 {label_class}", "{summary}" }
        if fix.is_heavy(value) {
            p { class: "mb-1 text-warning-emphasis", "Your computer may become slow with this much." }
        }
        if fix.is_small(value) {
            p { class: "mb-1 text-body-secondary", "Large workflows may still fail with this little." }
        }
        if let Some(message) = handle.adjust_error.read().clone() {
            p { class: "mb-1 text-danger", "{message}" }
        }
        button {
            r#type: "button",
            class: "btn btn-warning",
            disabled: busy || unchanged,
            onclick: move |_| handle.adjust_memory.call(value),
            if busy {
                span { class: "spinner-border spinner-border-sm me-2", role: "status" }
                "Restarting the container engine..."
            } else {
                "Restart engine with {value as f64 / 1024.0:.1} GB"
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn status(kind: Option<&str>, rootless: Option<bool>, ok: bool) -> RuntimeStatus {
        RuntimeStatus {
            kind: kind.map(str::to_string),
            version: kind.map(|_| "5.1.0".to_string()),
            rootless,
            ok,
            checks: vec![],
            unavailable_engines: vec![],
            memory_fix: None,
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

    /// Hints from the worker availability: none worker, or only others
    #[test]
    fn worker_engine_hints() {
        let none = EnginesResponse {
            unavailable: vec![Engine::Nextflow, Engine::Snakemake],
            workers_online: 0,
        };
        assert_eq!(
            worker_engine_hint(&none, &Engine::Nextflow).as_deref(),
            Some("No worker online")
        );
        assert_eq!(worker_engine_hint(&none, &Engine::Cwl), None);
        let some = EnginesResponse {
            unavailable: vec![Engine::Snakemake],
            workers_online: 2,
        };
        assert_eq!(
            worker_engine_hint(&some, &Engine::Snakemake).as_deref(),
            Some("No online worker can run Snakemake workflows")
        );
        assert_eq!(worker_engine_hint(&some, &Engine::Nextflow), None);
    }
}

/// Which kind of host the UI talks to: the desktop app provides the [`RuntimeStatusHandle`]
/// (runs happen on the user's own computer), the web frontend does not (a server with workers).
pub fn execution_host() -> ExecutionHost {
    if try_consume_context::<RuntimeStatusHandle>().is_some() {
        ExecutionHost::Desktop
    } else {
        ExecutionHost::Server
    }
}

/// Info box saying what a workflow of `engine` has to expect from the execution environment
/// (executor and software deployment are fixed by MAcWorP), worded for the desktop app or a
/// server, see [`Engine::execution_environment`].
///
/// # Arguments
/// * `engine` - engine of the workflow
#[component]
pub fn ExecutionEnvironmentNote(engine: Engine) -> Element {
    let text = engine.execution_environment(execution_host());
    rsx! {
        div {
            class: "alert alert-info",
            role: "note",
            "{text}"
        }
    }
}
