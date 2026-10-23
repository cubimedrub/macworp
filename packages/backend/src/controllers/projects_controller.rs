use std::{path::PathBuf, sync::Arc};

use axum::{
    Router,
    extract::{Json, Path, Query, Request, State},
    response::IntoResponse,
    routing::{get, post},
};

use http::{HeaderMap, StatusCode};
use macworp_core::{
    http::headers::ContentRangeHeaderValue,
    requests::{NewProjectRequest, ProjectCreateFolderRequest, ProjectPathDeletionRequest},
    responses::ProjectResponse,
    routing::ProjectsRouting,
};
use sea_orm::{
    ColumnTrait, EntityTrait, ModelTrait, QueryFilter, TransactionError, TransactionTrait,
};
use serde::Deserialize;
use thiserror::Error;

use crate::{
    authentication::middleware::RequireAuth,
    filesystems::is_filesystem::IsFilesystem,
    models::{ActiveProject, Project, ProjectColumn, ProjectTable},
    state::State as AppState,
};

#[derive(Error, Debug)]
pub enum ProjectsControllerError {
    #[error("{0}")]
    Database(#[from] sea_orm::DbErr),
    #[error("{0}")]
    Authentication(#[from] crate::authentication::error::Error),
    #[error("Project not found")]
    ProjectNotFound,
    #[error("Access to this project is forbidden")]
    Forbidden,
    #[error("Errors while accessing filesystem: {0}")]
    Filesystem(#[from] crate::filesystems::errors::Error),
    #[error("Missing Content-Range header")]
    MissingContentRangeHeader,
    #[error("Invalid Content-Range header: {0}")]
    InvalidContentRangeHeader(macworp_core::http::headers::ContentRangeHeaderValueError),
}

impl IntoResponse for ProjectsControllerError {
    fn into_response(self) -> axum::response::Response {
        tracing::error!("{self}");
        match self {
            ProjectsControllerError::Forbidden => {
                (StatusCode::FORBIDDEN, self.to_string()).into_response()
            }
            ProjectsControllerError::ProjectNotFound => {
                (StatusCode::NOT_FOUND, self.to_string()).into_response()
            }
            ProjectsControllerError::Authentication(e) => e.into_response(),
            ProjectsControllerError::Filesystem(
                crate::filesystems::errors::Error::SymlinkNotAllowed(_),
            ) => (StatusCode::FORBIDDEN, "Symlinks are not allowed").into_response(),
            _ => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
        }
    }
}

#[derive(Deserialize)]
pub struct ProjectQueryParams {
    /// Project ID to fetch
    pub subpath: PathBuf,
}

#[derive(Deserialize)]
pub struct ProjectFileUploadQuery {
    /// File name to upload
    pub file_path: PathBuf,
}

pub struct ProjectsController<F: IsFilesystem + 'static> {
    _filesystem: std::marker::PhantomData<F>,
}

impl<F> ProjectsController<F>
where
    F: IsFilesystem,
{
    pub fn routes(state: Arc<AppState>) -> Router<Arc<AppState>> {
        Router::new()
            .route(ProjectsRouting::INDEX_PATH, get(Self::index))
            .route(ProjectsRouting::NEW_PATH, post(Self::new))
            .route(ProjectsRouting::EDIT_PATH, get(Self::edit))
            .route(ProjectsRouting::DELETE_PATH, post(Self::delete))
            .route(ProjectsRouting::UPLOAD_FILE_PATH, post(Self::upload_file))
            .route(
                ProjectsRouting::CREATE_FOLDER_PATH,
                post(Self::create_folder),
            )
            .route(ProjectsRouting::DELETE_PATH_PATH, post(Self::delete_path))
            .with_state(state)
    }

    pub async fn index(
        State(state): State<Arc<AppState>>,
        RequireAuth(user): RequireAuth,
    ) -> Result<Json<Vec<ProjectResponse>>, ProjectsControllerError> {
        let owned_projects: Vec<Project> = ProjectTable::find()
            .filter(ProjectColumn::OwnerId.eq(user.id))
            .all(state.db())
            .await
            .map_err(ProjectsControllerError::Database)?;

        Ok(Json(
            owned_projects
                .into_iter()
                .map(|project| ProjectResponse {
                    id: project.id,
                    name: project.name,
                    created_at: project.created_at,
                    owner_username: user.username.clone(),
                    current: PathBuf::new(),
                    files: Vec::new(),
                    directories: Vec::new(),
                    local_path: None,
                })
                .collect(),
        ))
    }

    #[allow(clippy::new_ret_no_self)]
    pub async fn new(
        State(state): State<Arc<AppState>>,
        RequireAuth(user): RequireAuth,
        Json(payload): Json<NewProjectRequest>,
    ) -> Result<Json<ProjectResponse>, ProjectsControllerError> {
        let state_clone = state.clone();
        let project = state
            .db()
            .transaction::<_, Project, ProjectsControllerError>(|txn| {
                Box::pin(async move {
                    let new_project = ActiveProject::builder()
                        .set_name(payload.name.clone())
                        .set_created_at(chrono::Utc::now())
                        .set_path(payload.path.to_string_lossy().to_string())
                        .set_owner_id(user.id);

                    let project: Project = new_project
                        .insert(txn)
                        .await
                        .map_err(ProjectsControllerError::Database)?
                        .into();

                    F::create_project_directory(state_clone.as_ref(), &project)
                        .await
                        .map_err(ProjectsControllerError::Filesystem)?;

                    Ok(project)
                })
            })
            .await
            .map_err(|err| match err {
                TransactionError::Connection(err) => ProjectsControllerError::Database(err),
                TransactionError::Transaction(err) => err,
            })?;

        Ok(Json(ProjectResponse {
            id: project.id,
            name: project.name,
            created_at: project.created_at,
            owner_username: user.username.clone(),
            current: PathBuf::new(),
            files: Vec::new(),
            directories: Vec::new(),
            local_path: None,
        }))
    }

    pub async fn edit(
        State(state): State<Arc<AppState>>,
        RequireAuth(user): RequireAuth,
        Path(id): Path<i64>,
        Query(query_params): Query<ProjectQueryParams>,
    ) -> Result<Json<ProjectResponse>, ProjectsControllerError> {
        let project = ProjectTable::find_by_id(id)
            .one(state.db())
            .await
            .map_err(ProjectsControllerError::Database)?
            .ok_or(ProjectsControllerError::ProjectNotFound)?;

        // TODO: once implemented add group check.
        if project.owner_id != user.id {
            return Err(ProjectsControllerError::Forbidden);
        }

        let content = F::content(state.as_ref(), &project, query_params.subpath)
            .map_err(ProjectsControllerError::Filesystem)?;
        let local_path = F::local_path(state.as_ref(), &project);

        Ok(Json(ProjectResponse {
            id: project.id,
            name: project.name,
            created_at: project.created_at,
            owner_username: user.username.clone(),
            current: content.current,
            files: content.files,
            directories: content.directories,
            local_path,
        }))
    }

    async fn delete(
        State(state): State<Arc<AppState>>,
        RequireAuth(user): RequireAuth,
        Path(id): Path<i64>,
    ) -> Result<StatusCode, ProjectsControllerError> {
        let project = ProjectTable::find_by_id(id)
            .one(state.db())
            .await
            .map_err(ProjectsControllerError::Database)?
            .ok_or(ProjectsControllerError::ProjectNotFound)?;

        if project.owner_id != user.id {
            return Err(ProjectsControllerError::Forbidden);
        }
        state
            .clone()
            .db()
            .transaction::<_, (), ProjectsControllerError>(|txn| {
                Box::pin(async move {
                    F::delete_project_directory(state.as_ref(), &project)
                        .await
                        .map_err(ProjectsControllerError::Filesystem)?;

                    project
                        .delete(txn)
                        .await
                        .map_err(ProjectsControllerError::Database)?;

                    Ok(())
                })
            })
            .await
            .map_err(|err| match err {
                TransactionError::Connection(err) => ProjectsControllerError::Database(err),
                TransactionError::Transaction(err) => err,
            })?;

        Ok(StatusCode::OK)
    }

    pub async fn upload_file(
        State(state): State<Arc<AppState>>,
        RequireAuth(user): RequireAuth,
        Path(id): Path<i64>,
        Query(params): Query<ProjectFileUploadQuery>,
        headers: HeaderMap,
        request: Request,
    ) -> Result<StatusCode, ProjectsControllerError> {
        let project = ProjectTable::find_by_id(id)
            .one(state.db())
            .await
            .map_err(ProjectsControllerError::Database)?
            .ok_or(ProjectsControllerError::ProjectNotFound)?;

        if project.owner_id != user.id {
            return Err(ProjectsControllerError::Forbidden);
        }

        let content_header_value = headers
            .get("Content-Range")
            .ok_or(ProjectsControllerError::MissingContentRangeHeader)?;

        let content_range = ContentRangeHeaderValue::try_from(content_header_value)
            .map_err(ProjectsControllerError::InvalidContentRangeHeader)?;

        F::stream_to_file(
            state.as_ref(),
            &project,
            params.file_path,
            content_range,
            request.into_body().into_data_stream(),
        )
        .await?;

        Ok(StatusCode::OK)
    }

    pub async fn create_folder(
        State(state): State<Arc<AppState>>,
        RequireAuth(user): RequireAuth,
        Path(id): Path<i64>,
        Json(payload): Json<ProjectCreateFolderRequest>,
    ) -> Result<StatusCode, ProjectsControllerError> {
        let project = ProjectTable::find_by_id(id)
            .one(state.db())
            .await
            .map_err(ProjectsControllerError::Database)?
            .ok_or(ProjectsControllerError::ProjectNotFound)?;

        if project.owner_id != user.id {
            return Err(ProjectsControllerError::Forbidden);
        }

        F::create_folder(state.as_ref(), &project, payload.folder_path)
            .await
            .map_err(ProjectsControllerError::Filesystem)?;

        Ok(StatusCode::OK)
    }

    pub async fn delete_path(
        State(state): State<Arc<AppState>>,
        RequireAuth(user): RequireAuth,
        Path(id): Path<i64>,
        Json(payload): Json<ProjectPathDeletionRequest>,
    ) -> Result<StatusCode, ProjectsControllerError> {
        let project = ProjectTable::find_by_id(id)
            .one(state.db())
            .await
            .map_err(ProjectsControllerError::Database)?
            .ok_or(ProjectsControllerError::ProjectNotFound)?;

        if project.owner_id != user.id {
            return Err(ProjectsControllerError::Forbidden);
        }

        F::delete_path(state.as_ref(), &project, payload.path)
            .await
            .map_err(ProjectsControllerError::Filesystem)?;

        Ok(StatusCode::OK)
    }
}
