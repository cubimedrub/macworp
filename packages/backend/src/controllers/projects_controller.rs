use std::{path::PathBuf, sync::Arc};

use axum::{
    Router,
    extract::{Json, Path, Query, State},
    routing::{get, post},
};

use http::StatusCode;
use macworp_core::{
    requests::NewProjectRequest, responses::ProjectResponse, routing::ProjectsRouting,
};
use sea_orm::{
    ColumnTrait, EntityTrait, ModelTrait, QueryFilter, TransactionError, TransactionTrait,
};
use serde::Deserialize;

use crate::{
    authentication::middleware::RequireAuth,
    controllers::errors::ProjectsControllerError,
    filesystems::is_filesystem::IsFilesystem,
    models::{ActiveProject, Project, ProjectColumn, ProjectTable},
    state::State as AppState,
};

#[derive(Deserialize)]
pub struct ProjectQueryParams {
    /// Project ID to fetch
    pub subpath: Option<String>,
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
                    owner_username: user.username.clone(),
                    files: Vec::new(),
                    directories: Vec::new(),
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
            owner_username: user.username.clone(),
            files: Vec::new(),
            directories: Vec::new(),
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

        let subpath = match query_params.subpath.as_ref() {
            Some(subpath) => {
                let decoded_subpath = urlencoding::decode(subpath)
                    .map_err(|_| ProjectsControllerError::InvalidProjectPath(subpath.to_string()))?
                    .to_string();
                Some(PathBuf::from(decoded_subpath))
            }
            None => None,
        };

        let content = F::content(state.as_ref(), &project, subpath)
            .map_err(ProjectsControllerError::Filesystem)?;

        Ok(Json(ProjectResponse {
            id: project.id,
            name: project.name,
            owner_username: user.username.clone(),
            files: content
                .files
                .into_iter()
                .map(|file| file.to_string_lossy().to_string())
                .collect(),
            directories: content
                .directories
                .into_iter()
                .map(|dir| dir.to_string_lossy().to_string())
                .collect(),
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
}
