use dioxus::prelude::*;

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
}

impl RuntimeStatus {
    pub fn level(&self) -> StatusLevel {
        if self.kind.is_none() || !self.ok {
            StatusLevel::Fail
        } else if self.rootless == Some(false)
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
        label
    }
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
}
