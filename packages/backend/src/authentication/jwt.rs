use chrono::{Duration, Utc};
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation, decode, encode};
use serde::{Deserialize, Serialize};

use crate::{
    authentication::error::Error,
    configuration::{Configuration, DEFAULT_LOGIN_VALIDITY},
};

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

    /// Get user ID as i64, fails if the subject is not an i64
    pub fn user_id(&self) -> Result<i64, Error> {
        self.sub.parse().map_err(|_| Error::InvalidTokenSubject)
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
    /// if given otherwise `DEFAULT_LOGIN_VALIDITY`
    ///
    /// # Arguments
    /// * `user_id` - The ID of the user for whom the token is being
    /// * `config` - The application configuration containing the secret and validity period
    ///
    pub fn encode(user_id: i64, config: &Configuration) -> Result<String, Error> {
        let now = Utc::now();
        let validity = config.login_validity().unwrap_or(DEFAULT_LOGIN_VALIDITY);
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
            &Validation::new(Algorithm::HS256),
        )
        .map_err(|err| match err.kind() {
            jsonwebtoken::errors::ErrorKind::ExpiredSignature => Error::ExpiredSession,
            _ => Error::JWTDecryption(err),
        })
        .map(|data| data.claims)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Creates a configuration with a freshly generated random secret.
    /// Every call returns a different secret, which tests use to simulate a foreign signer.
    fn config() -> Configuration {
        Configuration::default()
    }

    /// Signs a token with an arbitrary `sub` claim using the secret of the given configuration.
    /// The token is valid for 60 seconds. Allows creating validly signed tokens that
    /// `JsonWebToken::encode` would never produce, e.g. with a non-numeric subject.
    fn token_with_sub(sub: &str, config: &Configuration) -> String {
        let now = Utc::now().timestamp() as usize;
        let payload = Payload {
            sub: sub.to_string(),
            exp: now + 60,
            iat: now,
        };
        encode(
            &Header::default(),
            &payload,
            &EncodingKey::from_secret(config.secret().as_bytes()),
        )
        .unwrap()
    }

    /// A token encoded for a user decodes again with the same configuration and yields the
    /// same user ID. Without `login_validity` set, the token lifetime (`exp - iat`) is exactly
    /// `DEFAULT_LOGIN_VALIDITY`, guarding against the former `u32::MAX` default.
    #[test]
    fn roundtrip_uses_default_validity() {
        let config = config();
        let token = JsonWebToken::encode(42, &config).unwrap();
        let payload = JsonWebToken::decode(&token, &config).unwrap();

        assert_eq!(payload.user_id().unwrap(), 42);
        assert_eq!(
            payload.expires_at() - payload.issued_at(),
            DEFAULT_LOGIN_VALIDITY as usize
        );
    }

    /// A validly signed token whose `sub` is not an `i64` still decodes, but `user_id()` returns
    /// `Error::InvalidTokenSubject` (HTTP 401) instead of panicking.
    #[test]
    fn non_numeric_subject_is_error() {
        let config = config();
        let token = token_with_sub("abc", &config);
        let payload = JsonWebToken::decode(&token, &config).unwrap();

        assert!(matches!(payload.user_id(), Err(Error::InvalidTokenSubject)));
    }

    /// A token signed with one secret must not decode with a different secret
    /// (two default configurations have different random secrets) and fails with
    /// `Error::JWTDecryption`, so tokens cannot be forged without knowing the secret.
    #[test]
    fn wrong_secret_is_rejected() {
        let token = JsonWebToken::encode(1, &config()).unwrap();

        assert!(matches!(
            JsonWebToken::decode(&token, &config()),
            Err(Error::JWTDecryption(_))
        ));
    }

    /// `Configuration::validate` accepts the generated default secret and rejects the old
    /// `"development"` default as well as secrets shorter than 32 bytes.
    #[test]
    fn weak_secrets_fail_validation() {
        let mut config = config();
        assert!(config.validate().is_ok());

        *config.secret_mut() = "development".to_string();
        assert!(config.validate().is_err());

        *config.secret_mut() = "short".to_string();
        assert!(config.validate().is_err());
    }
}
