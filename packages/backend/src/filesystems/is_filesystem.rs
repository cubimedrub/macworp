use std::path::{Path, PathBuf};

use crate::{
    filesystems::{errors::Error, project_content::ProjectContent},
    models::Project,
    state::State,
};

/// Defintion of file system acces, e.g. read files and folders of project or store data.
///
pub trait IsFilesystem {
    /// Returns the content of a project
    ///
    /// # Arguments
    /// * `state` - The application state, used to access configuration and other resources
    /// * `project` - The project to get the content of
    /// * `subpath` - An optional subpath within the project to get the content
    ///
    fn content(
        state: &State,
        project: &Project,
        subpath: Option<PathBuf>,
    ) -> Result<ProjectContent, Error>;

    /// Creats a project folder if supported by the FS
    ///
    /// # Arguments
    /// * `state` - The application state, used to access configuration and other resources
    /// * `project` - The project to create the folder for
    ///
    fn create_project_directory(
        state: &State,
        project: &Project,
    ) -> impl Future<Output = Result<PathBuf, Error>> + Send;

    /// Securuly joins two pathes. Each non normal compontent of the subpath, e.g. Root or Parent is filtered out to prevent
    /// brekaing out of the base path
    ///
    /// # Arguments
    /// * `base` - The base path to join to
    /// * `subpath` - The subpath to join to the base path, only
    ///
    fn join_paths(base: &Path, subpath: &Path) -> PathBuf {
        let mut final_path = base.to_path_buf();
        subpath
            .components()
            .filter(|comp| matches!(comp, std::path::Component::Normal(_)))
            .for_each(|comp| final_path = final_path.join(comp));
        final_path
    }

    /// Deletes the project directory if supported by the FS
    ///
    /// # Arguments
    /// * `state` - The application state, used to access configuration and other resources
    /// * `project` - The project to delete the folder for
    ///
    fn delete_project_directory(
        state: &State,
        project: &Project,
    ) -> impl Future<Output = Result<(), Error>> + Send;
}
