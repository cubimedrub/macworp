use macworp_core::workflow_configuration::WorkflowConfiguration;
use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table("workflow_integrations")
                    .if_not_exists()
                    .col(big_pk_auto("id"))
                    .col(big_integer("owner_id"))
                    .col(string("name"))
                    .col(string("description"))
                    .col(json_binary("configuration").default(WorkflowConfiguration::default()))
                    .foreign_key(
                        ForeignKey::create()
                            .from("workflow_integrations", "owner_id")
                            .to("users", "id")
                            .on_delete(ForeignKeyAction::Cascade)
                            .on_update(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table("workflow_integrations").to_owned())
            .await
    }
}
