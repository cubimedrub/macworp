use sea_orm::{TransactionTrait, entity::prelude::*};

/// Why [`Model::set_admin`] failed.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The database failed.
    #[error("Database error: {0}")]
    Database(#[from] DbErr),
    /// No user has the given username.
    #[error("No user with this username exists")]
    NotFound,
    /// Demoting would leave the instance without an enabled administrator.
    #[error("Cannot demote the last enabled administrator")]
    LastAdmin,
}

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "users")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    #[sea_orm(unique, indexed)]
    pub username: String,

    pub firstname: String,
    pub lastname: String,
    pub email: String,

    #[sea_orm(nullable)]
    pub password: Option<String>,
    /// Whether the user administers the instance
    pub is_admin: bool,
    /// Whether the account is blocked from logging in and using existing tokens
    pub disabled: bool,
    /// Generation of the user's login tokens; a token is valid only while its `ver` claim equals
    /// this. [`Model::revoke_tokens`] increments it to log the user out everywhere.
    pub token_version: i32,

    #[sea_orm(has_many, relation_enum = "OwnedGroups", via_rel = "Owner")]
    pub groups: HasMany<super::group::Entity>,

    #[sea_orm(has_many, via = "users_groups")]
    pub member_groups: HasMany<super::group::Entity>,

    #[sea_orm(has_many)]
    pub projects: HasMany<super::project::Entity>,

    #[sea_orm(has_many)]
    pub workflows: HasMany<super::workflow_integration::Entity>,
}

impl ActiveModelBehavior for ActiveModel {}

impl Model {
    /// Invalidates every token issued so far for the user (logout everywhere) by incrementing
    /// `token_version`. Atomic, so concurrent logins cannot lose an increment.
    pub async fn revoke_tokens(db: &impl ConnectionTrait, id: i64) -> Result<(), DbErr> {
        use sea_orm::{ExprTrait, sea_query::Expr};
        Entity::update_many()
            .col_expr(Column::TokenVersion, Expr::col(Column::TokenVersion).add(1))
            .filter(Column::Id.eq(id))
            .exec(db)
            .await
            .map(|_| ())
    }

    /// At most `limit` users whose username starts with `prefix`, sorted by username.
    pub async fn search(
        db: &impl ConnectionTrait,
        prefix: &str,
        limit: u64,
    ) -> Result<Vec<Model>, DbErr> {
        use sea_orm::{QueryOrder, QuerySelect};
        Entity::find()
            .filter(Column::Username.starts_with(prefix))
            .order_by_asc(Column::Username)
            .limit(limit)
            .all(db)
            .await
    }

    /// All users sorted by username; with `query` only those whose username, names or email
    /// contain it.
    pub async fn list(db: &impl ConnectionTrait, query: Option<&str>) -> Result<Vec<Model>, DbErr> {
        use sea_orm::{Condition, QueryOrder};
        let mut select = Entity::find();
        if let Some(query) = query.map(str::trim).filter(|q| !q.is_empty()) {
            select = select.filter(
                Condition::any()
                    .add(Column::Username.contains(query))
                    .add(Column::Firstname.contains(query))
                    .add(Column::Lastname.contains(query))
                    .add(Column::Email.contains(query)),
            );
        }
        select.order_by_asc(Column::Username).all(db).await
    }

    /// Number of administrators that are not disabled, i.e. who can still manage the instance.
    pub async fn enabled_admin_count(db: &impl ConnectionTrait) -> Result<u64, DbErr> {
        Entity::find()
            .filter(Column::IsAdmin.eq(true))
            .filter(Column::Disabled.eq(false))
            .count(db)
            .await
    }

    /// Grants or removes the administrator flag of the user named `username`, e.g. from the
    /// command line. Setting the flag a user already has succeeds without a change. Demoting
    /// the last enabled administrator is rejected; the check runs in the same transaction as
    /// the update.
    pub async fn set_admin(
        db: &DatabaseConnection,
        username: &str,
        is_admin: bool,
    ) -> Result<Model, Error> {
        let transaction = db.begin().await?;
        let user = Entity::find()
            .filter(Column::Username.eq(username))
            .one(&transaction)
            .await?
            .ok_or(Error::NotFound)?;
        if user.is_admin == is_admin {
            return Ok(user);
        }
        if !is_admin && !user.disabled && Self::enabled_admin_count(&transaction).await? <= 1 {
            return Err(Error::LastAdmin);
        }
        let mut active = ActiveModel::from(user);
        active.is_admin = sea_orm::Set(is_admin);
        let user = active.update(&transaction).await?;
        transaction.commit().await?;

        tracing::info!(
            target: "audit",
            user = %user.username,
            action = if is_admin { "promote" } else { "demote" },
            "administrator flag changed via command line"
        );
        Ok(user)
    }

    /// Usernames of the given users by id, for showing who created shared resources. Unknown ids
    /// are missing from the map.
    pub async fn usernames(
        db: &impl ConnectionTrait,
        ids: impl IntoIterator<Item = i64>,
    ) -> Result<std::collections::HashMap<i64, String>, DbErr> {
        Ok(Entity::find()
            .filter(Column::Id.is_in(ids))
            .all(db)
            .await?
            .into_iter()
            .map(|user| (user.id, user.username))
            .collect())
    }
}

#[cfg(all(test, feature = "integration"))]
mod tests {
    use macworp_database_migration::{Migrator, MigratorTrait};
    use sea_orm::{ConnectionTrait, Database, DatabaseConnection};

    use super::{Error, Model};

    /// Migrated in-memory database with `root` (admin), `alice` and `bob` (plain users).
    async fn database() -> DatabaseConnection {
        let db = Database::connect("sqlite::memory:").await.unwrap();
        Migrator::up(&db, None).await.unwrap();
        for (name, admin) in [("root", 1), ("alice", 0), ("bob", 0)] {
            db.execute_unprepared(&format!(
                "INSERT INTO users (username, firstname, lastname, email, is_admin) VALUES ('{name}', 'f', 'l', '{name}@x', {admin})"
            ))
            .await
            .unwrap();
        }
        db
    }

    /// Promoting is idempotent and demoting works while another administrator remains.
    #[tokio::test]
    async fn promote_and_demote() {
        let db = database().await;
        assert!(Model::set_admin(&db, "alice", true).await.unwrap().is_admin);
        assert!(Model::set_admin(&db, "alice", true).await.unwrap().is_admin);
        assert!(!Model::set_admin(&db, "root", false).await.unwrap().is_admin);
    }

    /// The last enabled administrator cannot be demoted.
    #[tokio::test]
    async fn last_admin_cannot_be_demoted() {
        let db = database().await;
        assert!(matches!(
            Model::set_admin(&db, "root", false).await,
            Err(Error::LastAdmin)
        ));
    }

    /// Unknown usernames are reported.
    #[tokio::test]
    async fn unknown_user() {
        let db = database().await;
        assert!(matches!(
            Model::set_admin(&db, "nobody", true).await,
            Err(Error::NotFound)
        ));
    }
}
