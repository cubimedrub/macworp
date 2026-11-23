use std::sync::Arc;

use axum::{
    Router,
    extract::{Json, Query, Request, State},
    response::IntoResponse,
    routing::{get, post},
};
use http::{HeaderMap, StatusCode};
use macworp_core::{
    http::headers::ContentRangeHeaderValue,
    requests::{ProjectCreateFolderRequest, ProjectPathDeletionRequest},
    responses::ProjectResponse,
    routing::ReferenceRouting,
};
use serde::Deserialize;
use thiserror::Error;

use crate::{
    authorization::{Authorized, Editor, OneTimeTokenAuthorized, Viewer},
    filesystems::{
        directory_files::DirectoryFiles, download::Download, is_filesystem::IsFilesystem,
        project_content::ProjectContent,
    },
    models::ReferenceData,
    state::State as AppState,
};

/// Why a reference data request failed
#[derive(Error, Debug)]
pub enum Error {
    /// Authentication or authorization failed
    #[error("{0}")]
    Authorization(#[from] crate::authorization::Error),
    /// Reading or writing files failed
    #[error("Errors while accessing filesystem: {0}")]
    Filesystem(#[from] crate::filesystems::errors::Error),
    /// Upload without `Content-Range`, 400
    #[error("Missing Content-Range header")]
    MissingContentRangeHeader,
    /// Unparsable `Content-Range`, 400
    #[error("Invalid Content-Range header: {0}")]
    InvalidContentRangeHeader(macworp_core::http::headers::Error),
    /// The filesystem of this deployment does not let the API change reference files, 405
    #[error("Reference data is read-only here, change the folder directly")]
    ReadOnly,
}

impl IntoResponse for Error {
    /// Client mistakes keep their status, everything else is logged and hidden as 500.
    fn into_response(self) -> axum::response::Response {
        match self {
            Self::Authorization(e) => e.into_response(),
            Self::Filesystem(crate::filesystems::errors::Error::SymlinkNotAllowed(_)) => {
                (StatusCode::FORBIDDEN, "Symlinks are not allowed").into_response()
            }
            Self::Filesystem(crate::filesystems::errors::Error::FileNotFound(_)) => {
                StatusCode::NOT_FOUND.into_response()
            }
            Self::MissingContentRangeHeader | Self::InvalidContentRangeHeader(_) => {
                (StatusCode::BAD_REQUEST, self.to_string()).into_response()
            }
            Self::ReadOnly => (StatusCode::METHOD_NOT_ALLOWED, self.to_string()).into_response(),
            Self::Filesystem(_) => {
                tracing::error!("{self}");
                StatusCode::INTERNAL_SERVER_ERROR.into_response()
            }
        }
    }
}

/// Query of the browse endpoint
#[derive(Deserialize)]
pub struct BrowseQuery {
    /// Folder below the reference directory, empty for the top level
    #[serde(default)]
    subpath: std::path::PathBuf,
}

/// Query of the download endpoint
#[derive(Deserialize)]
pub struct DownloadQuery {
    /// File or folder below the reference directory, empty for all of it
    #[serde(default)]
    path: std::path::PathBuf,
}

/// Query of the upload endpoint
#[derive(Deserialize)]
pub struct UploadQuery {
    /// Target file below the reference directory
    file_path: std::path::PathBuf,
}

/// Browse and change the global reference data: everybody reads, Editors and admins write.
pub struct ReferenceController<F: IsFilesystem + 'static> {
    _filesystem: std::marker::PhantomData<F>,
}

impl<F> ReferenceController<F>
where
    F: IsFilesystem,
{
    /// Routes of the controller, see [`ReferenceRouting`].
    ///
    /// # Arguments
    /// * `state` - Shared application state
    pub fn routes(state: Arc<AppState>) -> Router<Arc<AppState>> {
        Router::new()
            .route(ReferenceRouting::EDIT_PATH, get(Self::edit))
            .route(ReferenceRouting::UPLOAD_FILE_PATH, post(Self::upload_file))
            .route(
                ReferenceRouting::CREATE_FOLDER_PATH,
                post(Self::create_folder),
            )
            .route(ReferenceRouting::DELETE_PATH_PATH, post(Self::delete_path))
            .route(ReferenceRouting::DOWNLOAD_PATH, get(Self::download))
            .with_state(state)
    }

    /// Fails with [`Error::ReadOnly`] where [`IsFilesystem::REFERENCE_WRITABLE`] is off.
    fn ensure_writable() -> Result<(), Error> {
        if F::REFERENCE_WRITABLE {
            Ok(())
        } else {
            Err(Error::ReadOnly)
        }
    }

    /// Download of a reference file as it is, or of a folder as a zip archive that is built while
    /// it streams (Viewer). The user may authenticate with a one-time token in the query. 404 if
    /// the path does not exist, 403 if it passes a symlink.
    async fn download(
        State(state): State<Arc<AppState>>,
        OneTimeTokenAuthorized(_): OneTimeTokenAuthorized<ReferenceData, Viewer>,
        Query(query): Query<DownloadQuery>,
    ) -> Result<axum::response::Response, Error> {
        let base = state.reference_directory().get();
        let resolved = F::join_paths(&base, &query.path)?;
        let name = resolved
            .file_name()
            .filter(|_| !query.path.as_os_str().is_empty())
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| "reference-data".to_string());
        Ok(Download::new(resolved, name).into_response().await?)
    }

