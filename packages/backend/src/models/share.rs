use sea_orm::entity::prelude::*;

/// A role granted to a principal on a project or workflow.
///
/// The type and role columns hold the discriminants of [`crate::authorization::PrincipalType`],
/// [`crate::authorization::ResourceType`] and [`macworp_core::entities::Role`]. There are no
/// foreign keys: `resource_id` and `principal_id` point into different tables by type.
#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "shares")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    /// [`crate::authorization::PrincipalType`]
    pub principal_type: i16,
    /// User or group id, `NULL` for the public principal
    pub principal_id: Option<i64>,
    /// [`crate::authorization::ResourceType`]
    pub resource_type: i16,
    /// Project or workflow id, depending on `resource_type`
    pub resource_id: i64,
    /// [`macworp_core::entities::Role`]
    pub role: i16,
}

impl ActiveModelBehavior for ActiveModel {}
