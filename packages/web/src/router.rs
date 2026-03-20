use dioxus::prelude::*;

use macworp_ui_core::{layouts::two_pane::TwoPane, pages::projects::*, router::IsRouter};

use crate::{pages::projects::ProjectNew, storage::Storage};

#[derive(Debug, Clone, Routable, PartialEq)]
#[rustfmt::skip]
#[allow(clippy::enum_variant_names)]
pub enum Router {
    #[layout(TwoPane<Router>)]
        #[route("/", ProjectIndex<Storage, Router>)]
        ProjectIndex,
        #[route("/projects/new")]
        ProjectNew,
        #[route("/projects/:id", ProjectEdit<Storage>)]
        ProjectEdit{ id: i64 },
}

impl IsRouter for Router {
    fn project_new_route() -> NavigationTarget {
        NavigationTarget::Internal(Router::ProjectNew.to_string())
    }

    fn project_index_route() -> NavigationTarget {
        NavigationTarget::Internal(Router::ProjectIndex.to_string())
    }

    fn project_route(id: i64) -> NavigationTarget {
        NavigationTarget::Internal(Router::ProjectEdit { id }.to_string())
    }
}