    /// Lists a folder of the reference data.
    async fn edit(
        State(state): State<Arc<AppState>>,
        Authorized { role, .. }: Authorized<ReferenceData, Viewer>,
        Query(query): Query<BrowseQuery>,
    ) -> Result<Json<ProjectResponse>, Error> {
        let base = state.reference_directory().get();
        let requested = F::join_paths(&base, &query.subpath)?;
        let content = ProjectContent::try_from((&base, &requested))?;
        Ok(Json(ProjectResponse {
            id: ReferenceRouting::SINGLETON_ID,
            name: "Global reference data".to_string(),
            created_at: chrono::DateTime::UNIX_EPOCH,
            owner_username: String::new(),
            ownerless: false,
            current: content.current,
            files: content.files,
            directories: content.directories,
            local_path: F::local_reference_path(&base),
            my_role: Some(role),
            read_only: false,
        }))
    }

    /// Stores one chunk of an uploaded file.
    async fn upload_file(
        State(state): State<Arc<AppState>>,
        Authorized { user, .. }: Authorized<ReferenceData, Editor>,
        Query(query): Query<UploadQuery>,
        headers: HeaderMap,
        request: Request,
    ) -> Result<StatusCode, Error> {
        Self::ensure_writable()?;
        let content_range = ContentRangeHeaderValue::try_from(
            headers
                .get("Content-Range")
                .ok_or(Error::MissingContentRangeHeader)?,
        )
        .map_err(Error::InvalidContentRangeHeader)?;
        Self::audit(&user, "upload", &query.file_path);
        DirectoryFiles::stream_to_file(
            &state.reference_directory().get(),
            query.file_path,
            content_range,
            request.into_body().into_data_stream(),
        )
        .await?;
        Ok(StatusCode::OK)
    }

    /// Creates a folder.
    async fn create_folder(
        State(state): State<Arc<AppState>>,
        Authorized { user, .. }: Authorized<ReferenceData, Editor>,
        Json(payload): Json<ProjectCreateFolderRequest>,
    ) -> Result<StatusCode, Error> {
        Self::ensure_writable()?;
        Self::audit(&user, "create folder", &payload.folder_path);
        DirectoryFiles::create_folder(&state.reference_directory().get(), payload.folder_path)
            .await?;
        Ok(StatusCode::OK)
    }

    /// Deletes a file or folder.
    async fn delete_path(
        State(state): State<Arc<AppState>>,
        Authorized { user, .. }: Authorized<ReferenceData, Editor>,
        Json(payload): Json<ProjectPathDeletionRequest>,
    ) -> Result<StatusCode, Error> {
        Self::ensure_writable()?;
        Self::audit(&user, "delete", &payload.path);
        DirectoryFiles::delete_path(&state.reference_directory().get(), payload.path).await?;
        Ok(StatusCode::OK)
    }

    /// Logs a change of the shared data, target `audit`.
    ///
    /// # Arguments
    /// * `user` - Who changes it
    /// * `action` - What they do
    /// * `path` - What they change, relative to the reference directory
    fn audit(user: &crate::models::User, action: &str, path: &std::path::Path) {
        tracing::info!(
            target: "audit",
            user = %user.username,
            path = %path.display(),
            "reference data: {action}"
        );
    }
}
