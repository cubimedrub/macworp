use std::path::PathBuf;

use macworp_core::{
    authentication::{TokenType, AUTHORIZATION_HEADER_NAME as PLAIN_AUTHORIZATION_HEADER_NAME},
    requests::{NewProjectRequest, PasswordLoginRequest},
    responses::{LoginProviderResponse, ProjectResponse, UserResponse},
    routing::*,
};
use reqwest::header::HeaderName;
use thiserror::Error;

const AUTHENTICATION_HEADER_NAME: HeaderName =
    HeaderName::from_static(PLAIN_AUTHORIZATION_HEADER_NAME);

#[derive(Debug, Error)]
pub enum Error {
    #[error("Failed to send request: {0}")]
    Request(#[from] reqwest::Error),
    #[error("Something went wrong querying backend: {0}")]
    Backend(String),
}

/// Client to communicate with MAcWorP's backend.
///
pub struct Client {
    base_url: String,
    timeout: u64,
}

impl Client {
    /// Creates a new client
    ///
    /// # Arguments
    /// * `base_url` - The base URL of the backend, e.g. "http://localhost:8080"
    /// * `timeout` - The timeout for requests in seconds
    pub fn new(base_url: String, timeout: u64) -> Self {
        Self { base_url, timeout }
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    pub fn timeout(&self) -> u64 {
        self.timeout
    }

    /// Returns a sub client to access the utilities
    ///
    pub fn utilities<'a>(&'a self) -> UtilitiesClient<'a> {
        UtilitiesClient::new(self)
    }

    pub fn login<'a>(&'a self) -> LoginClient<'a> {
        LoginClient::new(self)
    }

    pub fn projects<'a>(&'a self) -> ProjectsClient<'a> {
        ProjectsClient::new(self)
    }
}

/// Utilities subclient
///
pub struct UtilitiesClient<'a> {
    base_client: &'a Client,
}

impl<'a> UtilitiesClient<'a> {
    /// Creates a new client
    ///
    /// # Arguments
    /// * `base_client` - Base client
    pub fn new(base_client: &'a Client) -> Self {
        Self { base_client }
    }

    /// Returns the backend version
    ///
    pub async fn version(&self) -> Result<String, Error> {
        let url = format!(
            "{}{}{}",
            self.base_client.base_url(),
            UtilitiesRouting::controller_path(),
            UtilitiesRouting::version_path()
        );
        let client = reqwest::Client::new();
        let response = client
            .get(&url)
            .timeout(std::time::Duration::from_secs(self.base_client.timeout()))
            .send()
            .await?;
        response.text().await.map_err(Error::Request)
    }

    /// Returns true it the backends answers with 200
    ///
    pub async fn healthcheck(&self) -> Result<bool, Error> {
        let url = format!(
            "{}{}{}",
            self.base_client.base_url(),
            UtilitiesRouting::controller_path(),
            UtilitiesRouting::healthcheck_path()
        );
        let client = reqwest::Client::new();
        let response = client
            .get(&url)
            .timeout(std::time::Duration::from_secs(self.base_client.timeout()))
            .send()
            .await?;
        Ok(response.status().is_success())
    }
}

/// Login subclient
///
pub struct UsersClient<'a> {
    base_client: &'a Client,
}

impl<'a> UsersClient<'a> {
    /// Creates a new client
    ///
    /// # Arguments
    /// * `base_client` - Base client
    pub fn new(base_client: &'a Client) -> Self {
        Self { base_client }
    }

    /// Returns the backend version
    ///
    pub async fn current(&self, jwt: &str) -> Result<UserResponse, Error> {
        let url = format!(
            "{}{}{}",
            self.base_client.base_url(),
            UsersRouting::controller_path(),
            UsersRouting::current_path()
        );
        let client = reqwest::Client::new();
        let response = client
            .post(&url)
            .header(AUTHENTICATION_HEADER_NAME, jwt)
            .timeout(std::time::Duration::from_secs(self.base_client.timeout()))
            .send()
            .await?;
        response
            .json::<UserResponse>()
            .await
            .map_err(Error::Request)
    }
}

/// Login subclient
///
pub struct LoginClient<'a> {
    base_client: &'a Client,
}

impl<'a> LoginClient<'a> {
    /// Creates a new client
    ///
    /// # Arguments
    /// * `base_client` - Base client
    pub fn new(base_client: &'a Client) -> Self {
        Self { base_client }
    }

