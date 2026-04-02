use dioxus::prelude::*;
use thiserror::Error;

use crate::{client::Client, configuration::Configuration, router::IsRouter, storage::IsStorage};

#[derive(Debug, Error)]
pub enum Error<S: IsStorage> {
    #[error("Failed to fetch projects: {0}")]
    CreateProject(#[from] crate::client::Error),
    #[error("Not logged in")]
    NotLoggedIn,
    #[error("{0}")]
    Storage(S::Error),
}

#[component]
pub fn New<S: IsStorage + 'static, R: IsRouter + 'static>() -> Element {
    let storage = use_context::<Signal<S>>();
    let config = use_context::<Signal<Configuration>>();

    let mut workflow_integration_name = use_signal(String::new);
    let mut workflow_integration_description = use_signal(String::new);
    let is_create_button_disabled = use_memo(move || workflow_integration_name.read().is_empty());

    let mut create_action = use_action(move || async move {
        let workflow_integration_name = workflow_integration_name.read().clone();
        let workflow_integration_description = workflow_integration_description.read().clone();

        if workflow_integration_name.is_empty() {
            return Ok::<(), Error<S>>(());
        }

        let jwt = storage
            .read()
            .jwt()
            .map_err(Error::Storage)?
            .ok_or(Error::NotLoggedIn)?;

        let config = config.read().clone();

        let new_workflow_integration = Client::new(config.base_url().to_string(), config.timeout())
            .workflow_integrations()
            .new_workflow_integration(
                &jwt,
                workflow_integration_name,
                workflow_integration_description,
            )
            .await
            .map_err(Error::CreateProject)?;

        let nav = navigator();

        nav.push(R::workflow_integration_route(new_workflow_integration.id))
            .unwrap();

        Ok(())
    });

    rsx! {
        h1 {
            "New workflow integration"
        }
        match create_action.value() {
            Some(Err(err)) => rsx! {
                div { class: "alert alert-danger", role: "alert", "Failed to create workflow integration: {err}" }
            },
            _ => rsx! {}
        }
        div {
            class: "input-group mb-3",
            span {
                class: "input-group-text",
                id: "workflow-integration-name",
                "Name"
            }
            input {
                type: "text",
                class: "form-control",
                placeholder: "Workflow integration name",
                aria_label: "Workflow integration name",
                aria_describedby: "workflow-integration-name",
                value: "{workflow_integration_name}",
                oninput: move |event| workflow_integration_name.set(event.value()),
                onkeyup: move |event| {
                    if event.code() == Code::Enter {
                        create_action.call();
                    }
                }
            }
        }
        div {
            class: "input-group mb-3",
            span {
                class: "input-group-text",
                id: "workflow-integration-description",
                "Description"
            }
            textarea {
                class: "form-control",
                placeholder: "Workflow integration description",
                aria_label: "Workflow integration description",
                value: "{workflow_integration_description}",
                oninput: move |event| workflow_integration_description.set(event.value()),
                onkeyup: move |event| {
                    if event.code() == Code::Enter {
                        create_action.call();
                    }
                }
            }
        }
        div {
            class: "d-flex justify-content-end",
            button {
                class: "btn btn-primary",
                disabled: is_create_button_disabled,
                onclick: move |_| create_action.call(),
                "Create"
            }
        }
    }
}
