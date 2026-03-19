mod group;
mod group_share;
mod project;
pub mod user;
mod user_share;
mod users_groups;
mod workflow;

// Re-exporting the models for easier access from other modules
pub use group::{ActiveModel as ActiveGroup, Entity as GroupTable, Model as Group};
// pub use group_share::{
// Entity as GroupShareTable,
// Model as GroupShare, ActiveModel as ActiveGroupShare
// };
pub use project::{ActiveModel as ActiveProject, Entity as ProjectTable, Model as Project};
pub use user::{ActiveModel as ActiveUser, Entity as UserTable, Model as User};
// pub use user_share::{
// Entity as UserShareTable,
// Model as UserShare, ActiveModel as ActiveUserShare
// };
pub use users_groups::{
    ActiveModel as ActiveUsersGroups, Entity as UsersGroupsTable, Model as UsersGroups,
};
pub use workflow::{ActiveModel as ActiveWorkflow, Entity as WorkflowTable, Model as Workflow};
