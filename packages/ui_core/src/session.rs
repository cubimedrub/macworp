use dioxus::prelude::*;

use crate::{client::Client, configuration::Configuration, storage::IsStorage};

/// Credentials and client to talk to the backend from event handlers and resources that need no
/// typed storage error.
pub(crate) struct Session {
    /// Client for the configured backend
    pub client: Client,
    /// JWT of the logged in user
    pub jwt: String,
}

impl Session {
    /// The session of the logged in user, an error message if nobody is logged in.
    pub fn new<S: IsStorage + 'static>(
        storage: Signal<S>,
        config: Signal<Configuration>,
    ) -> Result<Self, String> {
        let jwt = storage
            .read()
            .jwt()
            .ok()
            .flatten()
            .ok_or_else(|| "Not logged in".to_string())?;
        let config = config.read().clone();
        Ok(Self {
            client: Client::new(config.base_url().to_string(), config.timeout()),
            jwt,
        })
    }
}
