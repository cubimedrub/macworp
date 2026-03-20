use dioxus::router::{NavigationTarget, Routable};

pub trait IsRouter: Routable {
    fn project_new_route() -> NavigationTarget;
    fn project_index_route() -> NavigationTarget;
    fn project_route(id: i64) -> NavigationTarget;
}
