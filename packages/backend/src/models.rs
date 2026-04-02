mod group;
mod group_share;
mod project;
pub mod user;
mod user_share;
mod users_groups;
mod workflow_integration;

// Re-exporting the models for easier access from other modules
pub use group::{ActiveModel as ActiveGroup, Entity as GroupTable, Model as Group};
// pub use group_share::{
// Entity as GroupShareTable,
// Model as GroupShare, ActiveModel as ActiveGroupShare
// };
pub use project::{
    ActiveModel as ActiveProject, Column as ProjectColumn, Entity as ProjectTable, Model as Project,
};
pub use user::{ActiveModel as ActiveUser, Entity as UserTable, Model as User};
// pub use user_share::{
// Entity as UserShareTable,
// Model as UserShare, ActiveModel as ActiveUserShare
// };
pub use users_groups::{
    ActiveModel as ActiveUsersGroups, Entity as UsersGroupsTable, Model as UsersGroups,
};
pub use workflow_integration::{
    ActiveModel as ActiveWorkflowIntegration, Column as WorkflowIntegrationColumn,
    Entity as WorkflowIntegrationTable, Model as WorkflowIntegration,
};
