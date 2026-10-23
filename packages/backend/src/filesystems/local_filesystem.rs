use std::path::PathBuf;

use axum::{BoxError, body::Bytes};
use futures_util::Stream;
use macworp_core::http::headers::ContentRangeHeaderValue;

use crate::{
    filesystems::{
        errors::{Error, LocalFilesystemError},
        is_filesystem::IsFilesystem,
        project_content::ProjectContent,
    },
    models::Project,
    state::State,
};

/// Implementation of a local filesystems. Basically read only. Users have to manage data via native
/// file browser.
///
pub struct LocalFilesystem;

impl IsFilesystem for LocalFilesystem {
    fn content(
        state: &State,
        project: &Project,
        subpath: PathBuf,
    ) -> Result<ProjectContent, Error> {
        let project_path = Self::project_path(state, project)?;

        let requested_path = Self::join_paths(&project_path, &subpath)?;

        ProjectContent::try_from((&project_path, &requested_path))
    }

    fn project_path(_state: &State, project: &Project) -> Result<PathBuf, Error> {
        Ok(PathBuf::from(
            project
                .path
                .as_ref()
                .ok_or(LocalFilesystemError::ProjectPathMissing)?,
        ))
    }

    fn local_path(state: &State, project: &Project) -> Option<PathBuf> {
        Self::project_path(state, project).ok()
    }

    async fn create_project_directory(state: &State, project: &Project) -> Result<PathBuf, Error> {
        let path = Self::project_path(state, project)?;

        Ok(path)
    }

    async fn delete_project_directory(_state: &State, _project: &Project) -> Result<(), Error> {
        // Not messing around with local filesystem
        Ok(())
    }

    async fn stream_to_file<S, E>(
        _state: &State,
        _project: &Project,
        _file_path: PathBuf,
        _content_range: ContentRangeHeaderValue,
        _stream: S,
    ) -> Result<(), Error>
    where
        S: Stream<Item = Result<Bytes, E>>,
        E: Into<BoxError>,
    {
        Ok(())
    }

    async fn create_folder(
        _state: &State,
        _project: &Project,
        _folder_path: PathBuf,
    ) -> Result<(), Error> {
        Ok(())
    }

    async fn delete_path(_state: &State, _project: &Project, _path: PathBuf) -> Result<(), Error> {
        Ok(())
    }
}
