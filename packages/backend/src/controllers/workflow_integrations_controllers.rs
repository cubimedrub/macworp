use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{Query, Request, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
};
use futures_util::StreamExt;
use macworp_core::{
    entities::Role,
    requests::{NewWorkflowIntegrationRequest, UpdateWorkflowIntegrationRequest},
    responses::WorkflowIntegrationResponse,
    routing::WorkflowIntegrationsRouting,
};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, EntityTrait, ModelTrait, QueryFilter, TransactionTrait,
};
use serde::Deserialize;
use thiserror::Error;
use tokio::io::AsyncWriteExt;

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
    /// The uploaded archive was rejected (unsafe, too big, not a zip), 400
    #[error("{0}")]
    Archive(crate::workflow_upload::Error),
    /// The request body exceeds `max_workflow_upload_bytes`, 413
    #[error("Upload is larger than {0} bytes")]
    UploadTooLarge(u64),
    /// The configured policy does not let this user create local workflows, 403
    #[error("Creating local workflows is not allowed for you on this server")]
    LocalWorkflowsForbidden,
    /// Uploads only apply to workflows with a local source, 409
    #[error("The workflow does not use a local source")]
    NotLocalSource,
    /// Receiving or installing the upload failed, 500
    #[error("{0}")]
    UploadIo(#[from] std::io::Error),
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
            Error::Archive(crate::workflow_upload::Error::Io(_)) => {
                StatusCode::INTERNAL_SERVER_ERROR.into_response()
            }
            Error::Archive(_) => (StatusCode::BAD_REQUEST, self.to_string()).into_response(),
            Error::UploadTooLarge(_) => {
                (StatusCode::PAYLOAD_TOO_LARGE, self.to_string()).into_response()
            }
            Error::LocalWorkflowsForbidden => {
                (StatusCode::FORBIDDEN, self.to_string()).into_response()
            }
            Error::NotLocalSource => (StatusCode::CONFLICT, self.to_string()).into_response(),
            Error::Source(crate::repository_cache::Error::Fetch { .. }) => {
                (StatusCode::BAD_GATEWAY, self.to_string()).into_response()
            }
            Error::Source(_) => (StatusCode::BAD_REQUEST, self.to_string()).into_response(),
            _ => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
        }
    }
}

/// Query of the upload endpoint
#[derive(Debug, Deserialize)]
struct UploadQuery {
    /// Main script inside the archive; the current one is kept if omitted
    script: Option<std::path::PathBuf>,
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
            .route(WorkflowIntegrationsRouting::UPLOAD_PATH, post(Self::upload))
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
            .set_owner_id(user.id)
            .set_configuration(Self::initial_configuration(&state, &user));

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

    /// Configuration of a new workflow: the default, but with a remote source where the policy
    /// does not let the user create local workflows.
    ///
    /// # Arguments
    /// * `state` - Backend state holding the policy
    /// * `user` - The creating user
    fn initial_configuration(
        state: &AppState,
        user: &crate::models::User,
    ) -> macworp_core::workflow_configuration::WorkflowConfiguration {
        use macworp_core::workflow_configuration::source::{RemoteSource, Source};
        let mut configuration =
            macworp_core::workflow_configuration::WorkflowConfiguration::default();
        if !state.may_create_local_workflows(user) {
            *configuration.source_mut() = Source::Remote(RemoteSource::default());
        }
        configuration
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
        state.confine_local_source(workflow_integration.id, &mut configuration);
        let storage = state.config().storage_directory();
        let response = tokio::task::spawn_blocking(move || {
            let resolved = cache.localize(&mut configuration)?;
            let Source::Local(local) = configuration.source() else {
                return Ok(Default::default());
            };
            let directory = macworp_core::paths::resolve_in_storage(&storage, local.location());
            let mut schemas = crate::repository_cache::RepositoryCache::schemas(&directory);
            schemas.commit = resolved.map(|r| r.sha);
            Ok::<_, crate::repository_cache::Error>(schemas)
        })
        .await
        .map_err(|_| Error::SourceTask)??;
        Ok(Json(response))
    }

