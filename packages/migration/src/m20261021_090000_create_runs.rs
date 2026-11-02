use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table("runs")
                    .if_not_exists()
                    .col(big_pk_auto("id"))
                    .col(big_integer("project_id"))
                    .col(big_integer_null("workflow_id"))
                    // NULL once the user who started the run was deleted
                    .col(big_integer_null("started_by"))
                    // Snapshots: runs stay reproducible after the workflow is edited or deleted
                    .col(json_binary("configuration"))
                    .col(json_binary("parameters"))
                    .col(text("comment").default(""))
                    .col(string_len("status", 16).default("queued"))
                    .col(integer_null("exit_code"))
                    .col(text_null("error"))
                    // Progress reported by the engine: tasks seen so far (grows during the run) and finished
                    .col(integer("tasks_total").default(0))
                    .col(integer("tasks_done").default(0))
                    // Latest progress detail, e.g. the task that just changed state
                    .col(text_null("progress_message"))
                    // Set while a running run should be stopped, the worker moves it to cancelled
                    .col(boolean("cancel_requested").default(false))
                    // Locked runs are never evicted by the per-project retention limit
                    .col(boolean("locked").default(false))
                    .col(timestamp_with_time_zone("created_at").default(Expr::current_timestamp()))
                    .col(timestamp_with_time_zone_null("started_at"))
                    .col(timestamp_with_time_zone_null("finished_at"))
                    .foreign_key(
                        ForeignKey::create()
                            .from("runs", "project_id")
                            .to("projects", "id")
                            .on_delete(ForeignKeyAction::Cascade)
                            .on_update(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .from("runs", "workflow_id")
                            .to("workflow_integrations", "id")
                            .on_delete(ForeignKeyAction::SetNull)
                            .on_update(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .from("runs", "started_by")
                            .to("users", "id")
                            .on_delete(ForeignKeyAction::SetNull)
                            .on_update(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_runs_project_created")
                    .table("runs")
                    .col("project_id")
                    .col("created_at")
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_runs_status")
                    .table("runs")
                    .col("status")
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table("run_logs")
                    .if_not_exists()
                    .col(big_pk_auto("id"))
                    .col(big_integer("run_id"))
                    .col(big_integer("seq"))
                    .col(string_len("stream", 16))
                    .col(timestamp_with_time_zone("created_at").default(Expr::current_timestamp()))
                    // Batch of consecutive lines of one stream, newline separated
                    .col(text("content"))
                    .col(integer("line_count"))
                    .foreign_key(
                        ForeignKey::create()
                            .from("run_logs", "run_id")
                            .to("runs", "id")
                            .on_delete(ForeignKeyAction::Cascade)
                            .on_update(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        // Resume cursor for streaming: `WHERE run_id = ? AND seq > ?` (seq counts chunks)
        manager
            .create_index(
                Index::create()
                    .name("idx_run_logs_run_seq")
                    .table("run_logs")
                    .col("run_id")
                    .col("seq")
                    .unique()
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table("run_logs").to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table("runs").to_owned())
            .await
    }
}
