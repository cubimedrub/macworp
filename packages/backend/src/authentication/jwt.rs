use chrono::{Duration, Utc};
use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation, decode, encode};
use serde::{Deserialize, Serialize};

use crate::{authentication::error::Error, configuration::Configuration};

/// Payload/Claims for the JWT
///
#[derive(Debug, Serialize, Deserialize)]
pub struct Payload {
    /// User ID as string
    sub: String, // Required to be string by jsonwebtoken crate
    /// UTC expiration timestamp
    exp: usize,
    /// UTC issued at timestamp
    iat: usize,
}

impl Payload {
    pub fn new(user_id: i64, expires_at: usize, issued_at: usize) -> Self {
        Self {
            sub: user_id.to_string(),
            exp: expires_at,
            iat: issued_at,
        }
    }

    /// Get user ID as i64, will panic if user ID is not i64
    pub fn user_id(&self) -> i64 {
        self.sub.parse().unwrap()
    }

    pub fn expires_at(&self) -> usize {
        self.exp
    }

    pub fn issued_at(&self) -> usize {
        self.iat
    }
}

/// Handler for JWT encoding and decoding
pub struct JsonWebToken;

impl JsonWebToken {
    /// Creates a JWT for the given user ID and configuration. The token will expire after the validity period specified in the configuration
    /// if given otherwise uses u32::MAX which would equals 49,710.2696296296 days
    ///
    /// # Arguments
    /// * `user_id` - The ID of the user for whom the token is being
    /// * `config` - The application configuration containing the secret and validity period
    ///
    pub fn encode(user_id: i64, config: &Configuration) -> Result<String, Error> {
        let now = Utc::now();
        let validity = config.login_validity().unwrap_or(u32::MAX);
        let expires_at = (now + Duration::seconds(validity as i64)).timestamp() as usize;
        let issued_at = now.timestamp() as usize;

        let payload = Payload::new(user_id, expires_at, issued_at);

        encode(
            &Header::default(),
            &payload,
            &EncodingKey::from_secret(config.secret().as_bytes()),
        )
        .map_err(Error::JWTEncryption)
    }

    /// Decodes a JWT and returns the payload if the token is valid. Will return an error if the token is expired or invalid.
    ///
    /// # Arguments
    /// * `token` - The JWT string to decode
    /// * `config` - The application configuration containing the secret for decoding
    ///
    pub fn decode(token: &str, config: &Configuration) -> Result<Payload, Error> {
        decode::<Payload>(
            token,
            &DecodingKey::from_secret(config.secret().as_bytes()),
            &Validation::default(),
        )
        .map_err(|err| match err.kind() {
            jsonwebtoken::errors::ErrorKind::ExpiredSignature => Error::ExpiredSession,
            _ => Error::JWTDecryption(err),
        })
        .map(|data| data.claims)
    }
}
