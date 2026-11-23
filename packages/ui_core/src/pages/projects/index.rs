use dioxus::prelude::*;
use macworp_core::responses::ProjectResponse;
use thiserror::Error;

use crate::{
    client::Client,
    components::{OwnerlessBadge, ShowAll, ShowAllToggle, Spinner},
    configuration::Configuration,
    router::IsRouter,
    sharing::SharingEnabled,
    storage::IsStorage,
};

#[derive(Debug, Error)]
pub enum Error<S: IsStorage> {
    #[error("Config not loaded")]
    ConfigNotLoaded,
    #[error("Failed to fetch projects: {0}")]
    FetchProjects(#[from] crate::client::Error),
    #[error("Not logged in")]
    NotLoggedIn,
    #[error("{0}")]
    Storage(S::Error),
}

#[component]
pub fn Index<S: IsStorage + 'static, R: IsRouter>() -> Element {
    let storage = use_context::<Signal<S>>();
    let config = use_context::<Signal<Configuration>>();

    let show_all = ShowAll::<S>::from_context();
    let mut projects: Resource<Result<Vec<ProjectResponse>, Error<S>>> =
        use_resource(move || async move {
            let jwt = storage
                .read()
                .jwt()
                .map_err(Error::Storage)?
                .ok_or(Error::NotLoggedIn)?;

            let config = config.read().clone();
            let all = show_all.is_active();

            Client::new(config.base_url().to_string(), config.timeout())
                .projects()
                .projects(&jwt, all)
                .await
                .map_err(Error::FetchProjects)
        });

    rsx! {
        div {
            class: "d-flex justify-content-between mb-3",
            h1 {
                "Projects"
            }
            div {
                class: "d-flex",
                ShowAllToggle::<S> {}
                Link {
                    class: "btn btn-primary h-fit-content",
                    to: R::project_new_route(),
                    i {
                        class: "fas fa-plus"
                    }
                }
            }
        }
        match &*projects.read() {
            Some(Ok(projects)) => rsx! {
                ul {
                    class: "list-group",
                    for project in projects {
                        Link {
                            class: "list-group-item",
                            to: R::project_route(project.id),
                            "{project.name}"
                            if SharingEnabled::from_context().is_enabled() {
                                if project.ownerless {
                                    OwnerlessBadge {}
                                } else if !project.owner_username.is_empty() {
                                    span { class: "badge text-bg-light ms-2", "owner: {project.owner_username}" }
                                }
                            }
                            small {
                                class: "text-muted ms-2",
                                {project.created_at.format("%Y-%m-%d %H:%M").to_string()}
                            }
                        }
                    }
                }
            },
            Some(Err(err)) => rsx! {
                div {
                    class: "alert alert-danger",
                    ondoubleclick: move |_| projects.restart(),
                    "{err}"
                }
            },
            None => rsx! {
                if projects.pending() {
                    Spinner {}
                }
            },
        }
    }
}
