use std::future::Future;
use std::path::{Path, PathBuf};

use axum::{BoxError, body::Bytes};
use futures_util::Stream;
use macworp_core::{http::headers::ContentRangeHeaderValue, responses::StorageProbe};

use crate::{
    filesystems::{
        directory_files::DirectoryFiles, errors::Error, project_content::ProjectContent,
    },
    models::Project,
    state::State,
};

/// Defintion of file system acces, e.g. read files and folders of project or store data.
///
pub trait IsFilesystem {
    /// Whether the files of the global reference data can be changed through the API. Desktop
    /// users manage their reference folder with the native file browser instead.
    const REFERENCE_WRITABLE: bool;

    /// Returns the content of a project
    ///
    /// # Arguments
    /// * `state` - The application state, used to access configuration and other resources
    /// * `project` - The project to get the content of
    /// * `subpath` - An optional subpath within the project to get the content
    ///
    fn content(state: &State, project: &Project, subpath: PathBuf)
    -> Result<ProjectContent, Error>;

    /// Creates the full project path depending on the platform
    ///
    /// # Arguments
    /// * `state` - The application state, used to access configuration and other resources
    /// * `project` - The project to get the path for
    fn project_path(state: &State, project: &Project) -> Result<PathBuf, Error>;

    /// Project path on the machine of the user, if the backend runs on it (desktop). Never set for
    /// remote filesystems so server paths are not exposed to clients.
    ///
    /// # Arguments
    /// * `state` - The application state, used to access configuration and other resources
    /// * `project` - The project to get the path for
    fn local_path(_state: &State, _project: &Project) -> Option<PathBuf> {
        None
    }

    /// The reference directory on the machine of the user, under the same rule as
    /// [`Self::local_path`]: only the local filesystem returns it.
    ///
    /// # Arguments
    /// * `_directory` - The current reference directory
    fn local_reference_path(_directory: &Path) -> Option<PathBuf> {
        None
    }

    /// Marker file proving that a worker sees the same project storage as the backend, `None`
    /// if workers do not need to share it (desktop: the worker runs in the backend process).
    /// The marker is created on first use and keeps its content afterwards.
    ///
    /// # Arguments
    /// * `state` - The application state, used to access configuration and other resources
    fn storage_probe(
        _state: &State,
    ) -> impl Future<Output = Result<Option<StorageProbe>, Error>> + Send {
        async { Ok(None) }
    }

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

    /// Securly joins two pathes. Each non normal compontent of the subpath, e.g. Root or Parent is filtered out to prevent
    /// brekaing out of the base path. Existing components below the base that are symlinks are rejected.
    /// The base itself may be a symlink.
    ///
    /// Note: check and later use are not atomic, so a symlink created in between is not caught.
    ///
    /// # Arguments
    /// * `base` - The base path to join to
    /// * `subpath` - The subpath to join to the base path, only
    ///
    fn join_paths(base: &Path, subpath: &Path) -> Result<PathBuf, Error> {
        DirectoryFiles::join_paths(base, subpath)
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

    /// Streams data to a file in the project directory. The file is created if it does not exist and overwritten if it does.
    ///
    /// # Arguments
    /// * `state` - The application state, used to access configuration and other resources
    /// * `project` - The project to store the file in
    /// * `file_path` - The path to the file to store, relative to the
    /// * `stream` - The stream of data to write to the file, each item is a chunk of bytes. The stream should end when the file is fully written.
    ///
    fn stream_to_file<S, E>(
        state: &State,
        project: &Project,
        file_path: PathBuf,
        content_range: ContentRangeHeaderValue,
        stream: S,
    ) -> impl Future<Output = Result<(), Error>> + Send
    where
        S: Stream<Item = Result<Bytes, E>> + Send,
        E: Into<BoxError>;

    /// Reads a small UTF-8 text file of the project (sample sheets). Available on every
    /// filesystem, also the local one.
    ///
    /// # Arguments
    /// * `state` - The application state, used to access configuration and other resources
    /// * `project` - The project to read from
    /// * `path` - File to read, relative to the project
    fn read_text(
        state: &State,
        project: &Project,
        path: PathBuf,
    ) -> impl Future<Output = Result<String, Error>> + Send {
        async move {
            let base = Self::project_path(state, project)?;
            DirectoryFiles::read_text(&base, &path, macworp_core::requests::TEXT_FILE_LIMIT).await
        }
    }

    /// Writes a small UTF-8 text file of the project (sample sheets), replacing it atomically.
    /// Available on every filesystem: on the desktop it is the only way the app writes into
    /// the user's own project folder.
    ///
    /// # Arguments
    /// * `state` - The application state, used to access configuration and other resources
    /// * `project` - The project to write to
    /// * `path` - File to write, relative to the project
    /// * `content` - New content
    fn write_text(
        state: &State,
        project: &Project,
        path: PathBuf,
        content: String,
    ) -> impl Future<Output = Result<(), Error>> + Send {
        async move {
            let base = Self::project_path(state, project)?;
            DirectoryFiles::write_text(
                &base,
                &path,
                &content,
                macworp_core::requests::TEXT_FILE_LIMIT,
            )
            .await
        }
    }

    /// Creates a folder
    ///
    /// # Arguments
    /// * `state` - The application state, used to access configuration and other resources
    /// * `project` - The project to create the folder in
    /// * `folder_path` - The path to the folder to create, relative to the
    ///
    fn create_folder(
        state: &State,
        project: &Project,
        folder_path: PathBuf,
    ) -> impl Future<Output = Result<(), Error>> + Send;

    /// Deletes a file or folder
    ///
    /// # Arguments
    /// * `state` - The application state, used to access configuration and other resources
    /// * `project` - The project to delete the file or folder in
    /// * `path` - The path to the file or folder to delete, relative to
    ///
    fn delete_path(
        state: &State,
        project: &Project,
        path: PathBuf,
    ) -> impl Future<Output = Result<(), Error>> + Send;
}

#[cfg(all(test, feature = "standalone"))]
mod tests {
    use std::path::{Path, PathBuf};

