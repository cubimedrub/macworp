use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{Path, State},
    routing::{get, post},
};
use macworp_core::{
    requests::NewWorkflowIntegrationRequest, responses::WorkflowIntegrationResponse,
    routing::WorkflowIntegrationsRouting,
};
use sea_orm::{ColumnTrait, EntityTrait, ModelTrait, QueryFilter};

use crate::{
    authentication::middleware::RequireAuth,
    controllers::errors::WorkflowIntegrationsControllerError,
    models::{
        ActiveWorkflowIntegration, WorkflowIntegration, WorkflowIntegrationColumn,
        WorkflowIntegrationTable,
    },
    state::State as AppState,
};

pub struct WorkflowIntegrationsController;

impl WorkflowIntegrationsController {
    pub fn routes(state: Arc<AppState>) -> Router<Arc<AppState>> {
        Router::new()
            .route(WorkflowIntegrationsRouting::INDEX_PATH, get(Self::index))
            .route(WorkflowIntegrationsRouting::NEW_PATH, post(Self::new))
            .route(WorkflowIntegrationsRouting::EDIT_PATH, get(Self::edit))
            .route(WorkflowIntegrationsRouting::DELETE_PATH, post(Self::delete))
            .with_state(state)
    }

    async fn index(
        State(state): State<Arc<AppState>>,
        RequireAuth(user): RequireAuth,
    ) -> Result<Json<Vec<WorkflowIntegrationResponse>>, WorkflowIntegrationsControllerError> {
        let owned_workflow_integrations: Vec<WorkflowIntegration> =
            WorkflowIntegrationTable::find()
                .filter(WorkflowIntegrationColumn::OwnerId.eq(user.id))
                .all(state.db())
                .await
                .map_err(WorkflowIntegrationsControllerError::Database)?;

        Ok(Json(
            owned_workflow_integrations
                .into_iter()
                .map(|workflow_integration| {
                    let mut response = WorkflowIntegrationResponse::from(workflow_integration);
                    response.owner_username = user.username.clone();
                    response
                })
                .collect(),
        ))
    }

    #[allow(clippy::new_ret_no_self)]
    async fn new(
        State(state): State<Arc<AppState>>,
        RequireAuth(user): RequireAuth,
        Json(payload): Json<NewWorkflowIntegrationRequest>,
    ) -> Result<Json<WorkflowIntegrationResponse>, WorkflowIntegrationsControllerError> {
        let new_workflow_integration = ActiveWorkflowIntegration::builder()
            .set_name(payload.name.clone())
            .set_description(payload.description)
            .set_owner_id(user.id);

        let workflow_integration: WorkflowIntegration = new_workflow_integration
            .insert(state.db())
            .await
            .map_err(WorkflowIntegrationsControllerError::Database)?
            .into();

        let mut response = WorkflowIntegrationResponse::from(workflow_integration);
        response.owner_username = user.username.clone();

        Ok(Json(response))
    }

    async fn edit(
        State(state): State<Arc<AppState>>,
        RequireAuth(user): RequireAuth,
        Path(id): Path<i64>,
    ) -> Result<Json<WorkflowIntegrationResponse>, WorkflowIntegrationsControllerError> {
        let workflow_integration = WorkflowIntegrationTable::find_by_id(id)
            .one(state.db())
            .await
            .map_err(WorkflowIntegrationsControllerError::Database)?
            .ok_or(WorkflowIntegrationsControllerError::WorkflowIntegrationNotFound)?;

        if workflow_integration.owner_id != user.id {
            return Err(WorkflowIntegrationsControllerError::Forbidden);
        }

        let mut response = WorkflowIntegrationResponse::from(workflow_integration);
        response.owner_username = user.username;

        Ok(Json(response))
    }

    async fn delete(
        State(state): State<Arc<AppState>>,
        RequireAuth(user): RequireAuth,
        Path(id): Path<i64>,
    ) -> Result<http::StatusCode, WorkflowIntegrationsControllerError> {
        let workflow_integration = WorkflowIntegrationTable::find_by_id(id)
            .one(state.db())
            .await
            .map_err(WorkflowIntegrationsControllerError::Database)?
            .ok_or(WorkflowIntegrationsControllerError::WorkflowIntegrationNotFound)?;

        if workflow_integration.owner_id != user.id {
            return Err(WorkflowIntegrationsControllerError::Forbidden);
        }

        workflow_integration
            .delete(state.db())
            .await
            .map_err(WorkflowIntegrationsControllerError::Database)?;

        Ok(http::StatusCode::OK)
    }
}
