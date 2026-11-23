use sea_orm::entity::prelude::*;

use macworp_core::entities::Role;

use crate::authorization::{Resource, ResourceType};

/// Id of the only row, created by the migration.
pub const SINGLETON_ID: i64 = macworp_core::routing::ReferenceRouting::SINGLETON_ID;

/// The instance-wide reference data (genomes, indices, annotation) every project can read.
///
/// A singleton without owner: everybody is a public Viewer, Editors are plain user or group
/// shares, and the row is never deleted (no route removes it, user deletion only touches
/// projects and workflows).
#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "reference_data")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    /// Always `NULL`, the column exists because [`Resource`] asks for one.
    #[sea_orm(nullable)]
    pub owner_id: Option<i64>,
}

impl ActiveModelBehavior for ActiveModel {}

impl Resource for Model {
    type Entity = Entity;

    const RESOURCE_TYPE: ResourceType = ResourceType::ReferenceData;
    const MAX_PUBLIC_ROLE: Role = Role::Viewer;
    const MIN_ROLE_WITHOUT_OWNER: Role = Role::Viewer;

    fn id_column() -> Column {
        Column::Id
    }

    fn owner_id_column() -> Column {
        Column::OwnerId
    }
}
