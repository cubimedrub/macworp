use std::path::PathBuf;

use crate::filesystems::errors::Error;

/// List of files and folder of a project
///
pub struct ProjectContent {
    pub files: Vec<PathBuf>,
    pub directories: Vec<PathBuf>,
}

impl TryFrom<(&PathBuf, &PathBuf)> for ProjectContent {
    type Error = Error;

    fn try_from((project_path, requested_path): (&PathBuf, &PathBuf)) -> Result<Self, Self::Error> {
        if !requested_path.is_dir() {
            return Err(Error::ProjectPathIsFile);
        }

        let mut files = Vec::new();
        let mut directories = Vec::new();

        for entry in requested_path.read_dir()? {
            let entry = entry?;
            let path = entry.path();

            if path.is_file() {
                files.push(path.strip_prefix(project_path).unwrap().to_path_buf());
            } else if path.is_dir() {
                directories.push(path.strip_prefix(project_path).unwrap().to_path_buf());
            }
        }

        Ok(ProjectContent { files, directories })
    }
}
