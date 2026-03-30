use std::{
    fmt::{Debug, Display},
    future::Future,
    ops::Deref,
    path::{Path, PathBuf},
};

use dioxus::signals::Signal;
use macworp_core::{
    authentication::TokenType,
    requests::{
        NewProjectRequest, PasswordLoginRequest, ProjectCreateFolderRequest,
        ProjectPathDeletionRequest,
    },
    responses::{LoginProviderResponse, ProjectResponse, UserResponse},
    routing::*,
};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("Failed to send request: {0}")]
    Request(#[from] reqwest::Error),
    #[error("Something went wrong querying backend: {0}")]
    Backend(String),
    #[error("Project file upload failed: {0}")]
    ProjectFileUpload(String),
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
            UtilitiesRouting::VERSION_PATH
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
            UtilitiesRouting::HEALTHCHECK_PATH
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
            UsersRouting::CURRENT_PATH
        );
        let client = reqwest::Client::new();
        let response = client
            .post(&url)
            .header(http::header::AUTHORIZATION, jwt)
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
                http::header::AUTHORIZATION,
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
                http::header::AUTHORIZATION,
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
        subpath: PathBuf,
    ) -> Result<ProjectResponse, Error> {
        let url = format!(
            "{}{}{}",
            self.base_client.base_url(),
            ProjectsRouting::controller_path(),
            ProjectsRouting::EDIT_PATH.replace("{id}", &id.to_string())
        );

        let client = reqwest::Client::new();

        let response = client
            .get(&url)
            .query(&[("subpath", subpath)])
            .header(
                http::header::AUTHORIZATION,
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

    pub async fn delete(&self, jwt: &str, id: i64) -> Result<bool, Error> {
        let url = format!(
            "{}{}{}",
            self.base_client.base_url(),
            ProjectsRouting::controller_path(),
            ProjectsRouting::DELETE_PATH.replace("{id}", &id.to_string())
        );

        let client = reqwest::Client::new();

        let response = client
            .post(&url)
            .header(
                http::header::AUTHORIZATION,
                format!("{} {jwt}", TokenType::Jwt),
            )
            .timeout(std::time::Duration::from_secs(self.base_client.timeout()))
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            tracing::error!(
                "Failed to delete project: HTTP {}, response text: {}",
                status,
                text
            );
            return Err(Error::Backend(format!(
                "Failed to delete project: HTTP {}, response text: {}",
                status, text
            )));
        }

        Ok(response.status().is_success())
    }

    /// Uploads a file to the project
    ///
    /// # Arguments
    /// * `jwt` - The JWT for authentication
    /// * `id` - The project id
    /// * `folder_path` - The path of the folder to upload the file to relative to the project
    ///
    pub async fn upload_file<U: IsFileUpload>(
        &self,
        jwt: &str,
        id: i64,
        file: U::File,
        buffer_size: usize,
        progress: Signal<f64>,
    ) -> Result<(), Error> {
        let url = format!(
            "{}{}{}",
            self.base_client.base_url(),
            ProjectsRouting::controller_path(),
            ProjectsRouting::UPLOAD_FILE_PATH.replace("{id}", &id.to_string())
        );

        U::upload_file(&url, jwt, file, buffer_size, progress)
            .await
            .map_err(|err| Error::ProjectFileUpload(err.to_string()))
    }

    /// Creates empty folder within a project
    ///
    /// # Arguments
    /// * `jwt` - The JWT for authentication
    /// * `id` - The project id
    /// * `folder_path` - The path of the folder to create relative to the project
    ///
    pub async fn create_folder(
        &self,
        jwt: &str,
        id: i64,
        folder_path: PathBuf,
    ) -> Result<(), Error> {
        let url = format!(
            "{}{}{}",
            self.base_client.base_url(),
            ProjectsRouting::controller_path(),
            ProjectsRouting::CREATE_FOLDER_PATH.replace("{id}", &id.to_string())
        );

        let client = reqwest::Client::new();

        let body = ProjectCreateFolderRequest { folder_path };
        let response = client
            .post(&url)
            .header(
                http::header::AUTHORIZATION,
                format!("{} {jwt}", TokenType::Jwt),
            )
            .timeout(std::time::Duration::from_secs(self.base_client.timeout()))
            .json(&body)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();

            tracing::error!(
                "Failed to create folder: HTTP {}, response text: {}",
                status,
                text
            );
            return Err(Error::Backend(format!(
                "Failed to create folder: HTTP {}, response text: {}",
                status, text
            )));
        }

        Ok(())
    }

    /// Deletes the given path
    ///
    /// # Arguments
    /// * `jwt` - The JWT for authentication
    /// * `id` - The project id
    /// * `path` - Path to delete
    ///
    pub async fn delete_path(&self, jwt: &str, id: i64, path: PathBuf) -> Result<(), Error> {
        let url = format!(
            "{}{}{}",
            self.base_client.base_url(),
            ProjectsRouting::controller_path(),
            ProjectsRouting::DELETE_PATH_PATH.replace("{id}", &id.to_string())
        );

        let client = reqwest::Client::new();

        let body = ProjectPathDeletionRequest { path };
        let response = client
            .post(&url)
            .header(
                http::header::AUTHORIZATION,
                format!("{} {jwt}", TokenType::Jwt),
            )
            .timeout(std::time::Duration::from_secs(self.base_client.timeout()))
            .json(&body)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();

            tracing::error!(
                "Failed to delete path: HTTP {}, response text: {}",
                status,
                text
            );
            return Err(Error::Backend(format!(
                "Failed to delete path: HTTP {}, response text: {}",
                status, text
            )));
        }

        Ok(())
    }
}

/// Trait defining an uploadable file.
pub trait IsUploadableFile: Debug + Display + Deref<Target = Self::Inner> {
    type Inner;
    fn into_inner(self) -> Self::Inner;
    fn parent(&self) -> Option<&Path>;
}

pub trait IsFileUpload {
    type Error: std::error::Error + std::fmt::Debug + Send + Sync;
    /// Uploadable file may be different depending on the platform.
    /// On WASM for example it is a FileSystemFileEntry which is later converted to JS File
    type File: IsUploadableFile;

    /// Uploads the given file in chunks of max buffer_size
    ///
    /// # Arguments
    /// * `url` - The URL to upload the file to
    /// * `jwt` - The JWT for authentication
    /// * `file` - The file to upload
    /// * `buffer_size` - Size of the buffer to fill before sending a upload request.
    /// * `progress` - Signal to report upload progress of the file to the UI
    ///
    fn upload_file(
        url: &str,
        jwt: &str,
        file: Self::File,
        buffer_size: usize,
        progress: Signal<f64>,
    ) -> impl Future<Output = Result<(), Self::Error>>;
}
