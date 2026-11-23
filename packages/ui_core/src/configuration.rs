use serde::{Deserialize, Serialize};

#[derive(PartialEq, Clone, Serialize, Deserialize)]
pub struct Configuration {
    base_url: String,
    /// Timeout in seconds
    timeout: u64,
    /// Buffer size for uploading files.
    /// Server needs to be adjust accodingly to accept the size of post requests this will produce.
    upload_buffer_size: usize,
}

impl Configuration {
    pub fn new(base_url: String, timeout: u64, upload_buffer_size: usize) -> Self {
        Self {
            base_url,
            timeout,
            upload_buffer_size,
        }
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    pub fn timeout(&self) -> u64 {
        10
    }

    pub fn upload_buffer_size(&self) -> usize {
        self.upload_buffer_size
    }
}
