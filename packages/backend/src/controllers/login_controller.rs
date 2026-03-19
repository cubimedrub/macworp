use std::sync::Arc;

use axum::{
    Router,
    extract::{Json, State},
    routing::post,
};

use macworp_core::{requests::PasswordLoginRequest, routing::LoginRouting};

use crate::{
    authentication::{
        error::Error as AuthenticationError, jwt::JsonWebToken, password::verify_password,
    },
    controllers::errors::LoginControllerError,
    models::UserTable,
    state::State as AppState,
};

pub struct LoginController;

impl LoginController {
    pub fn routes(state: Arc<AppState>) -> Router<Arc<AppState>> {
        Router::new()
            .route(
                LoginRouting::with_password_path(),
                post(Self::with_password),
            )
            .with_state(state)
    }

    pub async fn with_password(
        State(state): State<Arc<AppState>>,
        Json(payload): Json<PasswordLoginRequest>,
    ) -> Result<String, LoginControllerError> {
        let user = UserTable::find_by_username(&payload.username)
            .one(state.db())
            .await
            .map_err(LoginControllerError::Database)?
            .ok_or(AuthenticationError::InvalidUserOrPassword)?;

        let user_password = user
            .password
            .as_ref()
            .ok_or(AuthenticationError::InvalidUserOrPassword)?;

        verify_password(payload.password.as_bytes(), user_password, user.id)
            .map_err(LoginControllerError::Authentication)?;

        let jwt = JsonWebToken::encode(user.id, state.config())
            .map_err(LoginControllerError::Authentication)?;

        Ok(jwt)
    }
}
