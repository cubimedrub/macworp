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
            let name = entry.file_name().to_string_lossy().to_string();

            // `DirEntry::file_type` does not follow symlinks, so links are skipped
            let file_type = entry.file_type()?;
            if file_type.is_file() {
                files.push(name);
            } else if file_type.is_dir() {
                directories.push(name);
            }
        }

        Ok(ProjectContent {
            files,
            directories,
            current: macworp_core::paths::to_portable(
                requested_path.strip_prefix(project_path).unwrap(),
            ),
        })
    }
}
