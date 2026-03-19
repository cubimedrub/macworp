use macworp_core::{requests::PasswordLoginRequest, responses::UserResponse, routing::*};
use reqwest::header::HeaderName;

const AUTHENTICATION_HEADER_NAME: HeaderName = HeaderName::from_static("authentication");

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

    /// Returns a sub client to access the utilities
    ///
    pub fn utilities<'a>(&'a self) -> UtilitiesClient<'a> {
        UtilitiesClient::new(&self.base_url, self.timeout)
    }

    pub fn login<'a>(&'a self) -> LoginClient<'a> {
        LoginClient::new(&self.base_url, self.timeout)
    }
}

/// Utilities subclient
///
pub struct UtilitiesClient<'a> {
    base_url: &'a str,
    timeout: u64,
}

impl<'a> UtilitiesClient<'a> {
    /// Creates a new client
    ///
    /// # Arguments
    /// * `base_url` - The base URL of the backend, e.g. "http://localhost:8080"
    /// * `timeout` - The timeout for requests in seconds
    pub fn new(base_url: &'a str, timeout: u64) -> Self {
        Self { base_url, timeout }
    }

    /// Returns the backend version
    ///
    pub async fn version(&self) -> Result<String, reqwest::Error> {
        let url = format!(
            "{}{}{}",
            self.base_url,
            UtilitiesRouting::controller_path(),
            UtilitiesRouting::version_path()
        );
        let client = reqwest::Client::new();
        let response = client
            .get(&url)
            .timeout(std::time::Duration::from_secs(self.timeout))
            .send()
            .await?;
        response.text().await
    }

    /// Returns true it the backends answers with 200
    ///
    pub async fn healthcheck(&self) -> Result<bool, reqwest::Error> {
        let url = format!(
            "{}{}{}",
            self.base_url,
            UtilitiesRouting::controller_path(),
            UtilitiesRouting::healthcheck_path()
        );
        let client = reqwest::Client::new();
        let response = client
            .get(&url)
            .timeout(std::time::Duration::from_secs(self.timeout))
            .send()
            .await?;
        Ok(response.status().is_success())
    }
}

/// Login subclient
///
pub struct UsersClient<'a> {
    base_url: &'a str,
    timeout: u64,
}

impl<'a> UsersClient<'a> {
    /// Creates a new client
    ///
    /// # Arguments
    /// * `base_url` - The base URL of the backend, e.g. "http://localhost:8080"
    /// * `timeout` - The timeout for requests in seconds
    pub fn new(base_url: &'a str, timeout: u64) -> Self {
        Self { base_url, timeout }
    }

    /// Returns the backend version
    ///
    pub async fn current(&self, jwt: &str) -> Result<UserResponse, reqwest::Error> {
        let url = format!(
            "{}{}{}",
            self.base_url,
            UsersRouting::controller_path(),
            UsersRouting::current_path()
        );
        let client = reqwest::Client::new();
        let response = client
            .post(&url)
            .header(AUTHENTICATION_HEADER_NAME, jwt)
            .timeout(std::time::Duration::from_secs(self.timeout))
            .send()
            .await?;
        response.json::<UserResponse>().await
    }
}

/// Login subclient
///
pub struct LoginClient<'a> {
    base_url: &'a str,
    timeout: u64,
}

impl<'a> LoginClient<'a> {
    /// Creates a new client
    ///
    /// # Arguments
    /// * `base_url` - The base URL of the backend, e.g. "http://localhost:8080"
    /// * `timeout` - The timeout for requests in seconds
    pub fn new(base_url: &'a str, timeout: u64) -> Self {
        Self { base_url, timeout }
    }

    /// Returns the backend version
    ///
    pub async fn loign_with_password(
        &self,
        username: String,
        password: String,
    ) -> Result<UserResponse, reqwest::Error> {
        let url = format!(
            "{}{}{}",
            self.base_url,
            LoginRouting::controller_path(),
            LoginRouting::with_password_path()
        );
        let client = reqwest::Client::new();
        let body = PasswordLoginRequest { username, password };
        let response = client
            .post(&url)
            .timeout(std::time::Duration::from_secs(self.timeout))
            .json(&body)
            .send()
            .await?;
        response.json::<UserResponse>().await
    }
}
