use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::filesystems::errors::Error;

/// List of files and folder of a project
///
#[derive(Deserialize, Serialize)]
pub struct ProjectContent {
    pub current: PathBuf,
    pub files: Vec<String>,
    pub directories: Vec<String>,
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

            let name = match path.file_name() {
                Some(name) => name,
                None => continue, // Skip entries without a valid file names
            };

            if path.is_file() {
                files.push(name.to_string_lossy().to_string());
            } else if path.is_dir() {
                directories.push(name.to_string_lossy().to_string());
            }
        }

        Ok(ProjectContent {
            files,
            directories,
            current: requested_path
                .strip_prefix(project_path)
                .unwrap()
                .to_path_buf(),
        })
    }
}