    use crate::filesystems::{
        errors::Error, project_content::ProjectContent, remote_filesystem::RemoteFilesystem,
    };

    use super::IsFilesystem;

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "macworp_{name}_{}_{:?}",
                std::process::id(),
                std::thread::current().id()
            ));
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn strips_parent_and_root_components() {
        let tmp = TempDir::new("strip");
        let joined = RemoteFilesystem::join_paths(&tmp.0, Path::new("/../a/../b")).unwrap();
        assert_eq!(joined, tmp.0.join("a").join("b"));
    }

    #[test]
    fn missing_path_joins() {
        let tmp = TempDir::new("missing");
        let joined = RemoteFilesystem::join_paths(&tmp.0, Path::new("new/dir/file.txt")).unwrap();
        assert_eq!(joined, tmp.0.join("new/dir/file.txt"));
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlinked_directory_component() {
        let tmp = TempDir::new("symdir");
        let outside = TempDir::new("symdir_outside");
        std::os::unix::fs::symlink(&outside.0, tmp.0.join("link")).unwrap();

        let res = RemoteFilesystem::join_paths(&tmp.0, Path::new("link/secret"));
        assert!(matches!(res, Err(Error::SymlinkNotAllowed(_))));
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlinked_final_component() {
        let tmp = TempDir::new("symfile");
        let outside = TempDir::new("symfile_outside");
        std::fs::write(outside.0.join("f"), "x").unwrap();
        std::os::unix::fs::symlink(outside.0.join("f"), tmp.0.join("link")).unwrap();

        let res = RemoteFilesystem::join_paths(&tmp.0, Path::new("link"));
        assert!(matches!(res, Err(Error::SymlinkNotAllowed(_))));
    }

    #[cfg(unix)]
    #[test]
    fn allows_symlinked_base() {
        let real = TempDir::new("symbase_real");
        std::fs::create_dir(real.0.join("sub")).unwrap();
        let holder = TempDir::new("symbase_holder");
        let base = holder.0.join("base");
        std::os::unix::fs::symlink(&real.0, &base).unwrap();

        let joined = RemoteFilesystem::join_paths(&base, Path::new("sub")).unwrap();
        assert_eq!(joined, base.join("sub"));
    }

    #[cfg(unix)]
    #[test]
    fn listing_skips_symlinks() {
        let tmp = TempDir::new("list");
        let outside = TempDir::new("list_outside");
        std::fs::write(tmp.0.join("real.txt"), "x").unwrap();
        std::fs::create_dir(tmp.0.join("realdir")).unwrap();
        std::os::unix::fs::symlink(&outside.0, tmp.0.join("linkdir")).unwrap();
        std::os::unix::fs::symlink(outside.0.clone(), tmp.0.join("linkfile")).unwrap();

        let content = ProjectContent::try_from((&tmp.0, &tmp.0)).unwrap();
        assert_eq!(content.files, vec!["real.txt"]);
        assert_eq!(content.directories, vec!["realdir"]);
    }
}
