use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct Configuration {
    interface: String,
    port: u16,
    database_url: String,
    secret: String,
    /// Duration after the login expires in seconds
    login_validity: Option<u32>,
    /// Project path used on standalone
    projects_path: String,
    /// Worker credentials
    worker: WorkerCredentials,
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

    pub fn secret(&self) -> &str {
        &self.secret
    }

    pub fn secret_mut(&mut self) -> &mut String {
        &mut self.secret
    }

    pub fn login_validity(&self) -> &Option<u32> {
        &self.login_validity
    }

    pub fn login_validity_mut(&mut self) -> &mut Option<u32> {
        &mut self.login_validity
    }

    pub fn projects_path(&self) -> &str {
        &self.projects_path
    }

    pub fn projects_path_mut(&mut self) -> &mut String {
        &mut self.projects_path
    }

    pub fn worker(&self) -> &WorkerCredentials {
        &self.worker
    }

    pub fn worker_mut(&mut self) -> &mut WorkerCredentials {
        &mut self.worker
    }
}

impl Default for Configuration {
    fn default() -> Self {
        Self {
            interface: "127.0.0.1".to_string(),
            port: 8080,
            database_url: "postgresql://postgres:postgres@127.0.0.1:5432/postgres".to_string(), // for the provided docker compose
            secret: "development".to_string(),
            login_validity: None,
            projects_path: "dev_projects".to_string(),
            worker: WorkerCredentials::default(),
        }
    }
}

/// Credentials for the worker
///
#[derive(Debug, Serialize, Deserialize)]
pub struct WorkerCredentials {
    /// username of the worker
    pub username: String,
    /// Hashed password of the worker
    pub password: String,
}

impl WorkerCredentials {
    /// Creates new worker credentials
    ///
    /// # Arguments
    /// * `username` - The username of the worker
    /// * `password` - The hashed password of the worker
    ///
    pub fn new(username: String, password: String) -> Self {
        Self { username, password }
    }

    pub fn username(&self) -> &str {
        &self.username
    }

    /// Sets the username of the worker
    pub fn username_mut(&mut self) -> &String {
        &mut self.username
    }

    /// Returns the hashed password
    pub fn password(&self) -> &str {
        &self.password
    }

    /// Sets the hashed password of the worker
    pub fn password_mut(&mut self) -> &mut String {
        &mut self.password
    }
}

impl Default for WorkerCredentials {
    fn default() -> Self {
        Self {
            username: "worker".to_string(),
            password: password_auth::generate_hash(b"developer"),
        }
    }
}
