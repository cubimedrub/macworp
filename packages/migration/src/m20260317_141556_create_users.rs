use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table("users")
                    .if_not_exists()
                    .col(big_pk_auto("id"))
                    .col(string_len_uniq("username", 128))
                    .col(string_len("firstname", 128))
                    .col(string_len("lastname", 128))
                    .col(string_len_uniq("email", 128))
                    .col(string_null("password"))
                    .col(string_null("access_token"))
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table("users").to_owned())
            .await
    }
}
