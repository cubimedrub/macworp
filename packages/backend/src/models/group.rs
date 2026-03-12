use sea_orm::entity::prelude::*;

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
