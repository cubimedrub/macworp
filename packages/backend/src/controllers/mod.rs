mod groups_controller;
mod login_controller;
mod projects_controller;
mod runs_controller;
mod shares_controller;
mod users_controller;
mod utilities_controller;
mod worker_controller;
mod workflow_integrations_controllers;

// Re-export controllers for easier access
pub use groups_controller::GroupsController;
pub use login_controller::LoginController;
pub use projects_controller::ProjectsController;
pub use runs_controller::RunsController;
pub use shares_controller::SharesController;
pub use users_controller::UsersController;
pub use utilities_controller::UtilitiesController;
pub use worker_controller::WorkerController;
pub use workflow_integrations_controllers::WorkflowIntegrationsController;

/// HTTP-level authorization tests over the full router
#[cfg(all(test, feature = "integration"))]
mod authorization_http_tests;

/// HTTP-level tests of the worker protocol
#[cfg(all(test, feature = "integration"))]
mod worker_http_tests;
