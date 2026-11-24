use dioxus::prelude::*;
use macworp_core::{
    responses::EnginesResponse,
    workflow_configuration::engine::{Engine, ExecutionHost},
};

pub use macworp_core::runtime_status::{MemoryFix, RuntimeStatus, StatusCheck, StatusLevel};

/// Short tag of a check result.
///
/// # Arguments
/// * `level` - outcome of the check
fn level_tag(level: StatusLevel) -> &'static str {
    match level {
        StatusLevel::Ok => "ok",
        StatusLevel::Warn => "warn",
        StatusLevel::Fail => "FAIL",
    }
}

/// Bootstrap class of the badge for a check result.
///
/// # Arguments
/// * `level` - outcome of the check
fn level_badge_class(level: StatusLevel) -> &'static str {
    match level {
        StatusLevel::Ok => "text-bg-success",
        StatusLevel::Warn => "text-bg-warning",
        StatusLevel::Fail => "text-bg-danger",
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
                class: "badge border-0 {level_badge_class(level)}",
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
                                    span { class: "badge me-2 {level_badge_class(check.level)}", "{level_tag(check.level)}" }
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
