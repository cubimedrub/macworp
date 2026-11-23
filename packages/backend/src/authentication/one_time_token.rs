use std::{
    collections::HashMap,
    sync::Mutex,
    time::{Duration, Instant},
};

use rand::RngExt;

/// Short-lived tokens that each authenticate a single request as one user. They exist for requests
/// the browser makes itself (downloads) and so cannot carry the `Authorization` header: the client
/// obtains a token with an authenticated call and puts it into the URL.
///
/// Tokens live in memory only. They expire after seconds, so losing them on restart is harmless.
pub struct OneTimeTokens {
    /// How long a token stays valid after it was issued
    lifetime: Duration,
    /// Issued, not yet redeemed tokens: user id and issue time
    tokens: Mutex<HashMap<String, (i64, Instant)>>,
}

impl OneTimeTokens {
    /// Lifetime of a token, long enough for the browser to follow the link it was put into.
    pub const DEFAULT_LIFETIME: Duration = Duration::from_secs(10);

    /// Bytes of randomness in a token (hex encoded, so twice as many characters)
    const TOKEN_BYTES: usize = 32;

    /// Store whose tokens expire after `lifetime`.
    ///
    /// # Arguments
    /// * `lifetime` - How long a token stays valid after it was issued
    pub fn new(lifetime: Duration) -> Self {
        Self {
            lifetime,
            tokens: Mutex::default(),
        }
    }

    /// Issues a token for `user_id`. Expired tokens are dropped on the way.
    ///
    /// # Arguments
    /// * `user_id` - The user the token authenticates as
    pub fn issue(&self, user_id: i64) -> String {
        let mut bytes = [0u8; Self::TOKEN_BYTES];
        rand::rng().fill(&mut bytes);
        let token: String = bytes.iter().map(|byte| format!("{byte:02x}")).collect();

        let now = Instant::now();
        let mut tokens = self.tokens.lock().expect("one-time token lock poisoned");
        tokens.retain(|_, (_, issued)| now.duration_since(*issued) < self.lifetime);
        tokens.insert(token.clone(), (user_id, now));
        token
    }

    /// Consumes a token. Returns the user it was issued for, or `None` if it is unknown, was
    /// redeemed before or has expired. A second call with the same token never succeeds.
    ///
    /// # Arguments
    /// * `token` - The token from the request
    pub fn redeem(&self, token: &str) -> Option<i64> {
        let (user_id, issued) = self
            .tokens
            .lock()
            .expect("one-time token lock poisoned")
            .remove(token)?;
        (issued.elapsed() < self.lifetime).then_some(user_id)
    }
}

impl Default for OneTimeTokens {
    fn default() -> Self {
        Self::new(Self::DEFAULT_LIFETIME)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A token works once, for the user it was issued for.
    #[test]
    fn token_is_single_use() {
        let tokens = OneTimeTokens::default();
        let token = tokens.issue(7);
        assert_eq!(tokens.redeem(&token), Some(7));
        assert_eq!(tokens.redeem(&token), None);
    }

    /// Unknown tokens are rejected, and tokens differ per issue.
    #[test]
    fn unknown_token_is_rejected() {
        let tokens = OneTimeTokens::default();
        let first = tokens.issue(1);
        let second = tokens.issue(1);
        assert_ne!(first, second);
        assert_eq!(first.len(), OneTimeTokens::TOKEN_BYTES * 2);
        assert_eq!(tokens.redeem("nope"), None);
    }

    /// A token past its lifetime is rejected, and issuing drops it from the store.
    #[test]
    fn expired_token_is_rejected() {
        let tokens = OneTimeTokens::new(Duration::ZERO);
        let token = tokens.issue(1);
        assert_eq!(tokens.redeem(&token), None);
        tokens.issue(2);
        assert_eq!(tokens.tokens.lock().unwrap().len(), 1);
    }
}
