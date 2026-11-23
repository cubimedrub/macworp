/// Authentication errors
pub mod error;
/// JSON web token creation, encryption and decryption
pub mod jwt;
/// Authentication middleware for axum
pub mod middleware;
/// Short-lived single-use tokens for requests that cannot carry the `Authorization` header
pub mod one_time_token;
/// Helper functions for password authentication
pub mod password;
