use sea_orm::entity::prelude::*;

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
