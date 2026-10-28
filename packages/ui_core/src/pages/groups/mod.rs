/// Group page: rename, members, delete
pub mod edit;
/// List of the user's groups
pub mod index;
/// Create a group
pub mod new;

// Re-export pages for easier access
pub use edit::Edit as GroupEdit;
pub use index::Index as GroupIndex;
pub use new::New as GroupNew;
