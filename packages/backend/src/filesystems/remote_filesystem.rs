use std::path::PathBuf;

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

impl IsFilesystem for RemoteFilesystem {
    fn content(
        state: &State,
        project: &Project,
        subpath: Option<PathBuf>,
    ) -> Result<ProjectContent, Error> {
        let project_path =
            PathBuf::from(state.config().projects_path()).join(project.id.to_string());

        let requested_path = if let Some(subpath) = subpath {
            Self::join_paths(&project_path, &subpath)
        } else {
            project_path.clone()
        };

        ProjectContent::try_from((&project_path, &requested_path))
    }

    async fn create_project_directory(state: &State, project: &Project) -> Result<PathBuf, Error> {
        let path = PathBuf::from(state.config().projects_path()).join(project.id.to_string());

        tokio::fs::create_dir_all(&path).await.map_err(|e| {
            Error::RemoteFilesystem(RemoteFilesystemError::CreateProjectDirectoryFailed(e))
        })?;

        Ok(path)
    }
}
