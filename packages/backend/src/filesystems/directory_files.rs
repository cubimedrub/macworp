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

/// Suffix of the hidden file a chunked upload is written to until it is complete.
const PARTIAL_UPLOAD_SUFFIX: &str = "macworp_partial_upload";

/// Suffix of the hidden temporary file [`DirectoryFiles::write_text`] writes before it renames it
/// over the target. Differs from [`PARTIAL_UPLOAD_SUFFIX`] so a text write never touches a
/// chunked upload of the same file.
const PARTIAL_TEXT_SUFFIX: &str = "macworp_partial_text";

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

    /// Name of the hidden sibling a file is written to before it is renamed over `path`:
    /// `.<file name>.<suffix>` in the same folder. The suffix is appended to the whole file name
    /// (never replaces an extension), so `genome.fa` and `genome.fai` get distinct siblings, and
    /// the leading dot keeps unfinished files out of plain `*` globs.
    ///
    /// # Arguments
    /// * `path` - Final path of the file, relative or absolute
    /// * `suffix` - Marker that says what kind of unfinished file it is
    fn partial_path(path: &Path, suffix: &str) -> PathBuf {
        let file_name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        path.with_file_name(format!(".{file_name}.{suffix}"))
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
            &Self::partial_path(&file_path, PARTIAL_UPLOAD_SUFFIX),
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

    /// Reads a small UTF-8 text file below `base`.
    ///
    /// # Arguments
    /// * `base` - Directory the path is confined to
    /// * `path` - File to read, relative to `base`
    /// * `limit` - Largest accepted size in bytes
    pub async fn read_text(base: &Path, path: &Path, limit: usize) -> Result<String, Error> {
        let target_path = Self::join_paths(base, path)?;
        let size = match tokio::fs::metadata(&target_path).await {
            Ok(metadata) if metadata.is_file() => metadata.len(),
            Ok(_) => return Err(Error::FileNotFound(target_path)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Err(Error::FileNotFound(target_path));
            }
            Err(error) => return Err(error.into()),
        };
        if size > limit as u64 {
            return Err(Error::TextFileTooLarge(limit));
        }
        let bytes = tokio::fs::read(&target_path).await?;
        String::from_utf8(bytes).map_err(|_| Error::NotText)
    }

    /// Writes a small text file below `base`: the content goes to a temporary file next to the
    /// target that is renamed over it, so readers never see half a file. Missing parent folders
    /// are created.
    ///
    /// # Arguments
    /// * `base` - Directory the path is confined to
    /// * `path` - File to write, relative to `base`
    /// * `content` - New content
    /// * `limit` - Largest accepted size in bytes
    pub async fn write_text(
        base: &Path,
        path: &Path,
        content: &str,
        limit: usize,
    ) -> Result<(), Error> {
        if content.len() > limit {
            return Err(Error::TextFileTooLarge(limit));
        }
        let target_path = Self::join_paths(base, path)?;
        if target_path.file_name().is_none() {
            return Err(RemoteFilesystemError::NoParentPath(target_path).into());
        }
        let temporary_path =
            Self::join_paths(base, &Self::partial_path(path, PARTIAL_TEXT_SUFFIX))?;
        if let Some(parent) = target_path.parent() {
            tokio::fs::create_dir_all(parent).await.map_err(|error| {
                Error::RemoteFilesystem(RemoteFilesystemError::CreateProjectSubDirectory(error))
            })?;
        }
        if target_path.is_dir() {
            return Err(Error::RemoteFilesystem(
                RemoteFilesystemError::TargetPathIsDirectory(target_path),
            ));
        }
        tokio::fs::write(&temporary_path, content)
            .await
            .map_err(|error| Error::RemoteFilesystem(RemoteFilesystemError::StreamToFile(error)))?;
        tokio::fs::rename(&temporary_path, &target_path)
            .await
            .map_err(|error| {
                Error::RemoteFilesystem(RemoteFilesystemError::FinalizeUploadFailed(error))
            })
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Uploads `content` as one chunk to `name` below `base`.
    ///
    /// # Arguments
    /// * `base` - Directory the upload is confined to
    /// * `name` - Target file name
    /// * `content` - Whole file content
    async fn upload(base: &Path, name: &str, content: &'static [u8]) {
        let size = content.len() as u64;
        DirectoryFiles::stream_to_file(
            base,
            PathBuf::from(name),
            ContentRangeHeaderValue::new("bytes", (0, size - 1), Some(size)),
            futures_util::stream::iter([Ok::<_, std::io::Error>(Bytes::from_static(content))]),
        )
        .await
        .unwrap();
    }

    /// The unfinished file keeps the whole file name, so names that differ only in their
    /// extension never share one, and it is hidden.
    #[test]
    fn partial_path_appends_instead_of_replacing_the_extension() {
        let fasta = DirectoryFiles::partial_path(Path::new("refs/genome.fa"), "p");
        let index = DirectoryFiles::partial_path(Path::new("refs/genome.fai"), "p");
        assert_eq!(fasta, Path::new("refs/.genome.fa.p"));
        assert_eq!(index, Path::new("refs/.genome.fai.p"));
        assert_ne!(fasta, index);
        assert_eq!(
            DirectoryFiles::partial_path(Path::new("noext"), "p"),
            Path::new(".noext.p")
        );
    }

    /// Two uploads of files that differ only in their extension, interleaved chunk by chunk,
    /// both end up with their own content.
    #[tokio::test]
    async fn interleaved_uploads_with_different_extensions_do_not_collide() {
        let directory = tempfile::tempdir().unwrap();
        let base = directory.path();
        let chunk = |name: &str, range: (u64, u64), data: &'static [u8]| {
            DirectoryFiles::stream_to_file(
                base,
                PathBuf::from(name),
                ContentRangeHeaderValue::new("bytes", range, Some(4)),
                futures_util::stream::iter([Ok::<_, std::io::Error>(Bytes::from_static(data))]),
            )
        };
        chunk("genome.fa", (0, 1), b"AA").await.unwrap();
        chunk("genome.fai", (0, 1), b"11").await.unwrap();
        chunk("genome.fa", (2, 3), b"CC").await.unwrap();
        chunk("genome.fai", (2, 3), b"22").await.unwrap();
        assert_eq!(std::fs::read(base.join("genome.fa")).unwrap(), b"AACC");
        assert_eq!(std::fs::read(base.join("genome.fai")).unwrap(), b"1122");
    }

    /// While an upload is unfinished only a hidden file exists, and a finished upload leaves
    /// nothing but the target.
    #[tokio::test]
    async fn unfinished_upload_is_hidden_and_finished_upload_leaves_no_partial() {
        let directory = tempfile::tempdir().unwrap();
        let base = directory.path();
        DirectoryFiles::stream_to_file(
            base,
            PathBuf::from("a.txt"),
            ContentRangeHeaderValue::new("bytes", (0, 1), Some(4)),
            futures_util::stream::iter([Ok::<_, std::io::Error>(Bytes::from_static(b"ab"))]),
        )
        .await
        .unwrap();
        let visible = |base: &Path| {
            std::fs::read_dir(base)
                .unwrap()
                .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
                .filter(|name| !name.starts_with('.'))
                .count()
        };
        assert_eq!(visible(base), 0);
        assert!(base.join(".a.txt.macworp_partial_upload").is_file());
        DirectoryFiles::stream_to_file(
            base,
            PathBuf::from("a.txt"),
            ContentRangeHeaderValue::new("bytes", (2, 3), Some(4)),
            futures_util::stream::iter([Ok::<_, std::io::Error>(Bytes::from_static(b"cd"))]),
        )
        .await
        .unwrap();
        assert_eq!(std::fs::read(base.join("a.txt")).unwrap(), b"abcd");
        assert_eq!(std::fs::read_dir(base).unwrap().count(), 1);
    }

    /// An upload replaces an existing file with a new inode, the old content stays readable
    /// through a hard link taken before (the property the per-run reference snapshot relies on).
    #[tokio::test]
    async fn upload_over_an_existing_file_keeps_hard_links_on_the_old_content() {
        let directory = tempfile::tempdir().unwrap();
        let base = directory.path();
        upload(base, "ref.txt", b"old").await;
        std::fs::hard_link(base.join("ref.txt"), base.join("snapshot.txt")).unwrap();
        upload(base, "ref.txt", b"new").await;
        assert_eq!(std::fs::read(base.join("ref.txt")).unwrap(), b"new");
        assert_eq!(std::fs::read(base.join("snapshot.txt")).unwrap(), b"old");
    }

    /// A text write and a chunked upload of the same file use different temporary files.
    #[tokio::test]
    async fn text_write_does_not_touch_an_unfinished_upload() {
        let directory = tempfile::tempdir().unwrap();
        let base = directory.path();
        DirectoryFiles::stream_to_file(
            base,
            PathBuf::from("a.txt"),
            ContentRangeHeaderValue::new("bytes", (0, 1), Some(4)),
            futures_util::stream::iter([Ok::<_, std::io::Error>(Bytes::from_static(b"ab"))]),
        )
        .await
        .unwrap();
        DirectoryFiles::write_text(base, Path::new("a.txt"), "text", 100)
            .await
            .unwrap();
        assert_eq!(std::fs::read(base.join("a.txt")).unwrap(), b"text");
        assert_eq!(
            std::fs::read(base.join(".a.txt.macworp_partial_upload")).unwrap()[..2],
            *b"ab"
        );
    }
}
