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

    #[sea_orm(has_many, relation_enum = "OwnedGroups", via_rel = "Owner")]
    pub groups: HasMany<super::group::Entity>,

    #[sea_orm(has_many, via = "users_groups")]
    pub member_groups: HasMany<super::group::Entity>,

    #[sea_orm(has_many)]
    pub projects: HasMany<super::project::Entity>,

    #[sea_orm(has_many)]
    pub workflows: HasMany<super::workflow::Entity>,
}

impl ActiveModelBehavior for ActiveModel {}
