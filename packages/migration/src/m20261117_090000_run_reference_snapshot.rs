use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

/// Name of the point-in-time snapshot of the global reference data a run reads, `NULL` for runs
/// that read the live reference folder (desktop, runs from before snapshots).
#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table("runs")
                    .add_column(string_null("reference_snapshot"))
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table("runs")
                    .drop_column("reference_snapshot")
                    .to_owned(),
            )
            .await
    }
}
