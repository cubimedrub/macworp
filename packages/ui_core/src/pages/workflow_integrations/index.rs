use dioxus::prelude::*;
use macworp_core::responses::WorkflowIntegrationResponse;
use thiserror::Error;

use crate::{
    client::Client, components::Spinner, configuration::Configuration, router::IsRouter,
    runtime_status::engine_hint, sharing::SharingEnabled, storage::IsStorage,
};

#[derive(Debug, Error)]
pub enum Error<S: IsStorage> {
    #[error("Config not loaded")]
    ConfigNotLoaded,
    #[error("Failed to fetch workflow integrations: {0}")]
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

    let mut workflow_integrations: Resource<Result<Vec<WorkflowIntegrationResponse>, Error<S>>> =
        use_resource(move || async move {
            let jwt = storage
                .read()
                .jwt()
                .map_err(Error::Storage)?
                .ok_or(Error::NotLoggedIn)?;

            let config = config.read().clone();

            Client::new(config.base_url().to_string(), config.timeout())
                .workflow_integrations()
                .workflow_integrations(&jwt, false)
                .await
                .map_err(Error::FetchProjects)
        });

    rsx! {
        div {
            class: "d-flex justify-content-between mb-3",
            h1 {
                "Workflow integrations"
            }
            Link {
                class: "btn btn-primary h-fit-content",
                to: R::workflow_integration_new_route(),
                i {
                    class: "fas fa-plus"
                }
            }
        }
        match &*workflow_integrations.read() {
            Some(Ok(workflow_integrations)) => rsx! {
                ul {
                    class: "list-group",
                    for workflow_integration in workflow_integrations {
                        Link {
                            class: "list-group-item",
                            to: R::workflow_integration_route(workflow_integration.id),
                            "{workflow_integration.name}"
                            if SharingEnabled::from_context().is_enabled() {
                                span { class: "badge text-bg-light ms-2", "{workflow_integration.owner_username}" }
                            }
                            if let Some(hint) = engine_hint(workflow_integration.configuration().engine()) {
                                span {
                                    class: "badge text-bg-warning ms-2",
                                    title: "{hint}",
                                    "unavailable"
                                }
                            }
                        }
                    }
                }
            },
            Some(Err(err)) => rsx! {
                div {
                    class: "alert alert-danger",
                    ondoubleclick: move |_| workflow_integrations.restart(),
                    "{err}"
                }
            },
            None => rsx! {
                Spinner {}
            },
        }
    }
}
