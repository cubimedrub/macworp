use std::fmt::Display;

use dioxus::prelude::*;

use macworp_core::responses::UserResponse;

use crate::{
    client::Client,
    components::{AccountDeletionRules, ConfirmationDialog},
    configuration::Configuration,
    current_user::CurrentUser,
    router::IsRouter,
    runtime_status::RuntimeStatusBar,
    session::Session,
    sharing::SharingEnabled,
    storage::IsStorage,
};

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
pub fn TwoPane<R: IsRouter, S: IsStorage + 'static>() -> Element {
    let config = use_context::<Signal<Configuration>>();
    let storage = use_context::<Signal<S>>();

    // Check if user is logged in
    match storage.read().jwt() {
        Ok(Some(_)) => {}
        Ok(None) => {
            let nav = navigator();
            nav.push(R::login_route(Some(
                "Please log in to access the application".to_string(),
            )));
        }
        Err(err) => {
            let nav = navigator();
            nav.push(R::error_route(format!("{err}")));
        }
    }

    let frontend_version = use_signal(|| env!("CARGO_PKG_VERSION"));
    let mut navigation_state = use_signal(|| NavigationState::Open);

    let backend_version: Resource<Option<String>> = use_resource(move || async move {
        let config = config.read().clone();
        Some(
            Client::new(config.base_url().to_string(), config.timeout())
                .utilities()
                .version()
                .await
                .unwrap_or("".to_string()),
        )
    });

    let current_user_resource = use_resource(move || async move {
        let session = Session::new(storage, config).ok()?;
        let user: Option<UserResponse> = session.client.users().current(&session.jwt).await.ok();
        user
    });
    let current_user = use_context_provider(|| CurrentUser::new(current_user_resource));

    let mut is_delete_account_open = use_signal(|| false);
    let mut delete_account_error: Signal<Option<String>> = use_signal(|| None);

    // Deletes the own account, then behaves like a logout: the server rejects the token from now
    // on, so it is dropped and the user returns to the login page.
    let delete_account = move |_| {
        let Some(user) = current_user.user() else {
            return;
        };
        spawn(async move {
            let result = match Session::new(storage, config) {
                Ok(session) => session
                    .client
                    .users()
                    .delete(&session.jwt, user.id)
                    .await
                    .map_err(|e| e.to_string()),
                Err(e) => Err(e),
            };
            match result {
                Ok(()) => {
                    is_delete_account_open.set(false);
                    delete_account_error.set(None);
                    let mut storage = storage;
                    let stored = storage.write().set_jwt(None);
                    let nav = navigator();
                    match stored {
                        Ok(()) => nav.push(R::login_route(Some("Your account was deleted".to_string()))),
                        Err(err) => nav.push(R::error_route(format!("{err}"))),
                    };
                }
                Err(e) => delete_account_error.set(Some(e)),
            }
        });
    };

    let toggle_navigation = move |_| {
        let new_state = match *navigation_state.read() {
            NavigationState::Open => NavigationState::Close,
            NavigationState::Close => NavigationState::Open,
        };
        navigation_state.set(new_state);
    };

    rsx!(div {
        class: "layout-two-pane d-flex flex-row",

        ConfirmationDialog {
            is_open: is_delete_account_open,
            title: "Delete my account".to_string(),
            confirmation_button_label: "Delete account".to_string(),
            confirmation_callback: Callback::new(delete_account),
            cancel_callback: Callback::new(move |_| is_delete_account_open.set(false)),
            AccountDeletionRules {}
            if let Some(message) = delete_account_error() {
                div { class: "alert alert-danger", role: "alert", "{message}" }
            }
        }

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
                class: "flex-fill overflow-auto",
                ul {
                    class: "navbar-nav me-auto mb-2 mb-lg-0",

                    li {
                        class: "nav-item",
                        Link {
                            class: "nav-link",
                            to: R::project_index_route(),
                            i {
                                class: "fa-solid fa-folder-open me-3"
                            }
                            "Projects"
                        }
                    }
                    li {
                        class: "nav-item",
                        Link {
                            class: "nav-link",
                            to: R::workflow_integration_index_route(),
                            i {
                                class: "fa-solid fa-diagram-project me-3"
                            }
                            "Workflow integration"
                        }
                    }
                    if SharingEnabled::from_context().is_enabled() {
                        li {
                            class: "nav-item",
                            Link {
                                class: "nav-link",
                                to: R::group_index_route(),
                                i {
                                    class: "fa-solid fa-users me-3"
                                }
                                "Groups"
                            }
                        }
                        if current_user.is_admin() {
                            li {
                                class: "nav-item",
                                Link {
                                    class: "nav-link",
                                    to: R::user_index_route(),
                                    i {
                                        class: "fa-solid fa-user-gear me-3"
                                    }
                                    "Users"
                                }
                            }
                        }
                        li {
                            class: "nav-item",
                            button {
                                class: "nav-link btn btn-link text-start",
                                onclick: move |_| is_delete_account_open.set(true),
                                i {
                                    class: "fa-solid fa-user-slash me-3"
                                }
                                "Delete my account"
                            }
                        }
                    }
                }
            }

            nav {
                class: "external-nav"
            }

            div {
                class: "software d-flex flex-column",

                span {
                    "Frontend version: {frontend_version}"
                }
                span {
                    match &*backend_version.read() {
                        Some(Some(version)) => rsx!("Backend version: {version}"),
                        _ => rsx!("Backend version: unknown"),
                    }
                }

            }
        }
        div {
            class: "d-flex flex-column flex-fill min-vh-100",
            main {
                class: "main-panel flex-fill conainer-fluid p-1",
                Outlet::<R> {}
            }
            RuntimeStatusBar {}
        }
    })
}
