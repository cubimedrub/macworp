/// What happens to a user's resources when the account is deleted
pub mod account_deletion;
/// Shared code for authentication
pub mod authentication;
/// Entities to work with. These are basically clones of the backend's models
/// without database access but (de-)serializable for easy data exchange between backend and frontends.
pub mod entities;
/// Core errors
pub mod error;
/// HTTP tools
pub mod http;
/// Various functionality to logging/tracing
pub mod logging;
/// Lexical sanitising of in-project relative paths
pub mod paths;
/// Requests to the backend
pub mod requests;
/// Responses from the backend
pub mod responses;
/// Routing segments of controllers
pub mod routing;
/// Sample sheets: model, file formats, columns, editing
pub mod sample_sheet;
/// JSON Schema validation with the nf-schema extensions used by sample sheets
pub mod schema_validation;
/// Workflow configuration
pub mod workflow_configuration;
