use dioxus::prelude::{asset, Asset};

/// Client for communicating with backend
pub mod client;
/// Reusable components for UI
pub mod components;
/// UI configuration
pub mod configuration;
/// App layouts
pub mod layouts;
/// Pages showing a single task (a resource, resource list, resource edit, ...)
pub mod pages;
/// Routes to navigate
pub mod routes;

/// The common favicon
pub const FAVICON: Asset = asset!("/assets/favicon.ico");
/// Shared CSS
pub const MAIN_CSS: Asset = asset!("/assets/sass/main.sass");
