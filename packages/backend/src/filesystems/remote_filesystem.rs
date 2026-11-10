use std::path::PathBuf;

use axum::{BoxError, body::Bytes};
use futures_util::Stream;
use macworp_core::{http::headers::ContentRangeHeaderValue, responses::StorageProbe};

use crate::{
    filesystems::{
        directory_files::DirectoryFiles,
        errors::{Error, RemoteFilesystemError},
        is_filesystem::IsFilesystem,
        project_content::ProjectContent,
    },
    models::Project,
    state::State,
};

/// Name of the marker file in the projects directory, see [`IsFilesystem::storage_probe`]
pub const STORAGE_MARKER_FILE: &str = ".macworp-storage-id";

pub struct RemoteFilesystem;

impl IsFilesystem for RemoteFilesystem {
    const REFERENCE_WRITABLE: bool = true;

    fn content(
        state: &State,
        project: &Project,
        subpath: PathBuf,
    ) -> Result<ProjectContent, Error> {
        let project_path =
            PathBuf::from(state.config().projects_path()).join(project.id.to_string());

        let requested_path = Self::join_paths(&project_path, &subpath)?;

        ProjectContent::try_from((&project_path, &requested_path))
    }

    fn project_path(state: &State, project: &Project) -> Result<PathBuf, Error> {
        Ok(PathBuf::from(state.config().projects_path()).join(project.id.to_string()))
    }

    async fn storage_probe(state: &State) -> Result<Option<StorageProbe>, Error> {
        let projects_path = std::path::absolute(state.config().projects_path())?;
        let path = projects_path.join(STORAGE_MARKER_FILE);
        let content = match tokio::fs::read_to_string(&path).await {
            Ok(content) if !content.trim().is_empty() => content.trim().to_string(),
            _ => {
                tokio::fs::create_dir_all(&projects_path).await?;
                let content = crate::configuration::generate_secret();
                tokio::fs::write(&path, &content).await?;
                content
            }
        };
        Ok(Some(StorageProbe { path, content }))
    }

    async fn create_project_directory(state: &State, project: &Project) -> Result<PathBuf, Error> {
        let project_path = Self::project_path(state, project)?;

        tokio::fs::create_dir_all(&project_path)
            .await
            .map_err(|e| {
                Error::RemoteFilesystem(RemoteFilesystemError::CreateProjectDirectoryFailed(e))
            })?;

        Ok(project_path)
    }

    async fn delete_project_directory(state: &State, project: &Project) -> Result<(), Error> {
        let project_path = Self::project_path(state, project)?;

        tokio::fs::remove_dir_all(project_path)
            .await
            .map_err(|err| {
                Error::RemoteFilesystem(RemoteFilesystemError::DeleteProjectDirectory(err))
            })?;
        Ok(())
    }

    async fn stream_to_file<S, E>(
        state: &State,
        project: &Project,
        file_path: PathBuf,
        content_range: ContentRangeHeaderValue,
        stream: S,
    ) -> Result<(), Error>
    where
        S: Stream<Item = Result<Bytes, E>>,
        E: Into<BoxError>,
    {
        let project_path = Self::project_path(state, project)?;
        DirectoryFiles::stream_to_file(&project_path, file_path, content_range, stream).await
    }

    async fn create_folder(
        state: &State,
        project: &Project,
        folder_path: PathBuf,
    ) -> Result<(), Error> {
        DirectoryFiles::create_folder(&Self::project_path(state, project)?, folder_path).await
    }

    async fn delete_path(state: &State, project: &Project, path: PathBuf) -> Result<(), Error> {
        DirectoryFiles::delete_path(&Self::project_path(state, project)?, path).await
    }
}

#[cfg(all(test, feature = "standalone", feature = "integration"))]
mod tests {
    use super::*;
    use crate::configuration::Configuration;

    /// The marker is created once under the projects directory and keeps its content
    #[tokio::test]
    async fn storage_probe_is_created_once_and_stable() {
        let directory = tempfile::tempdir().unwrap();
        let mut configuration = Configuration::default();
        *configuration.projects_path_mut() =
            directory.path().join("projects").display().to_string();
        *configuration.database_url_mut() =
            format!("sqlite://{}/t.db?mode=rwc", directory.path().display());
        let state = State::new_for_integration(configuration).await.unwrap();

        let first = RemoteFilesystem::storage_probe(&state)
            .await
            .unwrap()
            .unwrap();
        assert!(first.path.is_absolute());
        assert!(first.path.ends_with(STORAGE_MARKER_FILE));
        assert!(!first.content.is_empty());
        assert_eq!(std::fs::read_to_string(&first.path).unwrap(), first.content);
        let second = RemoteFilesystem::storage_probe(&state)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(first, second);

        // A damaged (empty) marker is replaced
        std::fs::write(&first.path, "").unwrap();
        let third = RemoteFilesystem::storage_probe(&state)
            .await
            .unwrap()
            .unwrap();
        assert!(!third.content.is_empty());
    }
}
