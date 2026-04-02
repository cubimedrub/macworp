use dioxus::router::{NavigationTarget, Routable};

pub trait IsRouter: Routable {
    const LOGIN_VARIANT: Self;

    fn project_new_route() -> NavigationTarget<Self>;
    fn project_index_route() -> NavigationTarget<Self>;
    fn project_route(id: i64) -> NavigationTarget<Self>;
    fn login_route(message: Option<String>) -> NavigationTarget<Self>;
    fn error_route(message: String) -> NavigationTarget<Self>;
    fn workflow_integration_new_route() -> NavigationTarget<Self>;
    fn workflow_integration_index_route() -> NavigationTarget<Self>;
    fn workflow_integration_route(id: i64) -> NavigationTarget<Self>;
}
