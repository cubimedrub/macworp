use std::path::PathBuf;

use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("Project path is not a directory but file")]
    ProjectPathIsFile,

    #[error("Unable to read content of project director: {0}")]
    ReadDir(#[from] std::io::Error),

    #[error("Symlinks are not allowed in project paths: {0:?}")]
    SymlinkNotAllowed(PathBuf),

    #[error(transparent)]
    LocalFilesystem(#[from] LocalFilesystemError),

    #[error(transparent)]
    RemoteFilesystem(#[from] RemoteFilesystemError),
}

#[derive(Error, Debug)]
pub enum LocalFilesystemError {
    #[error("Project path is not set for project with id")]
    ProjectPathMissing,
}

#[derive(Error, Debug)]
pub enum RemoteFilesystemError {
    #[error("Failed to create project directory")]
    CreateProjectDirectoryFailed(#[source] std::io::Error),
    #[error("Failed to delete project directory")]
    DeleteProjectDirectory(#[source] std::io::Error),
    #[error("Failed to stream data to file: {0}")]
    StreamToFile(#[source] std::io::Error),
    #[error("Path `{0:?}` has not parent.")]
    NoParentPath(PathBuf),
    #[error("Could not create sub directory: {0}")]
    CreateProjectSubDirectory(std::io::Error),
    #[error("Could not reserve file: {0}")]
    ReserveFile(std::io::Error),
    #[error("Missing Content-Range header")]
    MissingContentRangeHeader,
    #[error("Invalid Content-Range header: {0}")]
    InvalidContentRangeHeader(macworp_core::http::headers::Error),
    #[error("Missing file size in Content-Range header")]
    MissingFileSize,
    #[error("Target path is a directory: {0:?}")]
    TargetPathIsDirectory(PathBuf),
    #[error("Content range exceeds file size for target path: {0:?}")]
    GetFileMetadataFailed(std::io::Error),
    #[error("Unable to finalize upload")]
    FinalizeUploadFailed(std::io::Error),
    #[error("Target path is a file: {0:?}")]
    TargetPathIsFile(PathBuf),
    #[error("Failed to delete file, {0}")]
    DeleteFileFailed(std::io::Error),
    #[error("Failed to delete directory, {0}")]
    DeleteDirectoryFailed(std::io::Error),
    #[error("Path `{0:?}` does not exist.")]
    PathDoesNotExist(PathBuf),
}
