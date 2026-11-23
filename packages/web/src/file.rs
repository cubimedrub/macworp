use std::{
    fmt::Display,
    ops::Deref,
    path::{Path, PathBuf},
    rc::Rc,
};

use macworp_ui_core::client::IsUploadableFile;
use web_sys::FileSystemFileEntry;

#[derive(Debug)]
pub struct UploadableFile {
    inner: FileSystemFileEntry,
    target_directory: Rc<PathBuf>,
}

impl UploadableFile {
    pub fn new(inner: FileSystemFileEntry, target_directory: Rc<PathBuf>) -> Self {
        Self {
            inner,
            target_directory,
        }
    }

    /// Path the file is uploaded to, relative to the project root.
    ///
    /// The entry's `fullPath` is anchored at the drop root (it starts with `/`), and
    /// `PathBuf::join` replaces the base when given an absolute path. The leading `/` is
    /// therefore stripped so the file ends up inside the selected target directory.
    pub fn upload_path(&self) -> PathBuf {
        self.target_directory
            .join(self.inner.full_path().trim_start_matches('/'))
    }
}

impl IsUploadableFile for UploadableFile {
    type Inner = FileSystemFileEntry;

    fn into_inner(self) -> Self::Inner {
        self.inner
    }

    fn parent(&self) -> Option<&Path> {
        Some(self.target_directory.as_path())
    }
}

impl Display for UploadableFile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.inner.full_path())
    }
}

impl Deref for UploadableFile {
    type Target = FileSystemFileEntry;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}
