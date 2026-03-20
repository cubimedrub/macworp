use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table("projects")
                    .if_not_exists()
                    .col(big_pk_auto("id"))
                    .col(big_integer("owner_id"))
                    .col(text("path").null()) // this used for integrated features only
                    .col(string_len_uniq("name", 512))
                    .foreign_key(
                        ForeignKey::create()
                            .from("projects", "owner_id")
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
            .drop_table(Table::drop().table("projects").to_owned())
            .await
    }
}
