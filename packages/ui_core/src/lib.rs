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
/// Defintion of a router
pub mod router;
/// Container runtime status bar (only shown if the platform provides a handle)
pub mod runtime_status;
/// Trait defining how to store data, e.g. login etc.
pub mod storage;

/// The common favicon
pub const FAVICON: Asset = asset!("/assets/favicon.ico");
/// Shared CSS
pub const MAIN_CSS: Asset = asset!("/assets/sass/main.sass");
