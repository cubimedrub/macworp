use std::sync::Arc;

use axum::{
    Extension, Router, extract::Json, http::StatusCode, response::IntoResponse, routing::get,
};
use thiserror::Error;

use macworp_core::{responses::UserResponse, routing::UsersRouting};

use crate::{authentication::middleware::RequireAuth, state::State as AppState};

#[derive(Error, Debug)]
pub enum UsersControllerError {
    #[error("{0}")]
    Database(#[from] sea_orm::DbErr),
    #[error("{0}")]
    Authentication(#[from] crate::authentication::error::Error),
}

impl IntoResponse for UsersControllerError {
    fn into_response(self) -> axum::response::Response {
        tracing::error!("{self}");
        match self {
            UsersControllerError::Authentication(e) => e.into_response(),
            _ => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
        }
    }
}

pub struct UsersController;

impl UsersController {
    pub fn routes(state: Arc<AppState>) -> Router<Arc<AppState>> {
        Router::new()
            .route(UsersRouting::CURRENT_PATH, get(Self::current_user))
            .layer(Extension(state.clone()))
            .with_state(state)
    }

    async fn current_user(
        RequireAuth(user): RequireAuth,
    ) -> Result<Json<UserResponse>, UsersControllerError> {
        let response = UserResponse {
            id: user.id,
            username: user.username,
            firstname: user.firstname,
            lastname: user.lastname,
            email: user.email,
        };

        Ok(Json(response))
    }
}