    /// Replaces the files of a local workflow by an uploaded zip archive (Editor). The body is
    /// streamed to a temporary file (at most `max_workflow_upload_bytes`), extracted with path,
    /// symlink, size and file count checks and swapped in below
    /// `<storage_path>/local/<id>`; a rejected archive leaves the previous version in place.
    async fn upload(
        State(state): State<Arc<AppState>>,
        Authorized {
            model: workflow_integration,
            role,
            user,
            ..
        }: Authorized<WorkflowIntegration, Editor>,
        Query(query): Query<UploadQuery>,
        request: Request,
    ) -> Result<Json<WorkflowIntegrationResponse>, Error> {
        use macworp_core::workflow_configuration::source::Source;
        if !state.may_create_local_workflows(&user) {
            return Err(Error::LocalWorkflowsForbidden);
        }
        let mut configuration = workflow_integration.configuration.clone();
        if !matches!(configuration.source(), Source::Local(_)) {
            return Err(Error::NotLocalSource);
        }
        state.confine_local_source(workflow_integration.id, &mut configuration);
        let Source::Local(local) = configuration.source_mut() else {
            return Err(Error::NotLocalSource);
        };
        let target = state.config().resolve_storage_path(local.location());
        if let Some(script) = query.script {
            *local.script_mut() = macworp_core::paths::sanitize_relative(&script);
        }
        let script = local.script().clone();

        // Stream to a temporary file next to the target (same filesystem, cleaned up below).
        let limit = state.config().max_workflow_upload_bytes();
        let parent = target.parent().expect("workflow directory has a parent");
        tokio::fs::create_dir_all(parent).await?;
        let archive = parent.join(format!(".upload-{}.zip", uuid::Uuid::now_v7()));
        let received = Self::receive(request, &archive, limit).await;
        let installed = match received {
            Ok(()) => {
                let limits = state.config().workflow_upload_limits();
                let (archive_path, target_path) = (archive.clone(), target.clone());
                tokio::task::spawn_blocking(move || {
                    crate::workflow_upload::WorkflowUpload::install(
                        &archive_path,
                        &target_path,
                        &script,
                        &limits,
                    )
                })
                .await
                .map_err(|_| Error::SourceTask)
                .and_then(|r| r.map_err(Error::Archive))
            }
            Err(error) => Err(error),
        };
        let _ = tokio::fs::remove_file(&archive).await;
        let installed = installed?;
        tracing::info!(
            files = installed.files,
            bytes = installed.bytes,
            "installed uploaded workflow"
        );

        let mut active: ActiveWorkflowIntegration = workflow_integration.into();
        active.configuration = Set(configuration);
        let updated: WorkflowIntegration = active.update(state.db()).await?;
        let owner_username = User::usernames(state.db(), updated.owner_id)
            .await?
            .into_values()
            .next()
            .unwrap_or_default();
        let mut response = WorkflowIntegrationResponse::from(updated);
        response.owner_username = owner_username;
        response.my_role = Some(role);
        Ok(Json(response))
    }

    /// Writes the request body to `path`, failing once more than `limit` bytes arrived.
    ///
    /// # Arguments
    /// * `request` - Request whose body is the archive
    /// * `path` - File to create
    /// * `limit` - Largest accepted body in bytes
    async fn receive(request: Request, path: &std::path::Path, limit: u64) -> Result<(), Error> {
        let mut file = tokio::fs::File::create(path).await?;
        let mut stream = request.into_body().into_data_stream();
        let mut received = 0u64;
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|e| std::io::Error::other(e.to_string()))?;
            received += chunk.len() as u64;
            if received > limit {
                return Err(Error::UploadTooLarge(limit));
            }
            file.write_all(&chunk).await?;
        }
        file.flush().await?;
        Ok(())
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
        let id = workflow_integration.id;
        workflow_integration.delete(&txn).await?;
        txn.commit().await?;
        let directory = state.config().local_workflow_directory(id);
        if let Err(error) = tokio::fs::remove_dir_all(&directory).await
            && error.kind() != std::io::ErrorKind::NotFound
        {
            tracing::warn!(%error, directory = %directory.display(), "could not remove uploaded workflow");
        }

        Ok(http::StatusCode::OK)
    }

    async fn update(
        State(state): State<Arc<AppState>>,
        Authorized {
            model: workflow_integration,
            role,
            user,
            ..
        }: Authorized<WorkflowIntegration, Editor>,
        Json(payload): Json<UpdateWorkflowIntegrationRequest>,
    ) -> Result<Json<WorkflowIntegrationResponse>, Error> {
        use macworp_core::workflow_configuration::source::Source;
        // Switching a workflow to a local source is creating a local workflow
        let becomes_local = matches!(payload.configuration.source(), Source::Local(_))
            && !matches!(
                workflow_integration.configuration.source(),
                Source::Local(_)
            );
        if becomes_local && !state.may_create_local_workflows(&user) {
            return Err(Error::LocalWorkflowsForbidden);
        }
        let mut active: ActiveWorkflowIntegration = workflow_integration.into();
        active.name = Set(payload.name);
        active.description = Set(payload.description);
        let mut configuration = payload.configuration;
        state.confine_local_source(active.id.clone().unwrap(), &mut configuration);
        active.configuration = Set(configuration);

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
