use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

/// Id of the single global reference data row.
const SINGLETON_ID: i64 = 1;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Singleton: exactly one row, created here and never deleted. It has no owner, the
        // column exists because every shareable table carries one.
        manager
            .create_table(
                Table::create()
                    .table("reference_data")
                    .if_not_exists()
                    .col(big_pk_auto("id"))
                    .col(big_integer_null("owner_id"))
                    .to_owned(),
            )
            .await?;

        // Discriminants: principal 3 = public; resource 3 = reference data; role 1 = viewer.
        // Every user can read, editors are added as ordinary user/group shares.
        let db = manager.get_connection();
        for sql in [
            format!("INSERT INTO reference_data (id) VALUES ({SINGLETON_ID})"),
            format!(
                "INSERT INTO shares (principal_type, principal_id, resource_type, resource_id, role) \
                 VALUES (3, NULL, 3, {SINGLETON_ID}, 1)"
            ),
        ] {
            db.execute_unprepared(&sql).await?;
        }
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared("DELETE FROM shares WHERE resource_type = 3")
            .await?;
        manager
            .drop_table(Table::drop().table("reference_data").to_owned())
            .await
    }
}
