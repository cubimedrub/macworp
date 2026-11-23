use dioxus::signals::Signal;
use macworp_ui_core::client::IsFileUpload;
use thiserror::Error;

use crate::file::UploadableFile;

#[derive(Debug, Error)]
pub enum Error {}

pub struct DesktopFileUpload;

impl IsFileUpload for DesktopFileUpload {
    type Error = Error;
    type File = UploadableFile;

    async fn upload_file(
        _url: &str,
        _jwt: &str,
        _file: Self::File,
        _upload_buffer_size: usize,
        _progress: Signal<f64>,
    ) -> Result<(), Self::Error> {
        Ok(())
    }
}
