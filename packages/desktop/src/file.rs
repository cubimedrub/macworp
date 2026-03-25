use std::{fmt::Display, ops::Deref};

use macworp_ui_core::client::IsUploadableFile;

#[derive(Debug)]
pub struct UploadableFile;

impl Default for UploadableFile {
    fn default() -> Self {
        Self
    }
}

impl IsUploadableFile for UploadableFile {
    type Inner = ();

    fn into_inner(self) -> Self::Inner {}
}

impl Display for UploadableFile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "not supported on desktop")
    }
}

impl Deref for UploadableFile {
    type Target = ();

    fn deref(&self) -> &Self::Target {
        &()
    }
}
