use dioxus::prelude::*;

use macworp_ui_core::{
    components::file_browser::ImmutableFileBrowser,
    layouts::{one_pane::OnePane, two_pane::TwoPane},
    pages::{error::Error, projects::*},
    router::IsRouter,
};

use crate::{
    pages::projects::{edit::FileImmutableProjectEdit, ProjectNew},
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
        #[route("/projects/:id", ProjectEdit<FileImmutableProjectEdit, ImmutableFileBrowser, Storage, Router>)]
        ProjectEdit{ id: i64 },
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

    fn login_route(_message: Option<String>) -> NavigationTarget<Self> {
        Self::project_index_route()
    }

    fn error_route(message: String) -> NavigationTarget<Self> {
        NavigationTarget::Internal(Router::ErrorPage { message })
    }
}
