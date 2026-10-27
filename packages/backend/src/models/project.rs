use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "projects")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub owner_id: i64,
    pub path: Option<String>,
    pub name: String,
    pub created_at: DateTimeUtc,

    #[sea_orm(belongs_to, from = "owner_id", to = "id")]
    pub owner: HasOne<super::user::Entity>,
}

impl ActiveModelBehavior for ActiveModel {}

impl crate::authorization::Resource for Model {
    type Entity = Entity;

    const RESOURCE_TYPE: crate::authorization::ResourceType =
        crate::authorization::ResourceType::Project;
    const MAX_PUBLIC_ROLE: macworp_core::entities::Role = macworp_core::entities::Role::Owner;

    fn id_column() -> Column {
        Column::Id
    }
}
