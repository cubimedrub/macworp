//! How the workflow editor provides the files of a local workflow.
//!
//! On a server the user uploads a zip archive (the default, nothing to provide). A platform
//! with direct access to the machine the workflow runs on, the desktop app, provides a
//! [`LocalFolderPicker`] context instead, so the user chooses the folder in the native dialog
//! and nothing is copied.

use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;

use dioxus::prelude::try_use_context;

/// Result of a folder dialog: the chosen folder, `None` if it was cancelled.
pub type PickFuture = Pin<Box<dyn Future<Output = Option<PathBuf>>>>;

/// Opens a native folder dialog. Provide it as context to replace the zip upload.
#[derive(Clone)]
pub struct LocalFolderPicker {
    /// Opens the dialog starting in the given folder
    pick: Arc<dyn Fn(PathBuf) -> PickFuture + Send + Sync>,
}

impl LocalFolderPicker {
    /// Wraps a dialog function.
    ///
    /// # Arguments
    /// * `pick` - Opens the dialog; called with the folder to start in
    pub fn new(pick: impl Fn(PathBuf) -> PickFuture + Send + Sync + 'static) -> Self {
        Self {
            pick: Arc::new(pick),
        }
    }

    /// Opens the dialog.
    ///
    /// # Arguments
    /// * `start` - Folder to start in
    pub fn pick(&self, start: PathBuf) -> PickFuture {
        (self.pick)(start)
    }

    /// The picker provided by the platform, `None` on platforms that upload. Call it in a
    /// component.
    pub fn from_context() -> Option<Self> {
        try_use_context::<Self>()
    }
}
