/// Explains what deleting a user account does to what it owns
pub mod account_deletion_rules;
pub mod bootstrap;
pub mod confirmation_dialog;
pub mod file_browser;
/// Badge for resources without an owner
pub mod ownerless_badge;
pub mod projects;
/// Runs of a project: list, progress bar and logs
pub mod runs;
pub mod separator;
/// Share dialog for projects and workflows
pub mod sharing;
/// Admin switch to list everything
pub mod show_all_toggle;
pub mod spiner;
/// Components for workflow integration, e.g. the dynamic paramters.
pub mod workflow_integrations;

// Re-export components
pub use account_deletion_rules::AccountDeletionRules;
pub use bootstrap::Bootstrap;
pub use confirmation_dialog::ConfirmationDialog;
pub use file_browser::FileBrowser;
pub use ownerless_badge::OwnerlessBadge;
pub use show_all_toggle::{ShowAll, ShowAllToggle};
pub use spiner::Spinner;
