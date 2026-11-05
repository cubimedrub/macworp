use std::sync::Arc;

use axum::{
    Extension, RequestPartsExt,
    extract::{FromRef, FromRequestParts, Path, Query},
    http::{HeaderValue, request::Parts},
};
use base64::prelude::*;
use macworp_core::authentication::TokenType;
use sea_orm::{Condition, EntityTrait};
use serde::Deserialize;

use crate::{
    authentication::{error::Error, jwt::JsonWebToken, password::verify_password},
    models::{Run, RunTable, User, UserTable},
    state::State,
};

/// Id of the synthetic worker identity (Basic authentication). It has no row in the users table.
pub const WORKER_ID: i64 = i64::MIN;

/// Extractor for required authentication, accepts users and the worker identity. User-facing
/// endpoints use [`RequireUser`] instead.
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
            .get(http::header::AUTHORIZATION)
            .ok_or(Error::MissingAuthorizationHeader)?;

        let (token_type, token) = split_authorization_header(auth_header)?;

        let user = get_user_from_token(&app_state, &token_type, token)
            .await?
            .ok_or(Error::Unauthorized)?;

        Ok(RequireAuth(user))
    }
}

/// Extractor for user-facing endpoints: requires authentication and rejects the worker identity
/// with 403 (`Error::UserRequired`), since it has no user row to own or act on anything.
pub struct RequireUser(pub User);

impl<S> FromRequestParts<S> for RequireUser
where
    S: Send + Sync,
{
    type Rejection = Error;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let RequireAuth(user) = RequireAuth::from_request_parts(parts, state).await?;
        if user.id == WORKER_ID {
            Err(Error::UserRequired)
        } else {
            Ok(RequireUser(user))
        }
    }
}

/// Extractor for worker endpoints that have no run yet (claim): requires the static worker
/// credential (`Basic`). Users and run tokens are rejected, the worker is not a user.
pub struct RequireWorker {
    /// Username of the worker credential
    pub username: String,
}

impl<S> FromRequestParts<S> for RequireWorker
where
    S: Send + Sync,
{
    type Rejection = Error;

    /// Reads the `Authorization` header and checks it against the configured worker credential.
    ///
    /// Rejections: 400 if the header is missing or malformed, 401 if the token type is not
    /// `Basic` (JWTs and run tokens are refused) or username/password do not match.
    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let Extension(app_state) = parts
            .extract::<Extension<Arc<State>>>()
            .await
            .map_err(Error::StateMissing)?;
        let header = parts
            .headers
            .get(http::header::AUTHORIZATION)
            .ok_or(Error::MissingAuthorizationHeader)?;
        let (token_type, token) = split_authorization_header(header)?;
        if !matches!(token_type, TokenType::Basic) {
            return Err(Error::Unauthorized);
        }
        let username = worker_username_from_basic(&app_state, token)?.ok_or(Error::Unauthorized)?;
        Ok(RequireWorker { username })
    }
}

/// Extractor for worker endpoints of one run (path parameter `run_id`): requires
/// `Authorization: Run <token>` with the token issued when the run was claimed. Fails with 401 if
/// the run is unknown, not running (finished, cancelled, requeued after a lost lease) or the token
/// belongs to an earlier claim, so a stale worker can never write into a requeued run.
pub struct RequireRunToken(pub Run);

impl<S> FromRequestParts<S> for RequireRunToken
where
    S: Send + Sync,
{
    type Rejection = Error;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let Extension(app_state) = parts
            .extract::<Extension<Arc<State>>>()
            .await
            .map_err(Error::StateMissing)?;
        let Path(run_id) = parts
            .extract::<Path<i64>>()
            .await
            .map_err(|_| Error::InvalidAuthorizationHeader("invalid run id"))?;
        let header = parts
            .headers
            .get(http::header::AUTHORIZATION)
            .ok_or(Error::MissingAuthorizationHeader)?;
        let (token_type, token) = split_authorization_header(header)?;
        if !matches!(token_type, TokenType::Run) {
            return Err(Error::Unauthorized);
        }
        let run = RunTable::authenticate(app_state.db(), run_id, token)
            .await
            .map_err(|error| match error {
                crate::models::RunError::Database(e) => Error::Database(e),
                _ => Error::Unauthorized,
            })?;
        Ok(RequireRunToken(run))
    }
}

