/// User page: edit, reset password, disable, delete
pub mod edit;
/// List of all users
pub mod index;
/// Create a user
pub mod new;

// Re-export pages for easier access
pub use edit::Edit as UserEdit;
pub use index::Index as UserIndex;
pub use new::New as UserNew;
