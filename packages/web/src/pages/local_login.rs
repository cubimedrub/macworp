use dioxus::{logger::tracing, prelude::*};

use macworp_ui_core::{client::Client, configuration::Configuration, storage::IsStorage};
use thiserror::Error;

use crate::{router::Router, storage::Storage};

#[derive(Debug, Error)]
pub enum Error {
    #[error("Failed to fetch projects: {0}")]
    Login(#[from] macworp_ui_core::client::Error),
    #[error("{0}")]
    Storage(<Storage as IsStorage>::Error),
}

#[component]
pub fn LocalLogin() -> Element {
    let config = use_context::<Signal<Configuration>>();
    let mut storage = use_context::<Signal<Storage>>();

    let mut username = use_signal(String::new);
    let mut password = use_signal(String::new);

    let is_login_button_disabled =
        use_memo(move || username.read().is_empty() || password.read().is_empty());

    let mut login = use_action(move || async move {
        let username = username.read().clone();
        let password = password.read().clone();

        if username.is_empty() || password.is_empty() {
            return Ok::<(), Error>(());
        }

        let config = config.read().clone();

        let jwt = Client::new(config.base_url().to_string(), config.timeout())
            .login()
            .local_login(username, password)
            .await
            .map_err(Error::Login)?;

        tracing::info!("Successfully logged in, received JWT: {}", jwt);

        tracing::info!("Storing JWT in storage");

        storage.write().set_jwt(Some(jwt)).map_err(Error::Storage)?;

        tracing::info!("JWT stored successfully, navigating to project index");

        let nav = navigator();
        nav.push(Router::ProjectIndex);

        Ok(())
    });

    rsx! {
        h1 {
            "Login"
        }
        match login.value() {
            Some(Err(err)) => rsx! {
                div { class: "alert alert-danger", role: "alert", "Failed to create project: {err}" }
            },
            _ => rsx! {}
        }
        div {
            class: "input-group mb-3",
            span {
                class: "input-group-text",
                id: "username",
                "Username"
            }
            input {
                type: "text",
                class: "form-control",
                placeholder: "Username",
                aria_label: "Username",
                aria_describedby: "username",
                value: "{username}",
                oninput: move |event| username.set(event.value()),
                onkeydown: move |e| {
                    if e.key() == Key::Enter {
                        login.call();
                    }
                },
            }
        }
        div {
            class: "input-group mb-3",
            span {
                class: "input-group-text",
                id: "password",
                "Password"
            }
            input {
                type: "password",
                class: "form-control",
                placeholder: "Password",
                aria_label: "Password",
                aria_describedby: "password",
                value: "{password}",
                oninput: move |event| password.set(event.value()),
                onkeydown: move |e| {
                    if e.key() == Key::Enter {
                        login.call();
                    }
                },
            }
        }
        div {
            class: "d-flex justify-content-end",
            button {
                class: "btn btn-primary",
                disabled: is_login_button_disabled,
                onclick: move |_| login.call(),
                "Create"
            }
        }
    }
}
