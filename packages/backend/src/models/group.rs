use sea_orm::{Condition, QueryOrder, entity::prelude::*, sea_query::Query};

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "groups")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub owner_id: i64,
    pub name: String,
    pub description: String,

    #[sea_orm(belongs_to, relation_enum = "Owner", from = "owner_id", to = "id")]
    pub owner: HasOne<super::user::Entity>,

    #[sea_orm(has_many, via = "users_groups")]
    pub members: HasMany<super::user::Entity>,
}

impl ActiveModelBehavior for ActiveModel {}

impl Model {
    /// Condition restricting a group query to groups `user_id` owns or is a member of.
    pub fn visible_to(user_id: i64) -> Condition {
        Condition::any().add(Column::OwnerId.eq(user_id)).add(
            Column::Id.in_subquery(
                Query::select()
                    .column(super::users_groups::Column::GroupId)
                    .from(super::users_groups::Entity)
                    .and_where(super::users_groups::Column::UserId.eq(user_id))
                    .to_owned(),
            ),
        )
    }

    /// Whether `user_id` owns the group or is one of its members.
    pub async fn is_visible_to(
        &self,
        db: &impl ConnectionTrait,
        user_id: i64,
    ) -> Result<bool, DbErr> {
        Ok(Entity::find_by_id(self.id)
            .filter(Self::visible_to(user_id))
            .one(db)
            .await?
            .is_some())
    }

    /// Members of the group as `(id, username)`, sorted by username.
    pub async fn member_names(
        &self,
        db: &impl ConnectionTrait,
    ) -> Result<Vec<(i64, String)>, DbErr> {
        Ok(super::user::Entity::find()
            .filter(
                super::user::Column::Id.in_subquery(
                    Query::select()
                        .column(super::users_groups::Column::UserId)
                        .from(super::users_groups::Entity)
                        .and_where(super::users_groups::Column::GroupId.eq(self.id))
                        .to_owned(),
                ),
            )
            .order_by_asc(super::user::Column::Username)
            .all(db)
            .await?
            .into_iter()
            .map(|user| (user.id, user.username))
            .collect())
    }

    /// Names of the given groups by id. Unknown ids are missing from the map.
    pub async fn names(
        db: &impl ConnectionTrait,
        ids: impl IntoIterator<Item = i64>,
    ) -> Result<std::collections::HashMap<i64, String>, DbErr> {
        Ok(Entity::find()
            .filter(Column::Id.is_in(ids))
            .all(db)
            .await?
            .into_iter()
            .map(|group| (group.id, group.name))
            .collect())
    }
}
