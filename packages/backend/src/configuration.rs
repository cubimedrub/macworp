use std::hash::{Hash, Hasher};

use macworp_core::{responses::LoginProviderResponse, workflow_configuration::engine::Engine};
use rand::{RngExt, distr::Alphanumeric};
use serde::{Deserialize, Serialize};

/// Login validity in seconds if not configured (24 hours)
pub const DEFAULT_LOGIN_VALIDITY: u32 = 24 * 60 * 60;

/// Runs kept per project if not configured
pub const DEFAULT_MAX_RUNS_PER_PROJECT: u64 = 10;

/// Minimum length of the JWT secret in bytes
const MIN_SECRET_LENGTH: usize = 32;

/// Generates a random alphanumeric secret for signing JWTs
pub fn generate_secret() -> String {
    rand::rng()
        .sample_iter(Alphanumeric)
        .take(64)
        .map(char::from)
        .collect()
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Configuration {
    interface: String,
    port: u16,
    database_url: String,
    secret: String,
    /// Duration after the login expires in seconds, defaults to `DEFAULT_LOGIN_VALIDITY`
    login_validity: Option<u32>,
    /// Project path used on standalone
    projects_path: String,
    /// Worker credentials
    worker: WorkerCredentials,
    /// Login providers
    // TODO: Store as hashset, needs custom module for serialization
    login_providers: Vec<LoginProvider>,
    /// Origins allowed to call the API cross-origin, e.g. a separately deployed web frontend.
    /// Empty (default) disables CORS, `"*"` allows any origin. Ignored by the desktop integration.
    #[serde(default)]
    cors_allowed_origins: Vec<String>,
    /// Runs kept per project, the oldest finished and unlocked run is replaced by a new one.
    /// `0` disables the limit.
    #[serde(default = "default_max_runs_per_project")]
    max_runs_per_project: u64,
    /// Engines the container runtime cannot run, runs of them are rejected. Detected by the
    /// embedding application (desktop), never read from the config file.
    #[serde(skip)]
    unavailable_engines: Vec<Engine>,
}

fn default_max_runs_per_project() -> u64 {
    DEFAULT_MAX_RUNS_PER_PROJECT
}

impl Configuration {
    /// Checks the configuration for values that must not be used, e.g. weak JWT secrets
    pub fn validate(&self) -> Result<(), crate::error::Error> {
        if self.secret == "development" || self.secret.len() < MIN_SECRET_LENGTH {
            return Err(crate::error::Error::Configuration(
                "secret must be at least 32 bytes long and not the development default, use the `config` command to generate one",
            ));
        }
        for origin in &self.cors_allowed_origins {
            if origin != "*" && origin.parse::<http::HeaderValue>().is_err() {
                return Err(crate::error::Error::Configuration(
                    "cors_allowed_origins must contain `*` or valid origins, e.g. https://example.org",
                ));
            }
        }
        Ok(())
    }

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

    pub fn login_providers(&self) -> &Vec<LoginProvider> {
        &self.login_providers
    }

    pub fn login_providers_mut(&mut self) -> &mut Vec<LoginProvider> {
        &mut self.login_providers
    }

    pub fn cors_allowed_origins(&self) -> &Vec<String> {
        &self.cors_allowed_origins
    }

    pub fn cors_allowed_origins_mut(&mut self) -> &mut Vec<String> {
        &mut self.cors_allowed_origins
    }

    pub fn max_runs_per_project(&self) -> u64 {
        self.max_runs_per_project
    }

    pub fn max_runs_per_project_mut(&mut self) -> &mut u64 {
        &mut self.max_runs_per_project
    }

    pub fn unavailable_engines(&self) -> &[Engine] {
        &self.unavailable_engines
    }

    pub fn unavailable_engines_mut(&mut self) -> &mut Vec<Engine> {
        &mut self.unavailable_engines
    }
}

impl Default for Configuration {
    fn default() -> Self {
        Self {
            interface: "127.0.0.1".to_string(),
            port: 8080,
            database_url: "postgresql://postgres:postgres@127.0.0.1:5432/postgres".to_string(), // for the provided docker compose
            secret: generate_secret(),
            login_validity: None,
            projects_path: "dev_projects".to_string(),
            worker: WorkerCredentials::default(),
            login_providers: vec![LoginProvider::Local(LocalLoginProvider::default())],
            cors_allowed_origins: Vec::new(),
            max_runs_per_project: DEFAULT_MAX_RUNS_PER_PROJECT,
            unavailable_engines: Vec::new(),
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

#[derive(Debug, Serialize, Deserialize)]
pub enum LoginProvider {
    Local(LocalLoginProvider),
}

/// Login provider using the internal database
///
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct LocalLoginProvider {
    pub registration: bool,
}

impl From<&LoginProvider> for LoginProviderResponse {
    fn from(value: &LoginProvider) -> Self {
        match value {
            LoginProvider::Local(provider) => LoginProviderResponse::Local(provider.registration),
        }
    }
}

impl Hash for LoginProvider {
    fn hash<H: Hasher>(&self, state: &mut H) {
        match self {
            LoginProvider::Local(_) => {
                "local".hash(state);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_rejects_invalid_cors_origin() {
        let mut config = Configuration::default();
        assert!(config.validate().is_ok());
        config
            .cors_allowed_origins_mut()
            .push("bad\norigin".to_string());
        assert!(config.validate().is_err());
    }

    #[test]
    fn validate_accepts_cors_origins() {
        let mut config = Configuration::default();
        config.cors_allowed_origins_mut().push("*".to_string());
        config
            .cors_allowed_origins_mut()
            .push("http://localhost:8080".to_string());
        assert!(config.validate().is_ok());
    }
}
