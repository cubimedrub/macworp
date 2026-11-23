pub mod edit;
pub mod index;
pub mod new;

// Re-export pages for easier access
pub use edit::Edit as WorkflowIntegrationEdit;
pub use index::Index as WorkflowIntegrationIndex;
pub use new::New as WorkflowIntegrationNew;
