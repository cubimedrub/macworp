use dioxus::prelude::*;

use macworp_ui_core::{
    layouts::{one_pane::OnePane, two_pane::TwoPane},
    pages::{error::Error, projects::*, workflow_integrations::*},
    router::IsRouter,
};

use crate::{
    pages::{
        local_login::LocalLogin,
        login::Login,
        projects::{edit::FileMutableProjectEdit, ProjectNew},
    },
    storage::Storage,
};

#[derive(Debug, Clone, Routable, PartialEq)]
#[rustfmt::skip]
#[allow(clippy::enum_variant_names)]
pub enum Router {
    #[layout(OnePane<Router>)]
        #[route("/login?:message", Login)]
        Login { message: Option<String> },
        #[route("/login/local")]
        LocalLogin,
        #[route("/error?:message", Error<Router>)]
        ErrorPage { message: String }, // need to call it error page due ambiguity

    #[layout(TwoPane<Router, Storage>)]
        #[route("/", ProjectIndex<Storage, Router>)]
        ProjectIndex,
        #[route("/projects/new")]
        ProjectNew,
        #[route("/projects/:id", ProjectEdit<FileMutableProjectEdit>)]
        ProjectEdit{ id: i64 },

        #[route("/workflow-integrations", WorkflowIntegrationIndex<Storage, Router>)]
        WorkflowIntegrationIndex,
        #[route("/workflow-integrations/new", WorkflowIntegrationNew<Storage, Router>)]
        WorkflowIntegrationNew,
        #[route("/workflow-integrations/:id", WorkflowIntegrationEdit<Storage, Router>)]
        WorkflowIntegrationEdit{ id: i64 },
}

impl IsRouter for Router {
    const LOGIN_VARIANT: Self = Self::Login { message: None };

    fn project_new_route() -> NavigationTarget<Self> {
        NavigationTarget::Internal(Router::ProjectNew)
    }

    fn project_index_route() -> NavigationTarget<Self> {
        NavigationTarget::Internal(Router::ProjectIndex)
    }

    fn project_route(id: i64) -> NavigationTarget<Self> {
        NavigationTarget::Internal(Router::ProjectEdit { id })
    }

    fn login_route(message: Option<String>) -> NavigationTarget<Self> {
        NavigationTarget::Internal(Router::Login { message })
    }

    fn error_route(message: String) -> NavigationTarget<Self> {
        NavigationTarget::Internal(Router::ErrorPage { message })
    }

    fn workflow_integration_new_route() -> NavigationTarget<Self> {
        NavigationTarget::Internal(Router::WorkflowIntegrationNew)
    }

    fn workflow_integration_index_route() -> NavigationTarget<Self> {
        NavigationTarget::Internal(Router::WorkflowIntegrationIndex)
    }

    fn workflow_integration_route(id: i64) -> NavigationTarget<Self> {
        NavigationTarget::Internal(Router::WorkflowIntegrationEdit { id })
    }
}
