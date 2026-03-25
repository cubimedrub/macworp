use macworp_ui_core::client::IsFileUpload;
use thiserror::Error;

use crate::file::UploadableFile;

#[derive(Debug, Error)]
pub enum Error {}

pub struct DesktopFileUpload;

impl IsFileUpload for DesktopFileUpload {
    type Error = Error;
    type File = UploadableFile;

    async fn upload_file(_url: &str, _jwt: &str, _file: Self::File) -> Result<(), Self::Error> {
        Ok(())
    }
}
