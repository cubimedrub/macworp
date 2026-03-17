mod group;
mod group_share;
mod project;
pub mod user;
mod user_share;
mod users_groups;
mod workflow;

// Re-exporting the models for easier access from other modules
pub use group::{Entity as GroupTable, Model as Group};
// pub use group_share::{
// Entity as GroupShareTable,
// Model as GroupShare
// };
pub use project::{Entity as ProjectTable, Model as Project};
pub use user::{Entity as UserTable, Model as User};
// pub use user_share::{
// Entity as UserShareTable,
// Model as UserShare
// };
pub use users_groups::{Entity as UsersGroupsTable, Model as UsersGroups};
pub use workflow::{Entity as WorkflowTable, Model as Workflow};
