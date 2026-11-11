use std::sync::Arc;

use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
};
use macworp_core::{
    entities::Role,
    requests::{NewWorkflowIntegrationRequest, UpdateWorkflowIntegrationRequest},
    responses::WorkflowIntegrationResponse,
    routing::WorkflowIntegrationsRouting,
};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, EntityTrait, ModelTrait, QueryFilter, TransactionTrait,
};
use thiserror::Error;

use crate::{
    authentication::middleware::{ListScope, RequireUser},
    authorization::{Authorized, Editor, Owner, Principal, Resource, Viewer},
    models::{ActiveWorkflowIntegration, User, WorkflowIntegration, WorkflowIntegrationTable},
    state::State as AppState,
};

#[derive(Error, Debug)]
pub enum Error {
    #[error("{0}")]
    Database(#[from] sea_orm::DbErr),
    #[error("{0}")]
    Share(#[from] crate::authorization::ShareError),
    #[error("{0}")]
    Authentication(#[from] crate::authentication::error::Error),
    /// The remote source is invalid or unreachable and not cached
    #[error("{0}")]
    Source(#[from] crate::repository_cache::Error),
    /// The blocking cache task failed
    #[error("Reading the workflow schemas failed")]
    SourceTask,
}

impl IntoResponse for Error {
    fn into_response(self) -> axum::response::Response {
        if let Error::Authentication(e) = self {
            return e.into_response();
        }
        tracing::error!("{self}");
        match self {
            Error::Source(crate::repository_cache::Error::Fetch { .. }) => {
                (StatusCode::BAD_GATEWAY, self.to_string()).into_response()
            }
            Error::Source(_) => (StatusCode::BAD_REQUEST, self.to_string()).into_response(),
            _ => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
        }
    }
}

pub struct WorkflowIntegrationsController;

impl WorkflowIntegrationsController {
    pub fn routes(state: Arc<AppState>) -> Router<Arc<AppState>> {
        Router::new()
            .route(WorkflowIntegrationsRouting::INDEX_PATH, get(Self::index))
            .route(WorkflowIntegrationsRouting::NEW_PATH, post(Self::new))
            .route(WorkflowIntegrationsRouting::EDIT_PATH, get(Self::edit))
            .route(WorkflowIntegrationsRouting::DELETE_PATH, post(Self::delete))
            .route(WorkflowIntegrationsRouting::UPDATE_PATH, post(Self::update))
            .route(
                WorkflowIntegrationsRouting::SCHEMAS_PATH,
                get(Self::schemas),
            )
            .with_state(state)
    }

    /// Lists the workflows the user holds at least the Viewer role on, with `?all=true`
    /// (administrators only) every workflow.
    async fn index(
        State(state): State<Arc<AppState>>,
        scope: ListScope,
    ) -> Result<Json<Vec<WorkflowIntegrationResponse>>, Error> {
        let owned_workflow_integrations: Vec<WorkflowIntegration> =
            WorkflowIntegrationTable::find()
                .filter(scope.filter(WorkflowIntegration::visible_to(scope.user.id, Role::Viewer)))
                .all(state.db())
                .await
                .map_err(Error::Database)?;
        let owners = User::usernames(
            state.db(),
            owned_workflow_integrations
                .iter()
                .filter_map(|w| w.owner_id),
        )
        .await?;
        let ownerless = WorkflowIntegration::ownerless_among(
            state.db(),
            owned_workflow_integrations.iter().map(|w| w.id),
        )
        .await?;

        Ok(Json(
            owned_workflow_integrations
                .into_iter()
                .map(|workflow_integration| {
                    let is_ownerless = ownerless.contains(&workflow_integration.id);
                    let owner = workflow_integration
                        .owner_id
                        .and_then(|id| owners.get(&id).cloned());
                    let mut response = WorkflowIntegrationResponse::from(workflow_integration);
                    response.owner_username = owner.unwrap_or_default();
                    response.ownerless = is_ownerless;
                    response
                })
                .collect(),
        ))
    }

    #[allow(clippy::new_ret_no_self)]
    async fn new(
        State(state): State<Arc<AppState>>,
        RequireUser(user): RequireUser,
        Json(payload): Json<NewWorkflowIntegrationRequest>,
    ) -> Result<Json<WorkflowIntegrationResponse>, Error> {
        let new_workflow_integration = ActiveWorkflowIntegration::builder()
            .set_name(payload.name.clone())
            .set_description(payload.description)
            .set_owner_id(user.id);

        let txn = state.db().begin().await?;
        let workflow_integration: WorkflowIntegration =
            new_workflow_integration.insert(&txn).await?.into();
        WorkflowIntegration::shares(&txn, workflow_integration.id)
            .grant(Principal::User(user.id), Role::Owner)
            .await?;
        txn.commit().await?;

        let mut response = WorkflowIntegrationResponse::from(workflow_integration);
        response.owner_username = user.username.clone();
        response.my_role = Some(Role::Owner);

        Ok(Json(response))
    }

    async fn edit(
        State(state): State<Arc<AppState>>,
        Authorized {
            model: workflow_integration,
            role,
            ..
        }: Authorized<WorkflowIntegration, Viewer>,
    ) -> Result<Json<WorkflowIntegrationResponse>, Error> {
        let owner_username = User::usernames(state.db(), workflow_integration.owner_id)
            .await?
            .into_values()
            .next()
            .unwrap_or_default();
        let ownerless = WorkflowIntegration::ownerless_among(state.db(), [workflow_integration.id])
            .await?
            .contains(&workflow_integration.id);
        let mut response = WorkflowIntegrationResponse::from(workflow_integration);
        response.owner_username = owner_username;
        response.ownerless = ownerless;
        response.my_role = Some(role);

        Ok(Json(response))
    }

    /// Schemas shipped with the workflow (Viewer). A remote source is fetched into the
    /// repository cache first (no network if already cached).
    async fn schemas(
        State(state): State<Arc<AppState>>,
        Authorized {
            model: workflow_integration,
            ..
        }: Authorized<WorkflowIntegration, Viewer>,
    ) -> Result<Json<macworp_core::responses::WorkflowSchemasResponse>, Error> {
        use macworp_core::workflow_configuration::source::Source;
        let cache = state.repository_cache();
        let mut configuration = workflow_integration.configuration.clone();
        let response = tokio::task::spawn_blocking(move || {
            let resolved = cache.localize(&mut configuration)?;
            let Source::Local(local) = configuration.source() else {
                return Ok(Default::default());
            };
            let mut schemas = crate::repository_cache::RepositoryCache::schemas(local.location());
            schemas.commit = resolved.map(|r| r.sha);
            Ok::<_, crate::repository_cache::Error>(schemas)
        })
        .await
        .map_err(|_| Error::SourceTask)??;
        Ok(Json(response))
    }

    async fn delete(
        State(state): State<Arc<AppState>>,
        Authorized {
            model: workflow_integration,
            ..
        }: Authorized<WorkflowIntegration, Owner>,
    ) -> Result<http::StatusCode, Error> {
        let txn = state.db().begin().await?;
        WorkflowIntegration::shares(&txn, workflow_integration.id)
            .delete_all()
            .await?;
        workflow_integration.delete(&txn).await?;
        txn.commit().await?;

        Ok(http::StatusCode::OK)
    }

    async fn update(
        State(state): State<Arc<AppState>>,
        Authorized {
            model: workflow_integration,
            role,
            ..
        }: Authorized<WorkflowIntegration, Editor>,
        Json(payload): Json<UpdateWorkflowIntegrationRequest>,
    ) -> Result<Json<WorkflowIntegrationResponse>, Error> {
        let mut active: ActiveWorkflowIntegration = workflow_integration.into();
        active.name = Set(payload.name);
        active.description = Set(payload.description);
        active.configuration = Set(payload.configuration);

        let updated: WorkflowIntegration = active.update(state.db()).await?;

        let owner_username = User::usernames(state.db(), updated.owner_id)
            .await?
            .into_values()
            .next()
            .unwrap_or_default();
        let ownerless = WorkflowIntegration::ownerless_among(state.db(), [updated.id])
            .await?
            .contains(&updated.id);
        let mut response = WorkflowIntegrationResponse::from(updated);
        response.owner_username = owner_username;
        response.ownerless = ownerless;
        response.my_role = Some(role);

        Ok(Json(response))
    }
}
