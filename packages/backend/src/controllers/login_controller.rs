use std::sync::Arc;

use axum::{Router, response::IntoResponse, routing::post};

use macworp_core::routing::LoginRouting;

use crate::state::State;

pub struct LoginController;

impl LoginController {
    pub fn routes() -> Router<Arc<State>> {
        Router::new().route(
            LoginRouting::login_password_path(),
            post(Self::login_password),
        )
    }

    pub async fn login_password() -> impl IntoResponse {}
}
