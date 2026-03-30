use dioxus::prelude::*;
use macworp_ui_core::components::file_browser::{
    CreateFolderProps, DeletePathProps, IsFileBrowser,
};
use thiserror::Error;

use crate::storage::Storage;

#[derive(Debug, Error)]
pub enum Error {}

/// Immutable file browser for desktop use where the user is supposed
/// to use the native file explorer
pub struct FileBrowser;

impl IsFileBrowser for FileBrowser {
    type Storage = Storage;
    type Error = Error;

    const CreateFolder: Component<CreateFolderProps> = |_props| {
        rsx! {}
    };

    const PathDelete: Component<DeletePathProps> = |_props| {
        rsx! {}
    };
}
