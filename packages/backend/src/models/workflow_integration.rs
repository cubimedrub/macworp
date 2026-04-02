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
    pub owner_id: i64,
    pub is_public: bool,
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
            is_public: value.is_public,
            name: value.name,
            description: value.description,
            configuration: value.configuration,
        }
    }
}
