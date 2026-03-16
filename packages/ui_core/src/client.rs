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
        let url = format!("{}/utilities/version", self.base_url);
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
        let url = format!("{}/utilities/healthcheck", self.base_url);
        let client = reqwest::Client::new();
        let response = client
            .get(&url)
            .timeout(std::time::Duration::from_secs(self.timeout))
            .send()
            .await?;
        Ok(response.status().is_success())
    }
}
