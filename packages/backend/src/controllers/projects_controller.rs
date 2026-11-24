use std::{path::PathBuf, sync::Arc};

use axum::{
    Router,
    extract::{Json, Query, Request, State},
    response::IntoResponse,
    routing::{get, post, put},
};

use http::{HeaderMap, StatusCode};
use macworp_core::{
    entities::Role,
    http::headers::ContentRangeHeaderValue,
    paths::{REFERENCE_PREFIX, strip_reference_prefix},
    requests::{
        NewProjectRequest, ProjectCreateFolderRequest, ProjectPathDeletionRequest,
        ProjectWriteTextRequest,
    },
    responses::{ProjectResponse, ProjectTextResponse},
    routing::ProjectsRouting,
};
use sea_orm::{EntityTrait, ModelTrait, QueryFilter, TransactionError, TransactionTrait};
use serde::Deserialize;
use thiserror::Error;

use crate::{
    authentication::middleware::{ListScope, RequireUser},
    authorization::{
        Authorized, Editor, OneTimeTokenAuthorized, Owner, Principal, Resource, Viewer,
    },
    filesystems::{
        download::Download, is_filesystem::IsFilesystem, project_content::ProjectContent,
    },
    models::{ActiveProject, Project, ProjectTable, User},
    state::State as AppState,
};

