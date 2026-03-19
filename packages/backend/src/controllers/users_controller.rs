use std::sync::Arc;

use axum::{Extension, Router, extract::Json, routing::get};

use macworp_core::{responses::UserResponse, routing::UsersRouting};

use crate::{
    authentication::middleware::RequireAuth, controllers::errors::UsersControllerError,
    state::State as AppState,
};

pub struct UsersController;

impl UsersController {
    pub fn routes(state: Arc<AppState>) -> Router<Arc<AppState>> {
        Router::new()
            .route(UsersRouting::current_path(), get(Self::current_user))
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
