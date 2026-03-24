use std::sync::Arc;

use axum::{
    Router,
    extract::{Json, Path, State},
    response::Redirect,
    routing::{get, post},
};

use macworp_core::{
    requests::PasswordLoginRequest, responses::LoginProviderResponse, routing::LoginRouting,
};

use crate::{
    authentication::{
        error::Error as AuthenticationError, jwt::JsonWebToken, password::verify_password,
    },
    configuration::LoginProvider,
    controllers::errors::LoginControllerError,
    models::UserTable,
    state::State as AppState,
};

pub struct LoginController;

impl LoginController {
    pub fn routes(state: Arc<AppState>) -> Router<Arc<AppState>> {
        let mut router: Router<Arc<AppState>> = Router::new()
            .route(
                LoginRouting::LOGIN_PROVIDERS_PATH,
                get(Self::login_providers),
            )
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

        let jwt = JsonWebToken::encode(user.id, state.config())
            .map_err(LoginControllerError::Authentication)?;

        Ok(jwt)
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