    /// Returns the backend version
    ///
    pub async fn local_login(&self, username: String, password: String) -> Result<String, Error> {
        let url = format!(
            "{}{}{}",
            self.base_client.base_url(),
            LoginRouting::controller_path(),
            LoginRouting::WITH_LOCAL
        );
        let client = reqwest::Client::new();
        let body = PasswordLoginRequest { username, password };
        let response = client
            .post(&url)
            .timeout(std::time::Duration::from_secs(self.base_client.timeout()))
            .json(&body)
            .send()
            .await?;
        response.text().await.map_err(Error::Request)
    }

    /// Returns the configured login providers
    ///
    pub async fn login_providers(&self) -> Result<Vec<LoginProviderResponse>, Error> {
        let url = format!(
            "{}{}{}",
            self.base_client.base_url(),
            LoginRouting::controller_path(),
            LoginRouting::LOGIN_PROVIDERS_PATH
        );
        let client = reqwest::Client::new();
        let response = client
            .get(&url)
            .timeout(std::time::Duration::from_secs(self.base_client.timeout()))
            .send()
            .await?;
        response.json().await.map_err(Error::Request)
    }

    pub fn open_id_connect_url(&self, provider_name: &str) -> String {
        format!(
            "{}{}{}",
            self.base_client.base_url(),
            LoginRouting::controller_path(),
            LoginRouting::WITH_OPEN_ID_CONNECT.replace("{name}", provider_name)
        )
    }
}

/// Projects subclient
///
pub struct ProjectsClient<'a> {
    base_client: &'a Client,
}

impl<'a> ProjectsClient<'a> {
    /// Creates a new client
    ///
    /// # Arguments
    /// * `base_client` - Base client
    pub fn new(base_client: &'a Client) -> Self {
        Self { base_client }
    }

    /// Returns the backend version
    ///
    pub async fn projects(&self, jwt: &str) -> Result<Vec<ProjectResponse>, Error> {
        let url = format!(
            "{}{}{}",
            self.base_client.base_url(),
            ProjectsRouting::controller_path(),
            ProjectsRouting::INDEX_PATH
        );
        let client = reqwest::Client::new();
        let response = client
            .get(&url)
            .header(
                AUTHENTICATION_HEADER_NAME,
                format!("{} {jwt}", TokenType::Jwt),
            )
            .timeout(std::time::Duration::from_secs(self.base_client.timeout()))
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            tracing::error!(
                "Failed to fetch projects: HTTP {}, response text: {}",
                status,
                text
            );
            return Err(Error::Backend(format!(
                "Failed to fetch projects: HTTP {}, response text: {}",
                status, text
            )));
        }

        response
            .json::<Vec<ProjectResponse>>()
            .await
            .map_err(Error::Request)
    }

    /// Creates a new project
    ///
    pub async fn new_project(
        &self,
        jwt: &str,
        name: String,
        path: PathBuf,
    ) -> Result<ProjectResponse, Error> {
        let url = format!(
            "{}{}{}",
            self.base_client.base_url(),
            ProjectsRouting::controller_path(),
            ProjectsRouting::NEW_PATH
        );
        let client = reqwest::Client::new();

        let body = NewProjectRequest { name, path };
        let response = client
            .post(&url)
            .header(
                AUTHENTICATION_HEADER_NAME,
                format!("{} {jwt}", TokenType::Jwt),
            )
            .json(&body)
            .timeout(std::time::Duration::from_secs(self.base_client.timeout()))
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            tracing::error!(
                "Failed to fetch projects: HTTP {}, response text: {}",
                status,
                text
            );
            return Err(Error::Backend(format!(
                "Failed to fetch projects: HTTP {}, response text: {}",
                status, text
            )));
        }

        response
            .json::<ProjectResponse>()
            .await
            .map_err(Error::Request)
    }

    /// Creates a new project
    ///
    pub async fn project(
        &self,
        jwt: &str,
        id: i64,
        subpath: Option<String>,
    ) -> Result<ProjectResponse, Error> {
        let mut url = format!(
            "{}{}{}",
            self.base_client.base_url(),
            ProjectsRouting::controller_path(),
            ProjectsRouting::EDIT_PATH.replace("{id}", &id.to_string())
        );

        if let Some(subpath) = subpath {
            url.push_str(&format!("?subpath={}", urlencoding::encode(&subpath)));
        }

        let client = reqwest::Client::new();

        let response = client
            .get(&url)
            .header(
                AUTHENTICATION_HEADER_NAME,
                format!("{} {jwt}", TokenType::Jwt),
            )
            .timeout(std::time::Duration::from_secs(self.base_client.timeout()))
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            tracing::error!(
                "Failed to fetch projects: HTTP {}, response text: {}",
                status,
                text
            );
            return Err(Error::Backend(format!(
                "Failed to fetch projects: HTTP {}, response text: {}",
                status, text
            )));
        }

        response
            .json::<ProjectResponse>()
            .await
            .map_err(Error::Request)
    }
}
