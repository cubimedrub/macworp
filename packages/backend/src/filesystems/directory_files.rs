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

use crate::filesystems::errors::{Error, RemoteFilesystemError};

/// Extension of the file a chunked upload is written to until it is complete.
const PARTIAL_UPLOAD_EXTENSION: &str = "macworp_partial_upload";

/// File operations confined to one base directory on the local disk of the backend. Shared by
/// the project storage of the remote filesystem and the global reference data.
pub struct DirectoryFiles;

impl DirectoryFiles {
    /// Securly joins two pathes. Each non normal compontent of the subpath, e.g. Root or Parent
    /// is filtered out to prevent breaking out of the base path. Existing components below the
    /// base that are symlinks are rejected. The base itself may be a symlink.
    ///
    /// Note: check and later use are not atomic, so a symlink created in between is not caught.
    ///
    /// # Arguments
    /// * `base` - The base path to join to
    /// * `subpath` - The subpath to join to the base path
    pub fn join_paths(base: &Path, subpath: &Path) -> Result<PathBuf, Error> {
        let mut final_path = base.to_path_buf();
        let mut exists = true;
        for comp in macworp_core::paths::sanitize_relative(subpath).components() {
            final_path.push(comp);
            if exists {
                match std::fs::symlink_metadata(&final_path) {
                    Ok(meta) if meta.file_type().is_symlink() => {
                        return Err(Error::SymlinkNotAllowed(final_path));
                    }
                    Ok(_) => {}
                    // Nothing below a missing path can be a symlink
                    Err(err) if err.kind() == std::io::ErrorKind::NotFound => exists = false,
                    Err(err) => return Err(err.into()),
                }
            }
        }
        Ok(final_path)
    }

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

    /// Streams one chunk of an upload into `file_path` below `base`, finishing the file when the last byte arrived. Partial uploads live next to the target.
    ///
    /// # Arguments
    /// * `project_path` - Directory the upload is confined to
    /// * `file_path` - Target file, relative to `project_path`
    /// * `content_range` - Range of this chunk and total size
    /// * `stream` - The chunk's bytes
    pub async fn stream_to_file<S, E>(
        project_path: &Path,
        file_path: PathBuf,
        content_range: ContentRangeHeaderValue,
        stream: S,
    ) -> Result<(), Error>
    where
        S: Stream<Item = Result<Bytes, E>>,
        E: Into<BoxError>,
    {
        let target_path = Self::join_paths(project_path, &file_path)?;
        let partial_upload_path = Self::join_paths(
            project_path,
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

    /// Creates the folder `folder_path` below `base`.
    ///
    /// # Arguments
    /// * `base` - Directory the folder is confined to
    /// * `folder_path` - Folder to create, relative to `base`
    pub async fn create_folder(base: &Path, folder_path: PathBuf) -> Result<(), Error> {
        let new_folder_path = Self::join_paths(base, &folder_path)?;

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

    /// Deletes the file or folder `path` below `base`.
    ///
    /// # Arguments
    /// * `base` - Directory the path is confined to
    /// * `path` - File or folder to delete, relative to `base`
    pub async fn delete_path(base: &Path, path: PathBuf) -> Result<(), Error> {
        let target_path = Self::join_paths(base, &path)?;

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
