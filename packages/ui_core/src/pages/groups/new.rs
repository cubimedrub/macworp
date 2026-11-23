use dioxus::prelude::*;

use crate::{configuration::Configuration, router::IsRouter, session::Session, storage::IsStorage};

/// Form to create a group; the creator becomes its owner.
#[component]
pub fn New<S: IsStorage + 'static, R: IsRouter + 'static>() -> Element {
    let storage = use_context::<Signal<S>>();
    let config = use_context::<Signal<Configuration>>();
    let navigator = navigator();

    let mut name = use_signal(String::new);
    let mut description = use_signal(String::new);
    let is_create_disabled = use_memo(move || name.read().trim().is_empty());

    let mut create_action = use_action(move || async move {
        let session = Session::new(storage, config).map_err(dioxus::CapturedError::from_display)?;
        let (name, description) = (name.read().clone(), description.read().clone());
        let group = session
            .client
            .groups()
            .create(&session.jwt, name, description)
            .await
            .map_err(dioxus::CapturedError::from_display)?;
        navigator.push(R::group_route(group.id));
        Ok::<(), dioxus::CapturedError>(())
    });

    rsx! {
        h1 { "New group" }
        match create_action.value() {
            Some(Err(err)) => rsx! {
                div { class: "alert alert-danger", role: "alert", "Failed to create group: {err}" }
            },
            _ => rsx! {}
        }
        div {
            class: "input-group mb-3",
            span { class: "input-group-text", "Name" }
            input {
                r#type: "text",
                class: "form-control",
                placeholder: "Group name",
                aria_label: "Group name",
                value: "{name}",
                oninput: move |event| name.set(event.value()),
                // Enter submits; the description is multi-line, so it keeps Enter for line breaks
                onkeyup: move |event| {
                    if event.code() == Code::Enter && !*is_create_disabled.read() {
                        create_action.call();
                    }
                },
            }
        }
        div {
            class: "input-group mb-3",
            span { class: "input-group-text", "Description" }
            textarea {
                class: "form-control",
                placeholder: "Group description",
                aria_label: "Group description",
                value: "{description}",
                oninput: move |event| description.set(event.value()),
            }
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
