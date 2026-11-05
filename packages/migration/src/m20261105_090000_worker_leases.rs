use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

/// Lease bookkeeping for runs claimed by remote workers.
#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // One column per statement: SQLite cannot add several columns in one ALTER TABLE
        manager
            .alter_table(
                Table::alter()
                    .table("runs")
                    // Worker must heartbeat before this, otherwise the run is requeued
                    .add_column(timestamp_with_time_zone_null("leased_until"))
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table("runs")
                    .add_column(string_len_null("worker_name", 128))
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table("runs")
                    // SHA-256 (hex) of the per-run token handed to the worker at claim
                    .add_column(string_len_null("worker_token_hash", 64))
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table("runs")
                    // How often the run was claimed, bounded by the configured maximum
                    .add_column(integer("attempts").default(0))
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_runs_leased_until")
                    .table("runs")
                    .col("leased_until")
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_index(
                Index::drop()
                    .name("idx_runs_leased_until")
                    .table("runs")
                    .to_owned(),
            )
            .await?;
        for column in [
            "leased_until",
            "worker_name",
            "worker_token_hash",
            "attempts",
        ] {
            manager
                .alter_table(Table::alter().table("runs").drop_column(column).to_owned())
                .await?;
        }
        Ok(())
    }
}
