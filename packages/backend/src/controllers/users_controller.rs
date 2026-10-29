use std::sync::Arc;

use axum::{
    Extension, Router,
    extract::{Json, Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, QueryFilter, TransactionTrait,
};
use thiserror::Error;

use macworp_core::{
    requests::{NewUserRequest, ResetPasswordRequest, UpdateUserRequest},
    responses::{UserResponse, UserSummaryResponse},
    routing::UsersRouting,
};

use crate::{
    authentication::middleware::{RequireAdmin, RequireAuth},
    models::{ActiveUser, User, UserTable},
    state::State as AppState,
};

/// Why a users request failed
#[derive(Error, Debug)]
pub enum Error {
    /// Database failure, 500
    #[error("{0}")]
    Database(#[from] sea_orm::DbErr),
    /// Missing or invalid credentials, or not an administrator
    #[error("{0}")]
    Authentication(#[from] crate::authentication::error::Error),
    /// No such user, 404
    #[error("No such user")]
    NotFound,
    /// The username is already in use, 409
    #[error("The username is already taken")]
    UsernameTaken,
    /// The change would leave no enabled administrator, 409
    #[error("There must remain at least one enabled administrator")]
    LastAdmin,
    /// Administrators cannot disable their own account, 409
    #[error("You cannot disable your own account")]
    SelfDisable,
    /// A request field is empty or otherwise unusable, 400
    #[error("{0}")]
    Invalid(&'static str),
}

impl IntoResponse for Error {
    fn into_response(self) -> axum::response::Response {
        tracing::error!("{self}");
        match self {
            Error::Authentication(e) => e.into_response(),
            Error::NotFound => (StatusCode::NOT_FOUND, self.to_string()).into_response(),
            Error::UsernameTaken | Error::LastAdmin | Error::SelfDisable => {
                (StatusCode::CONFLICT, self.to_string()).into_response()
            }
            Error::Invalid(_) => (StatusCode::BAD_REQUEST, self.to_string()).into_response(),
            Error::Database(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
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
            .route(UsersRouting::INDEX_PATH, get(Self::index))
            .route(UsersRouting::NEW_PATH, post(Self::new_user))
            .route(UsersRouting::UPDATE_PATH, post(Self::update))
            .route(
                UsersRouting::RESET_PASSWORD_PATH,
                post(Self::reset_password),
            )
            .route(UsersRouting::DISABLE_PATH, post(Self::disable))
            .route(UsersRouting::ENABLE_PATH, post(Self::enable))
            .layer(Extension(state.clone()))
            .with_state(state)
    }

    async fn current_user(RequireAuth(user): RequireAuth) -> Result<Json<UserResponse>, Error> {
        Ok(Json(Self::response(user)))
    }

    /// The API representation of a user, without the password hash.
    fn response(user: User) -> UserResponse {
        UserResponse {
            id: user.id,
            username: user.username,
            firstname: user.firstname,
            lastname: user.lastname,
            email: user.email,
            is_admin: user.is_admin,
            disabled: user.disabled,
        }
    }

    /// Writes the audit log line of an administrator's action on a user.
    fn audit(admin: &User, target: &User, action: &str) {
        tracing::info!(
            target: "audit",
            admin = %admin.username,
            user = %target.username,
            action,
            "administrator changed a user"
        );
    }

    /// Rejects `candidate` when it is an enabled administrator and the last one, so removing
    /// admin rights from it or disabling it would lock everyone out of administration. Runs inside
    /// the transaction of the change so concurrent requests cannot both pass.
    async fn ensure_not_last_admin(
        db: &impl sea_orm::ConnectionTrait,
        candidate: &User,
    ) -> Result<(), Error> {
        if candidate.is_admin && !candidate.disabled && User::enabled_admin_count(db).await? <= 1 {
            return Err(Error::LastAdmin);
        }
        Ok(())
    }

    /// Hashes a new password, rejecting an empty one.
    fn hash_password(password: &str) -> Result<String, Error> {
        if password.is_empty() {
            return Err(Error::Invalid("The password must not be empty"));
        }
        Ok(password_auth::generate_hash(password.as_bytes()))
    }

    /// Loads a user by id.
    async fn find(db: &impl sea_orm::ConnectionTrait, id: i64) -> Result<User, Error> {
        UserTable::find_by_id(id)
            .one(db)
            .await?
            .ok_or(Error::NotFound)
    }

    /// All users, filtered by the optional query parameter `q`. Administrators only.
    async fn index(
        State(state): State<Arc<AppState>>,
        RequireAdmin(_admin): RequireAdmin,
        Query(query): Query<UserIndexQuery>,
    ) -> Result<Json<Vec<UserResponse>>, Error> {
        let users = User::list(state.db(), query.q.as_deref()).await?;
        Ok(Json(users.into_iter().map(Self::response).collect()))
    }

    /// Creates an internal user with a local password. External users are provisioned by their
    /// login provider.
    async fn new_user(
        State(state): State<Arc<AppState>>,
        RequireAdmin(admin): RequireAdmin,
        Json(request): Json<NewUserRequest>,
    ) -> Result<Json<UserResponse>, Error> {
        Self::create(&state, &admin, request).await.map(Json)
    }

    /// Implementation of [`Self::new_user`].
    async fn create(
        state: &AppState,
        admin: &User,
        request: NewUserRequest,
    ) -> Result<UserResponse, Error> {
        let username = request.username.trim();
        if username.is_empty() {
            return Err(Error::Invalid("The username must not be empty"));
        }
        let password = Self::hash_password(&request.password)?;

        let transaction = state.db().begin().await?;
        if UserTable::find()
            .filter(crate::models::user::Column::Username.eq(username))
            .one(&transaction)
            .await?
            .is_some()
        {
            return Err(Error::UsernameTaken);
        }
        let user = ActiveUser {
            username: Set(username.to_owned()),
            firstname: Set(request.firstname),
            lastname: Set(request.lastname),
            email: Set(request.email),
            password: Set(Some(password)),
            is_admin: Set(request.is_admin),
            ..Default::default()
        }
        .insert(&transaction)
        .await?;
        transaction.commit().await?;

        Self::audit(admin, &user, "create");
        Ok(Self::response(user))
    }

    /// Changes names, email and admin flag of a user.
    async fn update(
        State(state): State<Arc<AppState>>,
        RequireAdmin(admin): RequireAdmin,
        Path(id): Path<i64>,
        Json(request): Json<UpdateUserRequest>,
    ) -> Result<Json<UserResponse>, Error> {
        Self::apply_update(&state, &admin, id, request)
            .await
            .map(Json)
    }

    /// Implementation of [`Self::update`].
    async fn apply_update(
        state: &AppState,
        admin: &User,
        id: i64,
        request: UpdateUserRequest,
    ) -> Result<UserResponse, Error> {
        let transaction = state.db().begin().await?;
        let user = Self::find(&transaction, id).await?;
        if user.is_admin && !request.is_admin {
            Self::ensure_not_last_admin(&transaction, &user).await?;
        }
        let mut active = ActiveUser::from(user);
        active.firstname = Set(request.firstname);
        active.lastname = Set(request.lastname);
        active.email = Set(request.email);
        active.is_admin = Set(request.is_admin);
        let user = active.update(&transaction).await?;
        transaction.commit().await?;

        Self::audit(admin, &user, "update");
        Ok(Self::response(user))
    }

    /// Sets a new local password for a user, also one that had none.
    async fn reset_password(
        State(state): State<Arc<AppState>>,
        RequireAdmin(admin): RequireAdmin,
        Path(id): Path<i64>,
        Json(request): Json<ResetPasswordRequest>,
    ) -> Result<StatusCode, Error> {
        Self::apply_reset_password(&state, &admin, id, &request.password).await?;
        Ok(StatusCode::OK)
    }

    /// Implementation of [`Self::reset_password`].
    async fn apply_reset_password(
        state: &AppState,
        admin: &User,
        id: i64,
        password: &str,
    ) -> Result<(), Error> {
        let hash = Self::hash_password(password)?;
        let user = Self::find(state.db(), id).await?;
        let mut active = ActiveUser::from(user);
        active.password = Set(Some(hash));
        let user = active.update(state.db()).await?;

        Self::audit(admin, &user, "reset-password");
        Ok(())
    }

    /// Blocks a user from logging in; their existing tokens stop working as well.
    async fn disable(
        State(state): State<Arc<AppState>>,
        RequireAdmin(admin): RequireAdmin,
        Path(id): Path<i64>,
    ) -> Result<Json<UserResponse>, Error> {
        Self::set_disabled(&state, &admin, id, true).await.map(Json)
    }

    /// Lifts the block of a user.
    async fn enable(
        State(state): State<Arc<AppState>>,
        RequireAdmin(admin): RequireAdmin,
        Path(id): Path<i64>,
    ) -> Result<Json<UserResponse>, Error> {
        Self::set_disabled(&state, &admin, id, false)
            .await
            .map(Json)
    }

    /// Implementation of [`Self::disable`] and [`Self::enable`]; setting the state a user already
    /// has succeeds without a change.
    async fn set_disabled(
        state: &AppState,
        admin: &User,
        id: i64,
        disabled: bool,
    ) -> Result<UserResponse, Error> {
        if disabled && admin.id == id {
            return Err(Error::SelfDisable);
        }
        let transaction = state.db().begin().await?;
        let user = Self::find(&transaction, id).await?;
        let user = if user.disabled == disabled {
            user
        } else {
            if disabled {
                Self::ensure_not_last_admin(&transaction, &user).await?;
            }
            let mut active = ActiveUser::from(user);
            active.disabled = Set(disabled);
            active.update(&transaction).await?
        };
        transaction.commit().await?;

        Self::audit(admin, &user, if disabled { "disable" } else { "enable" });
        Ok(Self::response(user))
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

/// Query parameters of the admin user list
#[derive(serde::Deserialize)]
struct UserIndexQuery {
    /// Text the username, names or email must contain
    q: Option<String>,
}

/// Query parameters of the user search
#[derive(serde::Deserialize)]
struct UserSearchQuery {
    /// Username prefix
    q: String,
}

/// Tests of the user administration against a SQLite database.
#[cfg(all(test, feature = "integration"))]
mod tests {
    use macworp_database_migration::{Migrator, MigratorTrait};

    use super::*;
    use crate::{
        authentication::password::verify_password, configuration::Configuration, models::User,
    };

    /// Migrated SQLite database in a temp dir, kept alive by the returned guard.
    async fn state() -> (AppState, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let mut config = Configuration::default();
        *config.database_url_mut() = format!("sqlite://{}/test.db?mode=rwc", dir.path().display());
        let state = AppState::new_for_integration(config).await.unwrap();
        Migrator::up(state.db(), None).await.unwrap();
        (state, dir)
    }

    /// Request creating `name`, an administrator when `is_admin`, with password `secret`.
    fn new_user(name: &str, is_admin: bool) -> NewUserRequest {
        NewUserRequest {
            username: name.into(),
            firstname: name.into(),
            lastname: name.into(),
            email: format!("{name}@example.org"),
            is_admin,
            password: "secret".into(),
        }
    }

    /// Creates `name` through the controller and loads the row.
    async fn make(state: &AppState, admin: &User, name: &str, is_admin: bool) -> User {
        let response = UsersController::create(state, admin, new_user(name, is_admin))
            .await
            .unwrap();
        UsersController::find(state.db(), response.id)
            .await
            .unwrap()
    }

    /// Inserts the first administrator directly, as the CLI does.
    async fn first_admin(state: &AppState) -> User {
        ActiveUser {
            username: Set("root".into()),
            firstname: Set("root".into()),
            lastname: Set("root".into()),
            email: Set("root@example.org".into()),
            is_admin: Set(true),
            ..Default::default()
        }
        .insert(state.db())
        .await
        .unwrap()
    }

    /// Update request keeping `user`'s data but with the given admin flag.
    fn update_request(user: &User, is_admin: bool) -> UpdateUserRequest {
        UpdateUserRequest {
            firstname: user.firstname.clone(),
            lastname: user.lastname.clone(),
            email: user.email.clone(),
            is_admin,
        }
    }

    /// Creating hashes the password, and duplicate or empty input is rejected.
    #[tokio::test]
    async fn create_validates_and_hashes() {
        let (state, _dir) = state().await;
        let admin = first_admin(&state).await;

        let alice = make(&state, &admin, "alice", false).await;
        let hash = alice.password.as_deref().unwrap();
        assert!(verify_password(b"secret", hash, alice.id).unwrap());

        assert!(matches!(
            UsersController::create(&state, &admin, new_user("alice", false)).await,
            Err(Error::UsernameTaken)
        ));
        let mut empty = new_user("carol", false);
        empty.password = String::new();
        assert!(matches!(
            UsersController::create(&state, &admin, empty).await,
            Err(Error::Invalid(_))
        ));
        assert!(matches!(
            UsersController::create(&state, &admin, new_user("  ", false)).await,
            Err(Error::Invalid(_))
        ));
    }

    /// Update changes the fields; the last enabled admin cannot lose the flag, another one can,
    /// and a disabled admin does not count as a remaining one.
    #[tokio::test]
    async fn update_protects_last_admin() {
        let (state, _dir) = state().await;
        let root = first_admin(&state).await;

        let mut request = update_request(&root, false);
        assert!(matches!(
            UsersController::apply_update(&state, &root, root.id, request).await,
            Err(Error::LastAdmin)
        ));

        let other = make(&state, &root, "other", true).await;
        request = update_request(&other, true);
        request.firstname = "Changed".into();
        let response = UsersController::apply_update(&state, &root, other.id, request)
            .await
            .unwrap();
        assert_eq!(response.firstname, "Changed");

        UsersController::set_disabled(&state, &root, other.id, true)
            .await
            .unwrap();
        assert!(matches!(
            UsersController::apply_update(&state, &root, root.id, update_request(&root, false))
                .await,
            Err(Error::LastAdmin)
        ));

        UsersController::set_disabled(&state, &root, other.id, false)
            .await
            .unwrap();
        let response =
            UsersController::apply_update(&state, &root, root.id, update_request(&root, false))
                .await
                .unwrap();
        assert!(!response.is_admin);

        assert!(matches!(
            UsersController::apply_update(&state, &root, 9999, update_request(&root, false)).await,
            Err(Error::NotFound)
        ));
    }

    /// Disabling rejects self and the last admin, is idempotent, and enabling restores the user.
    #[tokio::test]
    async fn disable_and_enable() {
        let (state, _dir) = state().await;
        let root = first_admin(&state).await;
        let alice = make(&state, &root, "alice", false).await;

        assert!(matches!(
            UsersController::set_disabled(&state, &root, root.id, true).await,
            Err(Error::SelfDisable)
        ));

        let other = make(&state, &root, "other", true).await;
        // `other` is not the last admin while `root` is enabled
        assert!(
            UsersController::set_disabled(&state, &root, other.id, true)
                .await
                .unwrap()
                .disabled
        );
        // now `root` is the only enabled admin; another admin acting cannot disable it either
        let acting = UsersController::find(state.db(), other.id).await.unwrap();
        assert!(matches!(
            UsersController::set_disabled(&state, &acting, root.id, true).await,
            Err(Error::LastAdmin)
        ));

        assert!(
            UsersController::set_disabled(&state, &root, alice.id, true)
                .await
                .unwrap()
                .disabled
        );
        assert!(
            UsersController::set_disabled(&state, &root, alice.id, true)
                .await
                .unwrap()
                .disabled
        );
        assert!(
            !UsersController::set_disabled(&state, &root, alice.id, false)
                .await
                .unwrap()
                .disabled
        );
        assert!(matches!(
            UsersController::set_disabled(&state, &root, 9999, false).await,
            Err(Error::NotFound)
        ));
    }

    /// A reset password replaces the old one, also when the user had none.
    #[tokio::test]
    async fn reset_password_replaces_hash() {
        let (state, _dir) = state().await;
        let root = first_admin(&state).await;
        let alice = make(&state, &root, "alice", false).await;

        UsersController::apply_reset_password(&state, &root, alice.id, "new")
            .await
            .unwrap();
        let alice = UsersController::find(state.db(), alice.id).await.unwrap();
        let hash = alice.password.as_deref().unwrap();
        assert!(verify_password(b"new", hash, alice.id).unwrap());
        assert!(verify_password(b"secret", hash, alice.id).is_err());

        let none = make(&state, &root, "none", false).await;
        let mut active = ActiveUser::from(none.clone());
        active.password = Set(None);
        active.update(state.db()).await.unwrap();
        UsersController::apply_reset_password(&state, &root, none.id, "x")
            .await
            .unwrap();
        assert!(
            UsersController::find(state.db(), none.id)
                .await
                .unwrap()
                .password
                .is_some()
        );

        assert!(matches!(
            UsersController::apply_reset_password(&state, &root, alice.id, "").await,
            Err(Error::Invalid(_))
        ));
    }

    /// The admin list is sorted and `q` matches username, names and email.
    #[tokio::test]
    async fn list_filters() {
        let (state, _dir) = state().await;
        let root = first_admin(&state).await;
        make(&state, &root, "zed", false).await;
        make(&state, &root, "alice", false).await;

        let all = User::list(state.db(), None).await.unwrap();
        let names: Vec<_> = all.iter().map(|u| u.username.as_str()).collect();
        assert_eq!(names, ["alice", "root", "zed"]);
        let found = User::list(state.db(), Some("lic")).await.unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(
            User::list(state.db(), Some("example.org"))
                .await
                .unwrap()
                .len(),
            3
        );
        assert_eq!(User::list(state.db(), Some("  ")).await.unwrap().len(), 3);
    }
}
