use std::sync::Arc;

use axum::{
    Extension, Router,
    extract::{Json, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::get,
};
use thiserror::Error;

use macworp_core::{
    responses::{UserResponse, UserSummaryResponse},
    routing::UsersRouting,
};

use crate::{authentication::middleware::RequireAuth, models::User, state::State as AppState};

#[derive(Error, Debug)]
pub enum Error {
    #[error("{0}")]
    Database(#[from] sea_orm::DbErr),
    #[error("{0}")]
    Authentication(#[from] crate::authentication::error::Error),
}

impl IntoResponse for Error {
    fn into_response(self) -> axum::response::Response {
        tracing::error!("{self}");
        match self {
            Error::Authentication(e) => e.into_response(),
            _ => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
        }
    }
}

pub struct UsersController;

impl UsersController {
    /// Shortest search query that returns users
    const SEARCH_MIN_LENGTH: usize = 2;
    /// Most users a search returns
    const SEARCH_LIMIT: u64 = 10;

    pub fn routes(state: Arc<AppState>) -> Router<Arc<AppState>> {
        Router::new()
            .route(UsersRouting::CURRENT_PATH, get(Self::current_user))
            .route(UsersRouting::SEARCH_PATH, get(Self::search))
            .layer(Extension(state.clone()))
            .with_state(state)
    }

    async fn current_user(RequireAuth(user): RequireAuth) -> Result<Json<UserResponse>, Error> {
        let response = UserResponse {
            id: user.id,
            username: user.username,
            firstname: user.firstname,
            lastname: user.lastname,
            email: user.email,
            is_admin: user.is_admin,
            disabled: user.disabled,
        };

        Ok(Json(response))
    }

    /// Users whose username starts with the query parameter `q`, at most [`Self::SEARCH_LIMIT`].
    /// Queries shorter than [`Self::SEARCH_MIN_LENGTH`] return nothing, so the user list cannot
    /// be dumped.
    async fn search(
        State(state): State<Arc<AppState>>,
        RequireAuth(_user): RequireAuth,
        Query(query): Query<UserSearchQuery>,
    ) -> Result<Json<Vec<UserSummaryResponse>>, Error> {
        let prefix = query.q.trim();
        if prefix.chars().count() < Self::SEARCH_MIN_LENGTH {
            return Ok(Json(Vec::new()));
        }
        let users = User::search(state.db(), prefix, Self::SEARCH_LIMIT).await?;
        Ok(Json(
            users
                .into_iter()
                .map(|user| UserSummaryResponse {
                    id: user.id,
                    username: user.username,
                })
                .collect(),
        ))
    }
}

/// Query parameters of the user search
#[derive(serde::Deserialize)]
struct UserSearchQuery {
    /// Username prefix
    q: String,
}
