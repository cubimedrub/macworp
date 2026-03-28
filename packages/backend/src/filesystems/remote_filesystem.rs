use std::{
    io::SeekFrom,
    path::{Path, PathBuf},
    pin::pin,
};

use axum::{BoxError, body::Bytes};
use futures_util::{Stream, TryStreamExt};
use macworp_core::http::headers::ContentRangeHeaderValue;
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

        let requested_path = Self::join_paths(&project_path, &subpath);

        ProjectContent::try_from((&project_path, &requested_path))
    }

    fn project_path(state: &State, project: &Project) -> Result<PathBuf, Error> {
        Ok(PathBuf::from(state.config().projects_path()).join(project.id.to_string()))
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

        let target_path = Self::join_paths(&project_path, &file_path);

        tokio::fs::create_dir_all(target_path.parent().ok_or_else(|| {
            Error::RemoteFilesystem(RemoteFilesystemError::NoParentPath(target_path.clone()))
        })?)
        .await
        .map_err(|err| {
            Error::RemoteFilesystem(RemoteFilesystemError::CreateProjectSubDirectory(err))
        })?;

        if target_path.is_dir() {
            return Err(Error::RemoteFilesystem(
                RemoteFilesystemError::TargetPathIsDirectory(target_path),
            ));
        }

        if !target_path.is_file() {
            Self::reserve_file(
                &target_path,
                content_range.size().ok_or(Error::RemoteFilesystem(
                    RemoteFilesystemError::MissingFileSize,
                ))?,
            )
            .await?;
        } else {
            // Get the file size of the existing file
            let target_size = metadata(&target_path)
                .await
                .map_err(|err| {
                    Error::RemoteFilesystem(RemoteFilesystemError::GetFileMetadataFailed(err))
                })?
                .len();

            // If it is too not equal call reserve_file to resize accordingly
            if target_size
                != content_range.size().ok_or(Error::RemoteFilesystem(
                    RemoteFilesystemError::MissingFileSize,
                ))?
            {
                Self::reserve_file(
                    &target_path,
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
            let mut file = OpenOptions::new().write(true).open(target_path).await?;
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

        Ok(())
    }
}
