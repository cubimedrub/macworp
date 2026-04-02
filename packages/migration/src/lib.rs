pub use sea_orm_migration::prelude::*;

mod m20260317_141556_create_users;
mod m20260319_113549_create_projects;
mod m20260331_114655_create_workflow_integrations;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20260317_141556_create_users::Migration),
            Box::new(m20260319_113549_create_projects::Migration),
            Box::new(m20260331_114655_create_workflow_integrations::Migration),
        ]
    }
}
