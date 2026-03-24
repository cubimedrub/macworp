use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("Project path is not a directory but file")]
    ProjectPathIsFile,

    #[error("Unable to read content of project director: {0}")]
    ReadDir(#[from] std::io::Error),

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
}
