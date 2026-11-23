use std::path::PathBuf;

use dioxus::prelude::*;
use macworp_ui_core::{client::Client, configuration::Configuration, storage::IsStorage};
use thiserror::Error;

use crate::{router::Router, storage::Storage};

#[derive(Debug, Error)]
pub enum Error {
    #[error("Failed to fetch projects: {0}")]
    CreateProject(#[from] macworp_ui_core::client::Error),
    #[error("Not logged in")]
    NotLoggedIn,
    #[error("{0}")]
    Storage(<Storage as IsStorage>::Error),
}

#[component]
pub fn New() -> Element {
    let storage = use_context::<Signal<Storage>>();
    let config = use_context::<Signal<Configuration>>();

    let mut project_name = use_signal(String::new);
    let mut project_path = use_signal(PathBuf::new);
    let project_path_displayable =
        use_memo(move || project_path.read().to_string_lossy().to_string());

    let is_create_button_disabled = use_memo(move || {
        project_name.read().is_empty() || project_path.read().as_os_str().is_empty()
    });

    let mut new_project = use_action(move || async move {
        let project_name = project_name.read().clone();
        let project_path = project_path.read().clone();

        if project_name.is_empty() || project_path.as_os_str().is_empty() {
            return Ok::<(), Error>(());
        }

        let jwt = storage
            .read()
            .jwt()
            .map_err(Error::Storage)?
            .ok_or(Error::NotLoggedIn)?;

        let config = config.read().clone();

        let project = Client::new(config.base_url().to_string(), config.timeout())
            .projects()
            .new_project(&jwt, project_name, project_path)
            .await
            .map_err(Error::CreateProject)?;

        let nav = navigator();

        nav.push(Router::ProjectEdit { id: project.id });

        Ok(())
    });

    rsx! {
        h1 {
            "New Project"
        }
        match new_project.value() {
            Some(Err(err)) => rsx! {
                div { class: "alert alert-danger", role: "alert", "Failed to create project: {err}" }
            },
            _ => rsx! {}
        }
        div {
            class: "input-group mb-3",
            span {
                class: "input-group-text",
                id: "project-name",
                "Name"
            }
            input {
                type: "text",
                class: "form-control",
                placeholder: "Project name",
                aria_label: "Project name",
                aria_describedby: "project-name",
                value: "{project_name}",
                oninput: move |event| project_name.set(event.value())
            }
        }
        div {
            class: "input-group mb-3",
            span {
                class: "input-group-text",
                "Project path"
            }
            input {
                type: "text",
                class: "form-control",
                id: "project-path-display",
                disabled: true,
                value: "{project_path_displayable}"
            }
            input {
                type: "file",
                id: "project-path",
                class: "visually-hidden w-0",
                directory: true,
                multiple: false,
                onchange: move |event| project_path.set(event.files().first().map(|file| file.path()).unwrap_or_default()),
            }
            label {
                class: "btn btn-secondary",
                for: "project-path",
                "browse"
            }
        }
        div {
            class: "d-flex justify-content-end",
            button {
                class: "btn btn-primary",
                disabled: is_create_button_disabled,
                onclick: move |_| new_project.call(),
                "Create"
            }
        }
    }
}
