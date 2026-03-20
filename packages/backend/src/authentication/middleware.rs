use std::sync::Arc;

use axum::{
    Extension, RequestPartsExt,
    extract::{FromRef, FromRequestParts},
    http::{HeaderValue, request::Parts},
};
use base64::prelude::*;
use macworp_core::authentication::{AUTHORIZATION_HEADER_NAME, TokenType};
use sea_orm::EntityTrait;

use crate::{
    authentication::{error::Error, jwt::JsonWebToken, password::verify_password},
    models::{User, UserTable},
    state::State,
};

/// Extractor for required authentication
pub struct RequireAuth(pub User);

impl<S> FromRequestParts<S> for RequireAuth
where
    S: Send + Sync,
{
    type Rejection = Error;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let Extension(app_state) = parts
            .extract::<Extension<Arc<State>>>()
            .await
            .map_err(Error::StateMissing)?;

        let auth_header = &parts
            .headers
            .get(AUTHORIZATION_HEADER_NAME)
            .ok_or(Error::MissingAuthorizationHeader)?;

        let (token_type, token) = split_authorization_header(auth_header)?;

        let user = get_user_from_token(&app_state, &token_type, token)
            .await?
            .ok_or(Error::Unauthorized)?;

        Ok(RequireAuth(user))
    }
}

/// Extractor for optional authentication
pub struct OptionalAuth(pub Option<User>);

impl<S> FromRequestParts<S> for OptionalAuth
where
    State: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = Error;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let state = State::from_ref(state);

        let auth_header = match &parts.headers.get("Authorization") {
            Some(header) => *header,
            None => return Ok(OptionalAuth(None)),
        };

        let (token_type, token) = split_authorization_header(auth_header)?;

        let user = get_user_from_token(&state, &token_type, token).await?;

        Ok(OptionalAuth(user))
    }
}

/// Splits the authentication header into token type and token string.
///
/// # Arguments
/// * `header` - The value of the Authorization header to parse
///
fn split_authorization_header(header: &HeaderValue) -> Result<(TokenType, &str), Error> {
    let auth_header_value = header
        .to_str()
        .map_err(|_| Error::InvalidAuthorizationHeader("unable to parse to string"))?;

    let (token_type, token) =
        auth_header_value
            .split_once(" ")
            .ok_or(Error::InvalidAuthorizationHeader(
                "unable to split into token type and token",
            ))?;

    Ok((TokenType::try_from(token_type)?, token))
}

/// Gets user depending on token type.
///
/// # Arguments
/// * `state` - App state for database access and config
/// * `token_type` - Type of token (e.g. JWT)
/// * `token` - The token string to decode and validate
///
async fn get_user_from_token(
    state: &State,
    token_type: &TokenType,
    token: &str,
) -> Result<Option<User>, Error> {
    match token_type {
        TokenType::Jwt => {
            let payload = JsonWebToken::decode(token, state.config())?;

            let user = UserTable::find_by_id(payload.user_id())
                .one(state.db())
                .await
                .map_err(Error::Database)?;

            Ok(user)
        }
        TokenType::Basic => {
            let payload = String::from_utf8(
                BASE64_STANDARD
                    .decode(token)
                    .map_err(Error::BasicAuthDecode)?,
            )
            .map_err(|_| Error::InvalidAuthorizationHeader("unable to decode basic auth token"))?;

            let (username, password) =
                payload
                    .split_once(":")
                    .ok_or(Error::InvalidAuthorizationHeader(
                        "unable to decode basic auth token",
                    ))?;

            if state.config().worker().username() == username
                && verify_password(
                    password.as_bytes(),
                    state.config().worker().password(),
                    i64::MIN,
                )?
            {
                Ok(Some(User {
                    id: i64::MIN,
                    username: username.to_string(),
                    firstname: username.to_string(),
                    lastname: username.to_string(),
                    email: String::new(),
                    password: None,
                }))
            } else {
                Ok(None)
            }
        }
    }
}
