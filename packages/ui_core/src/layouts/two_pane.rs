use std::fmt::Display;

use dioxus::prelude::*;

use crate::{client::Client, configuration::Configuration, routes::Routes};

enum NavigationState {
    Open,
    Close,
}

impl Display for NavigationState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NavigationState::Open => write!(f, "opened"),
            NavigationState::Close => write!(f, "closed"),
        }
    }
}

#[component]
pub fn TwoPane() -> Element {
    let desktop_version = use_signal(|| env!("CARGO_PKG_VERSION"));
    let mut navigation_state = use_signal(|| NavigationState::Open);
    let config = use_context::<Resource<Configuration>>();

    let backend_version: Resource<Option<String>> = use_resource(move || async move {
        let base_url = match config.read_unchecked().as_ref() {
            Some(config) => config.base_url().to_string(),
            None => return None,
        };

        Some(
            Client::new(base_url, 10)
                .utilities()
                .version()
                .await
                .unwrap_or("".to_string()),
        )
    });

    let toggle_navigation = move |_| {
        let new_state = match *navigation_state.read() {
            NavigationState::Open => NavigationState::Close,
            NavigationState::Close => NavigationState::Open,
        };
        navigation_state.set(new_state);
    };

    rsx!(div {
        class: "layout-two-pane d-flex flex-row",

        nav {
            class: "navigation-panel d-flex flex-column min-vh-100 p-1 border-end border-3 border-black {navigation_state}",
            div {
                class: match *navigation_state.read() {
                    NavigationState::Open => "logo d-flex flex-row justify-content-between",
                    NavigationState::Close => "logo d-flex flex-column flex-column-reverse",
                },
                h5 {
                    class: match *navigation_state.read() {
                        NavigationState::Open => "mb-0",
                        NavigationState::Close => "m-0 mt-2",
                    },
                    "MAcWorP"
                }
                button {
                    id: "collapse-btn",
                    class: "btn btn-sm btn-outline-secondary",
                    title: "Collapse sidebar",
                    onclick: toggle_navigation,
                    i {
                        class: match *navigation_state.read() {
                            NavigationState::Open => "fa-solid fa-angle-double-left",
                            NavigationState::Close => "fa-solid fa-angle-double-right",
                        }
                    }
                }
            }

            nav {
                class: "flex-fill overflow-auto"
            }

            nav {
                class: "external-nav"
            }

            div {
                class: "software d-flex flex-column",

                span {
                    "Desktop version: {desktop_version}"
                }
                span {
                    match &*backend_version.read() {
                        Some(Some(version)) => rsx!("Backend version: {version}"),
                        _ => rsx!("Backend version: unknown"),
                    }
                }

            }
        }
        main {
            class: "main-panel min-vh-100 flex-fill conainer-fluid",
            Outlet::<Routes> {}
        }
    })
}
