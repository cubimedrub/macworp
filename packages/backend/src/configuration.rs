use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct Configuration {
    interface: String,
    port: u16,
    database_url: String,
}

impl Configuration {
    pub fn interface(&self) -> &str {
        &self.interface
    }

    pub fn interface_mut(&mut self) -> &mut String {
        &mut self.interface
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn port_mut(&mut self) -> &mut u16 {
        &mut self.port
    }

    pub fn database_url(&self) -> &str {
        &self.database_url
    }

    pub fn database_url_mut(&mut self) -> &mut String {
        &mut self.database_url
    }
}

impl Default for Configuration {
    fn default() -> Self {
        Self {
            interface: "127.0.0.1".to_string(),
            port: 8080,
            database_url: "postgresql://postgres:postgres@127.0.0.1:5432/postgres".to_string(), // for the provided docker compose
        }
    }
}
