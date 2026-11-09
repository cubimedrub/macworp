use std::{
    io::SeekFrom,
    path::{Path, PathBuf},
    pin::pin,
};

use axum::{BoxError, body::Bytes};
use futures_util::{Stream, TryStreamExt};
use macworp_core::{http::headers::ContentRangeHeaderValue, responses::StorageProbe};
use tokio::{
    fs::{OpenOptions, metadata},
    io::{AsyncSeekExt, BufWriter},
};
use tokio_util::io::StreamReader;

use crate::{
    filesystems::{
        errors::{Error, RemoteFilesystemError},
        is_filesystem::IsFilesystem,
        project_content::ProjectContent,
    },
    models::Project,
    state::State,
};

/// Name of the marker file in the projects directory, see [`IsFilesystem::storage_probe`]
pub const STORAGE_MARKER_FILE: &str = ".macworp-storage-id";

const PARTIAL_UPLOAD_EXTENSION: &str = "macworp_partial_upload";

pub struct RemoteFilesystem;

impl RemoteFilesystem {
    /// Creates an empty file to file to reserve storage space to make sure the incomming content
    /// can be stored.
    ///
    /// # Arguments
    /// * `target_path` - The path where the file should be created.
    /// * `size` - The size of the file to be created in bytes.
    ///
    async fn reserve_file(target_path: &Path, size: u64) -> Result<(), Error> {
        let file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(target_path)
            .await?;

        file.set_len(size)
            .await
            .map_err(RemoteFilesystemError::ReserveFile)?; // 50 MiB
        Ok(())
    }
}

impl IsFilesystem for RemoteFilesystem {
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

        let target_path = Self::join_paths(&project_path, &file_path)?;
        let partial_upload_path = Self::join_paths(
            &project_path,
            &file_path.with_extension(PARTIAL_UPLOAD_EXTENSION),
        )?;

        tokio::fs::create_dir_all(target_path.parent().ok_or_else(|| {
            Error::RemoteFilesystem(RemoteFilesystemError::NoParentPath(target_path.clone()))
        })?)
        .await
        .map_err(|err| {
            Error::RemoteFilesystem(RemoteFilesystemError::CreateProjectSubDirectory(err))
        })?;

        if target_path.is_dir() || partial_upload_path.is_dir() {
            return Err(Error::RemoteFilesystem(
                RemoteFilesystemError::TargetPathIsDirectory(target_path),
            ));
        }

        if !partial_upload_path.is_file() {
            Self::reserve_file(
                &partial_upload_path,
                content_range.size().ok_or(Error::RemoteFilesystem(
                    RemoteFilesystemError::MissingFileSize,
                ))?,
            )
            .await?;
        } else {
            // Get the file size of the existing file
            let target_size = metadata(&partial_upload_path)
                .await
                .map_err(|err| {
                    Error::RemoteFilesystem(RemoteFilesystemError::GetFileMetadataFailed(err))
                })?
                .len();

            // If it is not equal, call reserve_file to resize accordingly
            if target_size
                != content_range.size().ok_or(Error::RemoteFilesystem(
                    RemoteFilesystemError::MissingFileSize,
                ))?
            {
                Self::reserve_file(
                    &partial_upload_path,
                    content_range.size().ok_or(Error::RemoteFilesystem(
                        RemoteFilesystemError::MissingFileSize,
                    ))?,
                )
                .await?;
            }
        }

        async {
            // Convert the stream into an `AsyncRead`.
            let body_with_io_error = stream.map_err(std::io::Error::other);
            let mut body_reader = pin!(StreamReader::new(body_with_io_error));

            // Create the file. `File` implements `AsyncWrite`.
            let mut file = OpenOptions::new()
                .write(true)
                .open(&partial_upload_path)
                .await?;
            let mut writer = BufWriter::new(&mut file);
            writer
                .seek(SeekFrom::Start(content_range.range().0))
                .await?;

            // Copy the body into the file.
            tokio::io::copy(&mut body_reader, &mut writer).await?;

            Ok::<_, std::io::Error>(())
        }
        .await
        .map_err(|err| Error::RemoteFilesystem(RemoteFilesystemError::StreamToFile(err)))?;

        if content_range.range().1 + 1 == content_range.size().unwrap() {
            tokio::fs::rename(&partial_upload_path, &target_path)
                .await
                .map_err(|err| {
                    Error::RemoteFilesystem(RemoteFilesystemError::FinalizeUploadFailed(err))
                })?;
        }

        Ok(())
    }

    async fn create_folder(
        state: &State,
        project: &Project,
        folder_path: PathBuf,
    ) -> Result<(), Error> {
        let new_folder_path = Self::join_paths(&Self::project_path(state, project)?, &folder_path)?;

        if new_folder_path.is_file() {
            return Err(Error::RemoteFilesystem(
                RemoteFilesystemError::TargetPathIsFile(new_folder_path),
            ));
        }

        tokio::fs::create_dir_all(&new_folder_path)
            .await
            .map_err(|err| {
                Error::RemoteFilesystem(RemoteFilesystemError::CreateProjectSubDirectory(err))
            })
    }

    async fn delete_path(state: &State, project: &Project, path: PathBuf) -> Result<(), Error> {
        let target_path = Self::join_paths(&Self::project_path(state, project)?, &path)?;

        if target_path.is_file() {
            tokio::fs::remove_file(target_path).await.map_err(|err| {
                Error::RemoteFilesystem(RemoteFilesystemError::DeleteFileFailed(err))
            })
        } else if target_path.is_dir() {
            tokio::fs::remove_dir_all(target_path).await.map_err(|err| {
                Error::RemoteFilesystem(RemoteFilesystemError::DeleteDirectoryFailed(err))
            })
        } else {
            Err(Error::RemoteFilesystem(
                RemoteFilesystemError::PathDoesNotExist(path),
            ))
        }
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
