use std::path::PathBuf;

use dioxus::prelude::*;
use macworp_core::responses::ProjectResponse;
use thiserror::Error;

use crate::{
    client::Client,
    components::{file_browser::IsFileBrowser, ConfirmationDialog, FileBrowser, Spinner},
    configuration::Configuration,
    router::IsRouter,
    storage::IsStorage,
};

#[derive(Debug, Error)]
pub enum Error<S: IsStorage> {
    #[error("Config not loaded")]
    ConfigNotLoaded,
    #[error("Failed to fetch projects: {0}")]
    FetchProject(crate::client::Error),
    #[error("Failed to delete projects: {0}")]
    DeleteProject(crate::client::Error),
    #[error("Not logged in")]
    NotLoggedIn,
    #[error("{0}")]
    Storage(S::Error),
}

#[derive(Clone, PartialEq, Props)]
pub struct EditProps {
    /// Project ID to fetch
    pub id: i64,
}

#[component]
pub fn Edit<F: IsFileBrowser + 'static, S: IsStorage + 'static, R: IsRouter>(
    props: EditProps,
) -> Element {
    let storage = use_context::<Signal<S>>();
    let config = use_context::<Signal<Configuration>>();
    let navigator = navigator();

    let mut subpath: Signal<PathBuf> = use_signal(PathBuf::new);
    let mut is_delete_confirmation_dialog_open = use_signal(|| false);

    let project: Resource<Result<Signal<ProjectResponse>, Error<S>>> =
        use_resource(move || async move {
            let jwt = storage
                .read()
                .jwt()
                .map_err(Error::Storage)?
                .ok_or(Error::NotLoggedIn)?;

            let subpath = subpath.read().clone();

            let config = config.read().clone();

            let project = Client::new(config.base_url().to_string(), config.timeout())
                .projects()
                .project(&jwt, props.id, subpath)
                .await
                .map_err(Error::FetchProject)?;

            Ok(Signal::new(project))
        });

    let mut delete_project = use_action(move || async move {
        let jwt = storage
            .read()
            .jwt()
            .map_err(Error::Storage)?
            .ok_or(Error::NotLoggedIn)?;

        let config = config.read().clone();

        Client::new(config.base_url().to_string(), config.timeout())
            .projects()
            .delete(&jwt, props.id)
            .await
            .map_err(Error::DeleteProject)?;

        navigator.push(R::project_index_route());

        Ok::<_, Error<S>>(())
    });

    rsx! {
        div {
            ConfirmationDialog {
                is_open: is_delete_confirmation_dialog_open,
                title: "Delete project".to_string(),
                body: "Are you sure you want to delete this project? This action cannot be undone.".to_string(),
                confirmation_button_label: "Delete".to_string(),
                confirmation_callback: Callback::new(move |_| {
                    is_delete_confirmation_dialog_open.set(false);
                    delete_project.call();
                }),
                cancel_callback: Callback::new(move |_| is_delete_confirmation_dialog_open.set(false))
            }
            match &*project.read() {
                Some(Ok(project)) => rsx!(
                    div {
                        class: "logo d-flex flex-row justify-content-between mb-3",
                        h1 {
                            class: "mb-0",
                            "{project.read().name}"
                        }
                        button {
                            id: "collapse-btn",
                            class: "btn btn-sm btn-outline-danger",
                            title: "Collapse sidebar",
                            onclick: move |_| is_delete_confirmation_dialog_open.set(true),
                            i {
                                class: "fa-solid fa-trash me-1"
                            }
                            "Delete"
                        }
                    }
                    match delete_project.value() {
                        Some(Err(err)) => rsx! {
                            div {
                                class: "alert alert-danger",
                                role: "alert",
                                "Failed to delete project: {err}" }
                        },
                        _ => rsx! {}
                    }
                    FileBrowser<F> {
                        project: *project,
                        move_into_folder_callback: Callback::new(move |new_subpath| {
                            subpath.set(new_subpath);
                        })
                    }
                ),
                Some(Err(err)) => rsx!(
                    div {
                        class: "alert alert-danger",
                        "{err}"
                    }
                ),
                None => rsx!(
                    if project.pending() {
                        Spinner {}
                    }
                )
            }
        }
    }
}
