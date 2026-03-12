use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct Configuration {
    database_url: String,
}

impl Configuration {
    pub fn new(database_url: String) -> Self {
        Self { database_url }
    }

    pub fn database_url(&self) -> &str {
        &self.database_url
    }
}
