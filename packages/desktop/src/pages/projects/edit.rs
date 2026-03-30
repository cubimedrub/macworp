use std::{path::PathBuf, rc::Rc};

use dioxus::{core::Event, html::DragData};
use macworp_ui_core::pages::projects::edit::IsProjectEdit;
use thiserror::Error;

use crate::{
    client::DesktopFileUpload, components::file_browser::FileBrowser, file::UploadableFile,
    router::Router, storage::Storage,
};

#[derive(Debug, Error)]
pub enum Error {}

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
}
