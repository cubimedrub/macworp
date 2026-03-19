/// Shared code for authentication
pub mod authentication;
/// Entities to work with. These are basically clones of the backend's models
/// without database access but (de-)serializable for easy data exchange between backend and frontends.
pub mod entities;
/// Core errors
pub mod error;
/// Various functionality to logging/tracing
pub mod logging;
/// Requests to the backend
pub mod requests;
/// Responses from the backend
pub mod responses;
/// Routing segments of controllers
pub mod routing;
