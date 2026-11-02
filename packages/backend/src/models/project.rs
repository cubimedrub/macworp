use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "projects")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    /// Creator, `None` once that user was deleted while the project outlived them
    #[sea_orm(nullable)]
    pub owner_id: Option<i64>,
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

    // Editors keep a project alive, viewers alone cannot maintain it.
    const MIN_ROLE_WITHOUT_OWNER: macworp_core::entities::Role =
        macworp_core::entities::Role::Editor;

    fn id_column() -> Column {
        Column::Id
    }

    fn owner_id_column() -> Column {
        Column::OwnerId
    }
}