/// Checks a Basic token against the configured worker credential and returns its username.
fn worker_username_from_basic(state: &State, token: &str) -> Result<Option<String>, Error> {
    let payload = String::from_utf8(
        BASE64_STANDARD
            .decode(token)
            .map_err(Error::BasicAuthDecode)?,
    )
    .map_err(|_| Error::InvalidAuthorizationHeader("unable to decode basic auth token"))?;
    let (username, password) = payload
        .split_once(":")
        .ok_or(Error::InvalidAuthorizationHeader(
            "unable to decode basic auth token",
        ))?;
    let worker = state.config().worker();
    if worker.username() == username
        && verify_password(password.as_bytes(), worker.password(), WORKER_ID)?
    {
        Ok(Some(username.to_string()))
    } else {
        Ok(None)
    }
}

/// Extractor for endpoints only instance administrators may call, rejects everyone else with 403
/// (worker identity included).
pub struct RequireAdmin(pub User);

impl<S> FromRequestParts<S> for RequireAdmin
where
    S: Send + Sync,
{
    type Rejection = Error;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let RequireUser(user) = RequireUser::from_request_parts(parts, state).await?;
        if user.is_admin {
            Ok(RequireAdmin(user))
        } else {
            Err(Error::AdminRequired)
        }
    }
}

/// The `?all=` query parameter of list endpoints
#[derive(Deserialize, Default)]
struct AllParam {
    /// Whether to list every item instead of only the visible ones
    #[serde(default)]
    all: bool,
}

/// Extractor for list endpoints: the authenticated user plus whether they asked for `?all=true`.
/// Only administrators may do that, anyone else gets 403.
pub struct ListScope {
    /// The authenticated user
    pub user: User,
    /// Whether the list is not restricted to what the user can see
    pub all: bool,
}

impl ListScope {
    /// The condition to filter the list query with: `visible` normally, no restriction for an
    /// administrator who asked for all items.
    pub fn filter(&self, visible: Condition) -> Condition {
        if self.all { Condition::all() } else { visible }
    }
}

impl<S> FromRequestParts<S> for ListScope
where
    S: Send + Sync,
{
    type Rejection = Error;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let RequireUser(user) = RequireUser::from_request_parts(parts, state).await?;
        let Query(AllParam { all }) = Query::<AllParam>::from_request_parts(parts, state)
            .await
            .map_err(Error::InvalidQuery)?;
        if all {
            if !user.is_admin {
                return Err(Error::AdminRequired);
            }
            tracing::info!(target: "audit", admin = %user.username, "listing all items");
        }
        Ok(ListScope { user, all })
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

        let auth_header = match &parts.headers.get(http::header::AUTHORIZATION) {
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

            let user = UserTable::find_by_id(payload.user_id()?)
                .one(state.db())
                .await
                .map_err(Error::Database)?;

            if user.as_ref().is_some_and(|user| user.disabled) {
                return Err(Error::AccountDisabled);
            }

            // Tokens issued before the last logout carry an older version.
            if user
                .as_ref()
                .is_some_and(|user| user.token_version != payload.version())
            {
                return Err(Error::Unauthorized);
            }

            Ok(user)
        }
        // Run tokens authenticate workers on one run only, never users
        TokenType::Run => Ok(None),
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
                    WORKER_ID,
                )?
            {
                Ok(Some(User {
                    id: WORKER_ID,
                    username: username.to_string(),
                    firstname: username.to_string(),
                    lastname: username.to_string(),
                    email: String::new(),
                    password: None,
                    is_admin: false,
                    disabled: false,
                    token_version: 0,
                }))
            } else {
                Ok(None)
            }
        }
    }
}
