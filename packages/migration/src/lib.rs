pub use sea_orm_migration::prelude::*;

mod m20260317_141556_create_users;
mod m20260319_113549_create_projects;
mod m20260331_114655_create_workflow_integrations;
mod m20261021_090000_create_runs;
mod m20261027_090000_create_groups_shares;
mod m20261105_090000_worker_leases;
mod m20261110_090000_create_reference_data;
mod m20261112_090000_run_provenance;
mod m20261117_090000_run_reference_snapshot;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20260317_141556_create_users::Migration),
            Box::new(m20260319_113549_create_projects::Migration),
            Box::new(m20260331_114655_create_workflow_integrations::Migration),
            Box::new(m20261021_090000_create_runs::Migration),
            Box::new(m20261027_090000_create_groups_shares::Migration),
            Box::new(m20261105_090000_worker_leases::Migration),
            Box::new(m20261110_090000_create_reference_data::Migration),
            Box::new(m20261112_090000_run_provenance::Migration),
            Box::new(m20261117_090000_run_reference_snapshot::Migration),
        ]
    }
}
