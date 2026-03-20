use serde::{Deserialize, Serialize};

#[derive(PartialEq, Clone, Serialize, Deserialize)]
pub struct Configuration {
    base_url: String,
    // Timeout in seconds
    timeout: u64,
}

impl Configuration {
    pub fn new(base_url: String, timeout: u64) -> Self {
        Self { base_url, timeout }
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    pub fn timeout(&self) -> u64 {
        10
    }
}