#[derive(Error, Debug)]
pub enum Error {
    #[error("{0}")]
    Database(#[from] sea_orm::DbErr),
    #[error("{0}")]
    Authentication(#[from] crate::authentication::error::Error),
    #[error("{0}")]
    Share(#[from] crate::authorization::ShareError),
    #[error("Errors while accessing filesystem: {0}")]
    Filesystem(#[from] crate::filesystems::errors::Error),
    #[error("Missing Content-Range header")]
    MissingContentRangeHeader,
    #[error("Invalid Content-Range header: {0}")]
    InvalidContentRangeHeader(macworp_core::http::headers::Error),
    /// The path starts with the name of the virtual reference data folder, 400
    #[error("`{}` is reserved for the global reference data", REFERENCE_PREFIX)]
    ReservedPath,
}

impl IntoResponse for Error {
    fn into_response(self) -> axum::response::Response {
        if !matches!(self, Error::ReservedPath) {
            tracing::error!("{self}");
        }
        match self {
            Error::Authentication(e) => e.into_response(),
            Error::ReservedPath => (StatusCode::BAD_REQUEST, self.to_string()).into_response(),
            Error::Filesystem(crate::filesystems::errors::Error::SymlinkNotAllowed(_)) => {
                (StatusCode::FORBIDDEN, "Symlinks are not allowed").into_response()
            }
            Error::Filesystem(crate::filesystems::errors::Error::FileNotFound(_)) => {
                StatusCode::NOT_FOUND.into_response()
            }
            Error::Filesystem(crate::filesystems::errors::Error::TextFileTooLarge(_)) => {
                StatusCode::PAYLOAD_TOO_LARGE.into_response()
            }
            Error::Filesystem(crate::filesystems::errors::Error::NotText) => {
                StatusCode::UNPROCESSABLE_ENTITY.into_response()
            }
            _ => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
        }
    }
}

#[derive(Deserialize)]
pub struct ProjectQueryParams {
    /// Project ID to fetch
    pub subpath: PathBuf,
}

/// Query of the text file endpoints.
#[derive(Deserialize)]
pub struct ProjectTextQuery {
    /// File, relative to the project
    pub path: PathBuf,
}

/// Query of the download endpoint.
#[derive(Deserialize)]
pub struct ProjectDownloadQuery {
    /// File or folder, relative to the project; empty for the whole project
    #[serde(default)]
    pub path: PathBuf,
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
            .route(ProjectsRouting::READ_TEXT_PATH, get(Self::read_text))
            .route(ProjectsRouting::WRITE_TEXT_PATH, put(Self::write_text))
            .route(ProjectsRouting::DOWNLOAD_PATH, get(Self::download))
            .with_state(state)
    }

    /// Lists the projects the user holds at least the Viewer role on, with `?all=true`
    /// (administrators only) every project. Without directory content.
    pub async fn index(
        State(state): State<Arc<AppState>>,
        scope: ListScope,
    ) -> Result<Json<Vec<ProjectResponse>>, Error> {
        let projects: Vec<Project> = ProjectTable::find()
            .filter(scope.filter(Project::visible_to(scope.user.id, Role::Viewer)))
            .all(state.db())
            .await
            .map_err(Error::Database)?;
        let owners =
            User::usernames(state.db(), projects.iter().filter_map(|p| p.owner_id)).await?;
        let ownerless = Project::ownerless_among(state.db(), projects.iter().map(|p| p.id))
            .await
            .map_err(Error::Database)?;

        Ok(Json(
            projects
                .into_iter()
                .map(|project| ProjectResponse {
                    id: project.id,
                    name: project.name,
                    created_at: project.created_at,
                    owner_username: project
                        .owner_id
                        .and_then(|id| owners.get(&id).cloned())
                        .unwrap_or_default(),
                    ownerless: ownerless.contains(&project.id),
                    current: PathBuf::new(),
                    files: Vec::new(),
                    directories: Vec::new(),
                    read_only: false,
                    local_path: None,
                    my_role: None,
                })
                .collect(),
        ))
    }

    #[allow(clippy::new_ret_no_self)]
    pub async fn new(
        State(state): State<Arc<AppState>>,
        RequireUser(user): RequireUser,
        Json(payload): Json<NewProjectRequest>,
    ) -> Result<Json<ProjectResponse>, Error> {
        let state_clone = state.clone();
        let project = state
            .db()
            .transaction::<_, Project, Error>(|txn| {
                Box::pin(async move {
                    let new_project = ActiveProject::builder()
                        .set_name(payload.name.clone())
                        .set_created_at(chrono::Utc::now())
                        .set_path(payload.path.to_string_lossy().to_string())
                        .set_owner_id(user.id);

                    let project: Project = new_project
                        .insert(txn)
                        .await
                        .map_err(Error::Database)?
                        .into();

                    Project::shares(txn, project.id)
                        .grant(Principal::User(user.id), Role::Owner)
                        .await?;

                    F::create_project_directory(state_clone.as_ref(), &project)
                        .await
                        .map_err(Error::Filesystem)?;

                    Ok(project)
                })
            })
            .await
            .map_err(|err| match err {
                TransactionError::Connection(err) => Error::Database(err),
                TransactionError::Transaction(err) => err,
            })?;

        Ok(Json(ProjectResponse {
            id: project.id,
            name: project.name,
            created_at: project.created_at,
            owner_username: user.username.clone(),
            ownerless: false,
            current: PathBuf::new(),
            files: Vec::new(),
            directories: Vec::new(),
            read_only: false,
            local_path: None,
            my_role: Some(Role::Owner),
        }))
    }

    pub async fn edit(
        State(state): State<Arc<AppState>>,
        Authorized {
            model: project,
            role,
            ..
        }: Authorized<Project, Viewer>,
        Query(query_params): Query<ProjectQueryParams>,
    ) -> Result<Json<ProjectResponse>, Error> {
        let (content, read_only) = match strip_reference_prefix(&query_params.subpath) {
            // The virtual reference folder shows the instance-wide reference data
            Some(rest) => {
                let base = state.reference_directory().get();
                let requested = F::join_paths(&base, &rest).map_err(Error::Filesystem)?;
                let mut content =
                    ProjectContent::try_from((&base, &requested)).map_err(Error::Filesystem)?;
                // `join` with an empty path would append a trailing separator
                if !content.current.as_os_str().is_empty() {
                    content.current = macworp_core::paths::to_portable(
                        &std::path::Path::new(REFERENCE_PREFIX).join(content.current),
                    );
                } else {
                    content.current = std::path::PathBuf::from(REFERENCE_PREFIX);
                }
                (content, true)
            }
            None => {
                let mut content = F::content(state.as_ref(), &project, query_params.subpath)
                    .map_err(Error::Filesystem)?;
                if content.current.as_os_str().is_empty() {
                    // A project folder of that name is hidden: the name belongs to the reference
                    // data, which is always listed first
                    content.directories.retain(|name| name != REFERENCE_PREFIX);
                    content.directories.sort();
                    content.directories.insert(0, REFERENCE_PREFIX.to_string());
                }
                (content, false)
            }
        };
        let local_path = F::local_path(state.as_ref(), &project);

        Ok(Json(ProjectResponse {
            id: project.id,
            name: project.name,
            created_at: project.created_at,
            owner_username: User::usernames(state.db(), project.owner_id)
                .await?
                .into_values()
                .next()
                .unwrap_or_default(),
            ownerless: Project::ownerless_among(state.db(), [project.id])
                .await
                .map_err(Error::Database)?
                .contains(&project.id),
            current: content.current,
            files: content.files,
            directories: content.directories,
            read_only,
            local_path,
            my_role: Some(role),
        }))
    }

    async fn delete(
        State(state): State<Arc<AppState>>,
        Authorized { model: project, .. }: Authorized<Project, Owner>,
    ) -> Result<StatusCode, Error> {
        state
            .clone()
            .db()
            .transaction::<_, (), Error>(|txn| {
                Box::pin(async move {
                    F::delete_project_directory(state.as_ref(), &project)
                        .await
                        .map_err(Error::Filesystem)?;

                    Project::shares(txn, project.id)
                        .delete_all()
                        .await
                        .map_err(Error::Database)?;
                    project.delete(txn).await.map_err(Error::Database)?;

                    Ok(())
                })
            })
            .await
            .map_err(|err| match err {
                TransactionError::Connection(err) => Error::Database(err),
                TransactionError::Transaction(err) => err,
            })?;

        Ok(StatusCode::OK)
    }

    pub async fn upload_file(
        State(state): State<Arc<AppState>>,
        Authorized { model: project, .. }: Authorized<Project, Editor>,
        Query(params): Query<ProjectFileUploadQuery>,
        headers: HeaderMap,
        request: Request,
    ) -> Result<StatusCode, Error> {
        let content_header_value = headers
            .get("Content-Range")
            .ok_or(Error::MissingContentRangeHeader)?;

        let content_range = ContentRangeHeaderValue::try_from(content_header_value)
            .map_err(Error::InvalidContentRangeHeader)?;

        Self::ensure_not_reserved(&params.file_path)?;
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
        Authorized { model: project, .. }: Authorized<Project, Editor>,
        Json(payload): Json<ProjectCreateFolderRequest>,
    ) -> Result<StatusCode, Error> {
        Self::ensure_not_reserved(&payload.folder_path)?;
        F::create_folder(state.as_ref(), &project, payload.folder_path)
            .await
            .map_err(Error::Filesystem)?;

        Ok(StatusCode::OK)
    }

    /// Content of a small text file of the project (Viewer). 404 if missing, 413 above
    /// `TEXT_FILE_LIMIT`, 422 if not UTF-8.
    pub async fn read_text(
        State(state): State<Arc<AppState>>,
        Authorized { model: project, .. }: Authorized<Project, Viewer>,
        Query(params): Query<ProjectTextQuery>,
    ) -> Result<Json<ProjectTextResponse>, Error> {
        Self::ensure_not_reserved(&params.path)?;
        let content = F::read_text(state.as_ref(), &project, params.path).await?;
        Ok(Json(ProjectTextResponse { content }))
    }

    /// Download of a file of the project as it is, or of a folder as a zip archive that is built
    /// while it streams (Viewer). The user may authenticate with a one-time token in the query.
    /// Paths below the virtual reference folder are served from the reference data. 404 if the
    /// path does not exist, 403 if it passes a symlink.
    pub async fn download(
        State(state): State<Arc<AppState>>,
        OneTimeTokenAuthorized(Authorized { model: project, .. }): OneTimeTokenAuthorized<
            Project,
            Viewer,
        >,
        Query(params): Query<ProjectDownloadQuery>,
    ) -> Result<axum::response::Response, Error> {
        let (base, path, fallback_name, skip_reference_folder) =
            match strip_reference_prefix(&params.path) {
                Some(rest) => (
                    state.reference_directory().get(),
                    rest,
                    "reference-data".to_string(),
                    false,
                ),
                None => (
                    F::project_path(state.as_ref(), &project)?,
                    params.path,
                    project.name.clone(),
                    true,
                ),
            };
        let resolved = F::join_paths(&base, &path)?;
        let name = resolved
            .file_name()
            .filter(|_| !path.as_os_str().is_empty())
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or(fallback_name);
        let mut download = Download::new(resolved, name);
        if skip_reference_folder && path.as_os_str().is_empty() {
            // The folder only exists in listings; a real folder of that name is hidden by it
            download = download.skipping_top_level(REFERENCE_PREFIX);
        }
        Ok(download.into_response().await?)
    }

    /// Replaces or creates a small text file of the project (Editor), atomically.
    pub async fn write_text(
        State(state): State<Arc<AppState>>,
        Authorized { model: project, .. }: Authorized<Project, Editor>,
        Query(params): Query<ProjectTextQuery>,
        Json(payload): Json<ProjectWriteTextRequest>,
    ) -> Result<StatusCode, Error> {
        Self::ensure_not_reserved(&params.path)?;
        F::write_text(state.as_ref(), &project, params.path, payload.content).await?;
        Ok(StatusCode::OK)
    }

    /// Rejects paths below the virtual reference folder: it is not part of the project, and a
    /// real project folder of that name would be hidden by it.
    ///
    /// # Arguments
    /// * `path` - Path relative to the project, from the request
    fn ensure_not_reserved(path: &std::path::Path) -> Result<(), Error> {
        if strip_reference_prefix(path).is_some() {
            return Err(Error::ReservedPath);
        }
        Ok(())
    }

    pub async fn delete_path(
        State(state): State<Arc<AppState>>,
        Authorized { model: project, .. }: Authorized<Project, Editor>,
        Json(payload): Json<ProjectPathDeletionRequest>,
    ) -> Result<StatusCode, Error> {
        Self::ensure_not_reserved(&payload.path)?;
        F::delete_path(state.as_ref(), &project, payload.path)
            .await
            .map_err(Error::Filesystem)?;

        Ok(StatusCode::OK)
    }
}
