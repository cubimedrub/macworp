use std::sync::Arc;

use axum::{
    Router,
    extract::{Json, Path, State},
    http::StatusCode,
    response::{IntoResponse, Redirect},
    routing::{get, post},
};

use thiserror::Error;

use macworp_core::{
    requests::PasswordLoginRequest, responses::LoginProviderResponse, routing::LoginRouting,
};

use crate::{
    authentication::{
        error::Error as AuthenticationError, jwt::JsonWebToken, middleware::RequireAuth,
        password::verify_password,
    },
    configuration::LoginProvider,
    models::{User, UserTable},
    state::State as AppState,
};

#[derive(Error, Debug)]
pub enum LoginControllerError {
    #[error("{0}")]
    Database(#[from] sea_orm::DbErr),
    #[error("{0}")]
    Authentication(#[from] crate::authentication::error::Error),
}

impl IntoResponse for LoginControllerError {
    fn into_response(self) -> axum::response::Response {
        tracing::error!("{self}");
        match self {
            LoginControllerError::Authentication(e) => e.into_response(),
            _ => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
        }
    }
}

pub struct LoginController;

impl LoginController {
    pub fn routes(state: Arc<AppState>) -> Router<Arc<AppState>> {
        let mut router: Router<Arc<AppState>> = Router::new()
            .route(
                LoginRouting::LOGIN_PROVIDERS_PATH,
                get(Self::login_providers),
            )
            .route(LoginRouting::LOGOUT_PATH, post(Self::logout))
            .route(
                LoginRouting::WITH_OPEN_ID_CONNECT,
                get(Self::with_open_id_connect),
            );
        if let Some(_provider) = state
            .config()
            .login_providers()
            .iter()
            .position(|e| matches!(e, LoginProvider::Local(_)))
        {
            router = router.route(LoginRouting::WITH_LOCAL, post(Self::with_local));

            // TOOD: Enable registration route if acivated in provider
        }

        router.with_state(state)
    }

    pub async fn with_local(
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

        // Checked after the password so account state is not revealed without it.
        if user.disabled {
            return Err(AuthenticationError::AccountDisabled.into());
        }

        let jwt = JsonWebToken::encode(user.id, user.token_version, state.config())
            .map_err(LoginControllerError::Authentication)?;

        Ok(jwt)
    }

    /// Revokes all tokens of the caller (this one included) by bumping the user's token version.
    pub async fn logout(
        State(state): State<Arc<AppState>>,
        RequireAuth(user): RequireAuth,
    ) -> Result<StatusCode, LoginControllerError> {
        User::revoke_tokens(state.db(), user.id).await?;
        Ok(StatusCode::NO_CONTENT)
    }

    pub async fn login_providers(
        State(state): State<Arc<AppState>>,
    ) -> Json<Vec<LoginProviderResponse>> {
        Json(
            state
                .config()
                .login_providers()
                .iter()
                .map(LoginProviderResponse::from)
                .collect::<Vec<_>>(),
        )
    }

    pub async fn with_open_id_connect(
        State(_state): State<Arc<AppState>>,
        Path(_name): Path<String>,
    ) -> Redirect {
        // TODO: redirect to OIDC provider here and a callback endpoint
        unimplemented!()
    }
}
