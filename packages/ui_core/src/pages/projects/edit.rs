use dioxus::prelude::*;
use macworp_core::responses::ProjectResponse;
use thiserror::Error;

use crate::{
    client::Client, components::Spinner, configuration::Configuration, storage::IsStorage,
};

#[derive(Debug, Error)]
pub enum Error<S: IsStorage> {
    #[error("Config not loaded")]
    ConfigNotLoaded,
    #[error("Failed to fetch projects: {0}")]
    FetchProject(#[from] crate::client::Error),
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
pub fn Edit<S: IsStorage + 'static>(props: EditProps) -> Element {
    let storage = use_context::<Signal<S>>();
    let config = use_context::<Signal<Configuration>>();
    let navigator = navigator();

    let mut subpath: Signal<Option<String>> = use_signal(|| None);

    let project: Resource<Result<ProjectResponse, Error<S>>> = use_resource(move || async move {
        let jwt = storage
            .read()
            .jwt()
            .map_err(Error::Storage)?
            .ok_or(Error::NotLoggedIn)?;

        let subpath = subpath.read().clone();

        let config = config.read().clone();

        Client::new(config.base_url().to_string(), config.timeout())
            .projects()
            .project(&jwt, props.id, subpath)
            .await
            .map_err(Error::FetchProject)
    });

    let mut delete_project = use_action(move || async move {
        let jwt = storage
            .read()
            .jwt()
            .map_err(Error::Storage)?
            .ok_or(Error::NotLoggedIn)?;

        let config = config.read().clone();

        Client::new(
            config.read().base_url().to_string(),
            config.read().timeout(),
        )
        .projects()
        .project(&jwt, props.id, subpath)
        .await
        .map_err(Error::FetchProject)
    });

    rsx! {
        div {
            match &*project.read() {
                Some(Ok(project)) => rsx!(
                    h1 {
                        "Project: {project.name}"
                    }
                    ul {
                        class: "list-group",
                        for folder in project.directories.iter().cloned() {
                            li {
                                class: "list-group-item",
                                onabort: move |_| {
                                    let folder_name = folder.clone();
                                    subpath.set(Some(folder_name));
                                },
                                i {
                                    class: "fas fa-folder me-2"
                                }
                                "{folder}"
                            }
                        }
                        for file in &project.files {
                            li {
                                class: "list-group-item",
                                i {
                                    class: "fas fa-file me-2"
                                }
                                "{file}"
                            }
                        }
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
