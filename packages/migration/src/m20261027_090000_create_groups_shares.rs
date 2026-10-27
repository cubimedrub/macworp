use sea_orm_migration::{prelude::*, schema::*};

/// Creates groups, group membership and the polymorphic shares table, and backfills shares for
/// existing projects and workflows
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table("groups")
                    .if_not_exists()
                    .col(big_pk_auto("id"))
                    .col(big_integer("owner_id"))
                    .col(string_len("name", 512))
                    .col(string("description").default(""))
                    .foreign_key(
                        ForeignKey::create()
                            .from("groups", "owner_id")
                            .to("users", "id")
                            .on_delete(ForeignKeyAction::Cascade)
                            .on_update(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table("users_groups")
                    .if_not_exists()
                    .col(big_integer("user_id"))
                    .col(big_integer("group_id"))
                    .primary_key(Index::create().col("user_id").col("group_id"))
                    .foreign_key(
                        ForeignKey::create()
                            .from("users_groups", "user_id")
                            .to("users", "id")
                            .on_delete(ForeignKeyAction::Cascade)
                            .on_update(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .from("users_groups", "group_id")
                            .to("groups", "id")
                            .on_delete(ForeignKeyAction::Cascade)
                            .on_update(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        // Polymorphic: `resource_id` points into projects or workflow_integrations depending on
        // `resource_type`, `principal_id` into users or groups (NULL for the public principal), so
        // there are no foreign keys. Rows are removed by the code deleting the resource/principal.
        manager
            .create_table(
                Table::create()
                    .table("shares")
                    .if_not_exists()
                    .col(big_pk_auto("id"))
                    .col(small_integer("principal_type"))
                    .col(big_integer_null("principal_id"))
                    .col(small_integer("resource_type"))
                    .col(big_integer("resource_id"))
                    .col(small_integer("role"))
                    .to_owned(),
            )
            .await?;

        // NULL never collides in a unique index, so public shares are deduplicated in code.
        manager
            .create_index(
                Index::create()
                    .name("idx_shares_unique_grant")
                    .table("shares")
                    .col("principal_type")
                    .col("principal_id")
                    .col("resource_type")
                    .col("resource_id")
                    .unique()
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_shares_resource")
                    .table("shares")
                    .col("resource_type")
                    .col("resource_id")
                    .to_owned(),
            )
            .await?;

        // Existing rows stay accessible: creators become owners, public workflows get a public
        // viewer share. Discriminants: principal 1 = user, 3 = public; resource 1 = project,
        // 2 = workflow; role 1 = viewer, 3 = owner.
        let db = manager.get_connection();
        for sql in [
            "INSERT INTO shares (principal_type, principal_id, resource_type, resource_id, role) \
             SELECT 1, owner_id, 1, id, 3 FROM projects",
            "INSERT INTO shares (principal_type, principal_id, resource_type, resource_id, role) \
             SELECT 1, owner_id, 2, id, 3 FROM workflow_integrations",
            "INSERT INTO shares (principal_type, principal_id, resource_type, resource_id, role) \
             SELECT 3, NULL, 2, id, 1 FROM workflow_integrations WHERE is_public",
        ] {
            db.execute_unprepared(sql).await?;
        }
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table("shares").to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table("users_groups").to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table("groups").to_owned())
            .await
    }
}
