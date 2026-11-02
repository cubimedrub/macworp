use macworp_core::{
    responses::WorkflowIntegrationResponse, workflow_configuration::WorkflowConfiguration,
};
use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "workflow_integrations")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    /// Creator, `None` once that user was deleted while the workflow outlived them
    #[sea_orm(nullable)]
    pub owner_id: Option<i64>,
    pub name: String,
    pub description: String,
    pub configuration: WorkflowConfiguration,

    #[sea_orm(belongs_to, from = "owner_id", to = "id")]
    pub owner: HasOne<super::user::Entity>,
}

impl ActiveModelBehavior for ActiveModel {}

impl From<Model> for WorkflowIntegrationResponse {
    fn from(value: Model) -> Self {
        Self {
            id: value.id,
            owner_username: String::new(),
            name: value.name,
            description: value.description,
            configuration: value.configuration,
            my_role: None,
        }
    }
}

impl crate::authorization::Resource for Model {
    type Entity = Entity;

    const RESOURCE_TYPE: crate::authorization::ResourceType =
        crate::authorization::ResourceType::Workflow;
    // Others may see and run a workflow, editing stays with users and groups.
    const MAX_PUBLIC_ROLE: macworp_core::entities::Role = macworp_core::entities::Role::Viewer;

    // Anyone who can still see or edit a workflow keeps it alive.
    const MIN_ROLE_WITHOUT_OWNER: macworp_core::entities::Role =
        macworp_core::entities::Role::Viewer;

    fn id_column() -> Column {
        Column::Id
    }

    fn owner_id_column() -> Column {
        Column::OwnerId
    }
}
