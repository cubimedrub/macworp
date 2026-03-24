use std::path::PathBuf;

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
        _state: &State,
        project: &Project,
        subpath: Option<PathBuf>,
    ) -> Result<ProjectContent, Error> {
        let project_path = PathBuf::from(
            project
                .path
                .clone()
                .ok_or(LocalFilesystemError::ProjectPathMissing)?,
        );

        let requested_path = if let Some(subpath) = subpath {
            Self::join_paths(&project_path, &subpath)
        } else {
            project_path.clone()
        };

        ProjectContent::try_from((&project_path, &requested_path))
    }

    async fn create_project_directory(_state: &State, project: &Project) -> Result<PathBuf, Error> {
        let path = project
            .path
            .clone()
            .ok_or(LocalFilesystemError::ProjectPathMissing)
            .map(PathBuf::from)?;

        Ok(path)
    }

    async fn delete_project_directory(_state: &State, _project: &Project) -> Result<(), Error> {
        // Not messing around with local filesystem
        Ok(())
    }
}
