use std::{path::PathBuf, rc::Rc};

use dioxus::{core::Event, html::DragData};
use macworp_ui_core::pages::projects::edit::IsProjectEdit;
use notify::{RecursiveMode, Watcher};
use thiserror::Error;

use crate::{
    client::DesktopFileUpload, components::file_browser::FileBrowser, file::UploadableFile,
    router::Router, storage::Storage,
};

#[derive(Debug, Error)]
pub enum Error {
    #[error("{0}")]
    OpenFileExplorer(#[from] opener::OpenError),
    #[error("{0}")]
    Watch(#[from] notify::Error),
    #[error("File explorer task failed: {0}")]
    Join(#[from] tokio::task::JoinError),
}

pub struct FileImmutableProjectEdit;

impl IsProjectEdit for FileImmutableProjectEdit {
    type Error = Error;
    type File = UploadableFile;
    type FileUpload = DesktopFileUpload;
    type FileBrowser = FileBrowser;
    type Storage = Storage;
    type Router = Router;

    async fn prevent_window_file_drop_handling() -> Result<(), Self::Error> {
        Ok(())
    }

    async fn drop_handler(
        _event: Event<DragData>,
        _current_path: Rc<PathBuf>,
    ) -> Result<Vec<Result<Self::File, Self::Error>>, Self::Error> {
        Ok(Vec::new())
    }

    async fn dragover_handler(_event: Event<DragData>) -> Result<(), Self::Error> {
        Ok(())
    }

    async fn wait_for_change(path: PathBuf) -> Result<(), Self::Error> {
        let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
        let mut watcher =
            notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
                // Reads do not change the listing
                if let Ok(event) = event {
                    if !event.kind.is_access() {
                        sender.send(()).ok();
                    }
                }
            })?;
        watcher.watch(&path, RecursiveMode::NonRecursive)?;

        // The watcher lives in this future, dropping it stops watching
        receiver.recv().await;
        // Debounce: wait until the directory is quiet, so a bulk copy reloads once
        while tokio::time::timeout(std::time::Duration::from_millis(300), receiver.recv())
            .await
            .is_ok()
        {}
        Ok(())
    }

    async fn open_in_file_explorer(path: PathBuf) -> Result<(), Self::Error> {
        let path = Self::explorer_path(path);
        tokio::task::spawn_blocking(move || opener::open(path)).await??;
        Ok(())
    }
}

impl FileImmutableProjectEdit {
    /// The form of `path` the system's file explorer opens. Projects live on the host (also
    /// with a native Windows worker), so only the verbatim prefix `\\?\` that `canonicalize`
    /// puts before a Windows drive path is removed; Explorer does not open that form. UNC
    /// verbatim paths (`\\?\UNC\...`) and all other paths stay as they are.
    ///
    /// # Arguments
    /// * `path` - Local path of a project folder
    fn explorer_path(path: PathBuf) -> PathBuf {
        let text = path.to_string_lossy();
        match text.strip_prefix(r"\\?\") {
            Some(rest) if !rest.starts_with("UNC\\") => PathBuf::from(rest),
            _ => path,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::FileImmutableProjectEdit;

    /// Only the verbatim prefix of drive paths is removed.
    #[test]
    fn explorer_path_drops_the_verbatim_prefix_of_drive_paths() {
        let open = |text: &str| FileImmutableProjectEdit::explorer_path(PathBuf::from(text));
        assert_eq!(open(r"\\?\C:\Users\x"), PathBuf::from(r"C:\Users\x"));
        assert_eq!(open(r"C:\Users\x"), PathBuf::from(r"C:\Users\x"));
        assert_eq!(open("/home/x"), PathBuf::from("/home/x"));
        assert_eq!(
            open(r"\\?\UNC\server\share"),
            PathBuf::from(r"\\?\UNC\server\share")
        );
    }
}
