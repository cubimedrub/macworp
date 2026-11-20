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
}

#[component]
pub fn RuntimeStatusBar() -> Element {
    let Some(handle) = try_use_context::<RuntimeStatusHandle>() else {
        return rsx! {};
    };
    let mut details_open = use_signal(|| false);

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
                        ul {
                            class: "list-unstyled mb-0",
                            for check in status.checks.iter() {
                                li {
                                    key: "{check.name}",
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
        }
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
