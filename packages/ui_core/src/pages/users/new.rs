use dioxus::prelude::*;
use macworp_core::requests::NewUserRequest;

use crate::{configuration::Configuration, router::IsRouter, session::Session, storage::IsStorage};

/// Form to create a local user; for administrators.
#[component]
pub fn New<S: IsStorage + 'static, R: IsRouter + 'static>() -> Element {
    let storage = use_context::<Signal<S>>();
    let config = use_context::<Signal<Configuration>>();
    let navigator = navigator();

    let mut username = use_signal(String::new);
    let mut firstname = use_signal(String::new);
    let mut lastname = use_signal(String::new);
    let mut email = use_signal(String::new);
    let mut password = use_signal(String::new);
    let mut is_admin = use_signal(|| false);
    let is_create_disabled = use_memo(move || {
        [username, firstname, lastname, email, password]
            .iter()
            .any(|field| field.read().trim().is_empty())
    });

    let mut create_action = use_action(move || async move {
        let session = Session::new(storage, config).map_err(dioxus::CapturedError::from_display)?;
        let request = NewUserRequest {
            username: username.read().trim().to_string(),
            firstname: firstname.read().trim().to_string(),
            lastname: lastname.read().trim().to_string(),
            email: email.read().trim().to_string(),
            is_admin: is_admin(),
            password: password.read().clone(),
        };
        let user = session
            .client
            .users()
            .create(&session.jwt, &request)
            .await
            .map_err(dioxus::CapturedError::from_display)?;
        navigator.push(R::user_route(user.id));
        Ok::<(), dioxus::CapturedError>(())
    });

    // Enter in any single-line field submits
    let submit_on_enter = move |event: KeyboardEvent| {
        if event.code() == Code::Enter && !*is_create_disabled.read() {
            create_action.call();
        }
    };

    rsx! {
        h1 { "New user" }
        match create_action.value() {
            Some(Err(err)) => rsx! {
                div { class: "alert alert-danger", role: "alert", "Failed to create user: {err}" }
            },
            _ => rsx! {}
        }
        div {
            class: "input-group mb-3",
            span { class: "input-group-text", "Username" }
            input {
                r#type: "text",
                class: "form-control",
                aria_label: "Username",
                autocomplete: "off",
                value: "{username}",
                oninput: move |event| username.set(event.value()),
                onkeyup: submit_on_enter,
            }
        }
        div {
            class: "input-group mb-3",
            span { class: "input-group-text", "First name" }
            input {
                r#type: "text",
                class: "form-control",
                aria_label: "First name",
                value: "{firstname}",
                oninput: move |event| firstname.set(event.value()),
                onkeyup: submit_on_enter,
            }
        }
        div {
            class: "input-group mb-3",
            span { class: "input-group-text", "Last name" }
            input {
                r#type: "text",
                class: "form-control",
                aria_label: "Last name",
                value: "{lastname}",
                oninput: move |event| lastname.set(event.value()),
                onkeyup: submit_on_enter,
            }
        }
        div {
            class: "input-group mb-3",
            span { class: "input-group-text", "Email" }
            input {
                r#type: "email",
                class: "form-control",
                aria_label: "Email",
                value: "{email}",
                oninput: move |event| email.set(event.value()),
                onkeyup: submit_on_enter,
            }
        }
        div {
            class: "input-group mb-3",
            span { class: "input-group-text", "Password" }
            input {
                r#type: "password",
                class: "form-control",
                aria_label: "Initial password",
                autocomplete: "new-password",
                value: "{password}",
                oninput: move |event| password.set(event.value()),
                onkeyup: submit_on_enter,
            }
        }
        div {
            class: "form-check mb-3",
            input {
                r#type: "checkbox",
                class: "form-check-input",
                id: "new-user-is-admin",
                checked: is_admin(),
                onchange: move |event| is_admin.set(event.checked()),
            }
            label { class: "form-check-label", r#for: "new-user-is-admin", "Administrator" }
        }
        div {
            class: "d-flex justify-content-end",
            button {
                class: "btn btn-primary",
                disabled: is_create_disabled,
                onclick: move |_| create_action.call(),
                "Create"
            }
        }
    }
}
