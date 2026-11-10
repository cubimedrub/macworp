use dioxus::prelude::*;

use macworp_ui_core::{
    layouts::{one_pane::OnePane, two_pane::TwoPane},
    pages::{
        error::Error, groups::*, projects::*, reference::ReferenceEdit, users::*,
        workflow_integrations::*,
    },
    router::IsRouter,
};

use crate::{
    pages::{
        projects::{edit::FileImmutableProjectEdit, ProjectNew},
        settings::SettingsPage,
    },
    storage::Storage,
};

#[derive(Debug, Clone, Routable, PartialEq)]
#[rustfmt::skip]
#[allow(clippy::enum_variant_names)]
pub enum Router {
    #[layout(OnePane<Router>)]
        #[route("/error?:message", Error<Router>)]
        ErrorPage { message: String }, // need to call it error page due ambiguity

    #[layout(TwoPane<Router, Storage>)]
        #[route("/", ProjectIndex<Storage, Router>)]
        ProjectIndex,
        #[route("/projects/new")]
        ProjectNew,
        #[route("/projects/:id", ProjectEdit<FileImmutableProjectEdit>)]
        ProjectEdit{ id: i64 },

        #[route("/settings", SettingsPage)]
        SettingsPage,

        #[route("/reference", ReferenceEdit<FileImmutableProjectEdit>)]
        ReferenceEdit,

        #[route("/workflow-integrations", WorkflowIntegrationIndex<Storage, Router>)]
        WorkflowIntegrationIndex,
        #[route("/workflow-integrations/new", WorkflowIntegrationNew<Storage, Router>)]
        WorkflowIntegrationNew,
        #[route("/workflow-integrations/:id", WorkflowIntegrationEdit<Storage, Router>)]
        WorkflowIntegrationEdit{ id: i64 },

        #[route("/groups", GroupIndex<Storage, Router>)]
        GroupIndex,
        #[route("/groups/new", GroupNew<Storage, Router>)]
        GroupNew,
        #[route("/groups/:id", GroupEdit<Storage, Router>)]
        GroupEdit{ id: i64 },

        #[route("/users", UserIndex<Storage, Router>)]
        UserIndex,
        #[route("/users/new", UserNew<Storage, Router>)]
        UserNew,
        #[route("/users/:id", UserEdit<Storage, Router>)]
        UserEdit{ id: i64 },
}

impl IsRouter for Router {
    const LOGIN_VARIANT: Self = Self::ProjectIndex;

    fn project_new_route() -> NavigationTarget<Self> {
        NavigationTarget::Internal(Router::ProjectNew)
    }

    fn project_index_route() -> NavigationTarget<Self> {
        NavigationTarget::Internal(Router::ProjectIndex)
    }

    fn project_route(id: i64) -> NavigationTarget<Self> {
        NavigationTarget::Internal(Router::ProjectEdit { id })
    }

    fn settings_route() -> Option<NavigationTarget<Self>> {
        Some(NavigationTarget::Internal(Router::SettingsPage))
    }

    fn reference_route() -> NavigationTarget<Self> {
        NavigationTarget::Internal(Router::ReferenceEdit)
    }

    fn login_route(_message: Option<String>) -> NavigationTarget<Self> {
        Self::project_index_route()
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

    fn group_index_route() -> NavigationTarget<Self> {
        NavigationTarget::Internal(Router::GroupIndex)
    }

    fn group_new_route() -> NavigationTarget<Self> {
        NavigationTarget::Internal(Router::GroupNew)
    }

    fn group_route(id: i64) -> NavigationTarget<Self> {
        NavigationTarget::Internal(Router::GroupEdit { id })
    }

    fn user_index_route() -> NavigationTarget<Self> {
        NavigationTarget::Internal(Router::UserIndex)
    }

    fn user_new_route() -> NavigationTarget<Self> {
        NavigationTarget::Internal(Router::UserNew)
    }

    fn user_route(id: i64) -> NavigationTarget<Self> {
        NavigationTarget::Internal(Router::UserEdit { id })
    }
}
