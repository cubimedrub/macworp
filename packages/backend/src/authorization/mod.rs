use std::{marker::PhantomData, sync::Arc};

use axum::{
    Extension, RequestPartsExt,
    extract::{FromRequestParts, Path},
    http::{StatusCode, request::Parts},
    response::{IntoResponse, Response},
};
use macworp_core::entities::Role;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
use thiserror::Error;

/// What happens to the resources and groups of a deleted user
mod departure;
#[cfg(all(test, feature = "integration"))]
mod departure_tests;
/// The visibility filter behind [`Resource::visible_to`]
mod query;
/// The [`Resource`] trait
mod resource;
/// Stored forms of roles, principal types and resource types
mod role;
/// Deciding what becomes of a resource that lost its owner
mod settle;
#[cfg(all(test, feature = "integration"))]
mod share_tests;
/// Granting, changing and removing the shares of a resource
mod shares;

pub use departure::{Departed, Departure};
pub use resource::Resource;
pub(crate) use role::RoleDb;
pub use role::{PrincipalType, ResourceType};
pub use settle::Settled;
pub use shares::{Error as ShareError, Principal, Shares};

use crate::{authentication::middleware::RequireAuth, models::User, state::State};

/// Lowest role an endpoint requires, a type so it can be a generic parameter of [`Authorized`].
pub trait MinRole {
    /// The role itself
    const ROLE: Role;
}

/// [`MinRole`] for [`Role::Viewer`]
pub struct Viewer;
/// [`MinRole`] for [`Role::Editor`]
pub struct Editor;
/// [`MinRole`] for [`Role::Owner`]
pub struct Owner;

impl MinRole for Viewer {
    const ROLE: Role = Role::Viewer;
}
impl MinRole for Editor {
    const ROLE: Role = Role::Editor;
}
impl MinRole for Owner {
    const ROLE: Role = Role::Owner;
}

/// Why an authorization check failed, mapped to an HTTP status by the `IntoResponse` impl
#[derive(Error, Debug)]
pub enum Error {
    /// Missing or invalid credentials, response decided by the authentication error
    #[error("{0}")]
    Authentication(#[from] crate::authentication::error::Error),
    /// Database failure, 500
    #[error("{0}")]
    Database(#[from] sea_orm::DbErr),
    /// The route has no single integer path parameter, 400
    #[error("The path has no resource id")]
    InvalidPath,
    /// No such resource, or the user may not even see it, 404
    #[error("Not found")]
    NotFound,
    /// The user can see the resource but lacks the role, 403
    #[error("Access is forbidden")]
    Forbidden,
}

impl IntoResponse for Error {
    /// Not found and forbidden carry their message, database errors are logged and hidden.
    fn into_response(self) -> Response {
        match self {
            Error::Authentication(e) => e.into_response(),
            Error::InvalidPath => (StatusCode::BAD_REQUEST, self.to_string()).into_response(),
            Error::NotFound => (StatusCode::NOT_FOUND, self.to_string()).into_response(),
            Error::Forbidden => (StatusCode::FORBIDDEN, self.to_string()).into_response(),
            Error::Database(_) => {
                tracing::error!("{self}");
                StatusCode::INTERNAL_SERVER_ERROR.into_response()
            }
        }
    }
}

/// Authenticates the user and loads the resource whose id is the route's only path parameter, if the
/// user holds at least the role `M` on it.
///
/// Resources the user cannot see at all are reported as not found so ids do not leak.
pub struct Authorized<R: Resource, M> {
    /// The authenticated user
    pub user: User,
    /// The loaded resource
    pub model: <R::Entity as EntityTrait>::Model,
    /// Effective role of the user on the resource, at least `M`
    pub role: Role,
    _min: PhantomData<M>,
}

impl<R, M, S> FromRequestParts<S> for Authorized<R, M>
where
    R: Resource,
    <R::Entity as EntityTrait>::Model: Send,
    M: MinRole,
    S: Send + Sync,
{
    type Rejection = Error;

    /// Authenticates, reads the id from the path and checks the role, see [`authorize`].
    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let RequireAuth(user) = RequireAuth::from_request_parts(parts, state).await?;
        let Path(id) = parts
            .extract::<Path<i64>>()
            .await
            .map_err(|_| Error::InvalidPath)?;
        let Extension(app_state) = parts
            .extract::<Extension<Arc<State>>>()
            .await
            .map_err(crate::authentication::error::Error::StateMissing)?;

        authorize::<R, M>(&app_state, user, id).await
    }
}

/// The check behind [`Authorized`], for handlers that get ids from other places than the path.
///
/// Instance admins hold [`Role::Owner`] on every existing resource, whatever its shares. Using that
/// power on a resource the admin has no sufficient share on is logged with target `audit`.
pub async fn authorize<R, M>(state: &State, user: User, id: i64) -> Result<Authorized<R, M>, Error>
where
    R: Resource,
    M: MinRole,
{
    let own_role = R::shares(state.db(), id).role_of(user.id).await?;
    let role = if user.is_admin {
        Role::Owner
    } else {
        own_role.ok_or(Error::NotFound)?
    };
    if role < M::ROLE {
        return Err(Error::Forbidden);
    }
    let model = R::Entity::find()
        .filter(R::id_column().eq(id))
        .one(state.db())
        .await?
        .ok_or(Error::NotFound)?;
    if user.is_admin && own_role.is_none_or(|own| own < M::ROLE) {
        tracing::info!(
            target: "audit",
            admin = %user.username,
            resource = ?R::RESOURCE_TYPE,
            id,
            required = ?M::ROLE,
            "admin access without sufficient share"
        );
    }
    Ok(Authorized {
        user,
        model,
        role,
        _min: PhantomData,
    })
}

/// Tests of [`authorize`] against a SQLite database.
#[cfg(all(test, feature = "integration"))]
mod tests {
    use macworp_database_migration::{Migrator, MigratorTrait};
    use sea_orm::{ActiveModelTrait, ActiveValue::Set};

    use super::*;
    use crate::{
        configuration::Configuration,
        models::{
            ActiveProject, ActiveUser, ActiveWorkflowIntegration, Project, WorkflowIntegration,
        },
    };

    /// Database with two users (`owner`, `outsider`) and a project and workflow owned by `owner`.
    struct World {
        state: State,
        _dir: tempfile::TempDir,
        owner: User,
        outsider: User,
        project: i64,
        workflow: i64,
    }

    /// Inserts a user named `name`.
    async fn user(state: &State, name: &str) -> User {
        ActiveUser {
            username: Set(name.into()),
            firstname: Set(name.into()),
            lastname: Set(name.into()),
            email: Set(format!("{name}@example.org")),
            ..Default::default()
        }
        .insert(state.db())
        .await
        .unwrap()
    }

    /// Creates a migrated SQLite database in a temp dir with the fixtures of [`World`].
    async fn world() -> World {
        let dir = tempfile::tempdir().unwrap();
        let mut config = Configuration::default();
        *config.database_url_mut() = format!("sqlite://{}/test.db?mode=rwc", dir.path().display());
        let state = State::new_for_integration(config).await.unwrap();
        Migrator::up(state.db(), None).await.unwrap();

        let owner = user(&state, "owner").await;
        let outsider = user(&state, "outsider").await;
        let project = ActiveProject {
            owner_id: Set(Some(owner.id)),
            name: Set("p".into()),
            ..Default::default()
        }
        .insert(state.db())
        .await
        .unwrap();
        let workflow = ActiveWorkflowIntegration {
            owner_id: Set(Some(owner.id)),
            name: Set("w".into()),
            description: Set(String::new()),
            ..Default::default()
        }
        .insert(state.db())
        .await
        .unwrap();
        Project::shares(state.db(), project.id)
            .grant(Principal::User(owner.id), Role::Owner)
            .await
            .unwrap();
        WorkflowIntegration::shares(state.db(), workflow.id)
            .grant(Principal::User(owner.id), Role::Owner)
            .await
            .unwrap();
        World {
            state,
            _dir: dir,
            owner,
            outsider,
            project: project.id,
            workflow: workflow.id,
        }
    }

    /// An owner passes the check for every role.
    #[tokio::test]
    async fn owner_passes_every_role() {
        let w = world().await;
        let ok = authorize::<Project, Owner>(&w.state, w.owner.clone(), w.project)
            .await
            .unwrap();
        assert_eq!(ok.role, Role::Owner);
        assert_eq!(ok.model.id, w.project);
    }

    /// Resources a user cannot see, or that do not exist, are not found, so ids do not leak.
    #[tokio::test]
    async fn invisible_resource_is_not_found_not_forbidden() {
        let w = world().await;
        let err = authorize::<Project, Viewer>(&w.state, w.outsider.clone(), w.project)
            .await
            .err()
            .unwrap();
        assert!(matches!(err, Error::NotFound));
        let missing = authorize::<Project, Viewer>(&w.state, w.owner.clone(), 9999)
            .await
            .err()
            .unwrap();
        assert!(matches!(missing, Error::NotFound));
    }

    /// A viewer passes the viewer check but is forbidden from editor endpoints.
    #[tokio::test]
    async fn viewer_is_forbidden_from_editing() {
        let w = world().await;
        Project::shares(w.state.db(), w.project)
            .grant(Principal::User(w.outsider.id), Role::Viewer)
            .await
            .unwrap();
        assert!(
            authorize::<Project, Viewer>(&w.state, w.outsider.clone(), w.project)
                .await
                .is_ok()
        );
        let err = authorize::<Project, Editor>(&w.state, w.outsider.clone(), w.project)
            .await
            .err()
            .unwrap();
        assert!(matches!(err, Error::Forbidden));
    }

    /// Running needs project Editor and workflow Viewer, the two are independent.
    #[tokio::test]
    async fn public_workflow_and_editor_project_allow_running() {
        let w = world().await;
        Project::shares(w.state.db(), w.project)
            .grant(Principal::User(w.outsider.id), Role::Editor)
            .await
            .unwrap();
        assert!(
            authorize::<Project, Editor>(&w.state, w.outsider.clone(), w.project)
                .await
                .is_ok()
        );
        assert!(
            authorize::<WorkflowIntegration, Viewer>(&w.state, w.outsider.clone(), w.workflow)
                .await
                .is_err()
        );
        WorkflowIntegration::shares(w.state.db(), w.workflow)
            .grant(Principal::Public, Role::Viewer)
            .await
            .unwrap();
        assert!(
            authorize::<WorkflowIntegration, Viewer>(&w.state, w.outsider.clone(), w.workflow)
                .await
                .is_ok()
        );
        // Public viewing a workflow gives no right to edit it.
        assert!(matches!(
            authorize::<WorkflowIntegration, Editor>(&w.state, w.outsider.clone(), w.workflow)
                .await
                .err(),
            Some(Error::Forbidden)
        ));
    }

    /// An admin is Owner of resources nobody shared with them, but missing rows stay not found.
    #[tokio::test]
    async fn admin_is_owner_of_unshared_resources() {
        let w = world().await;
        let admin = User {
            is_admin: true,
            ..w.outsider.clone()
        };
        let project = authorize::<Project, Owner>(&w.state, admin.clone(), w.project)
            .await
            .unwrap();
        assert_eq!(project.role, Role::Owner);
        let workflow = authorize::<WorkflowIntegration, Owner>(&w.state, admin.clone(), w.workflow)
            .await
            .unwrap();
        assert_eq!(workflow.model.id, w.workflow);
        assert!(matches!(
            authorize::<Project, Viewer>(&w.state, admin, 9999)
                .await
                .err(),
            Some(Error::NotFound)
        ));
        // The same user without the flag stays an outsider.
        assert!(matches!(
            authorize::<Project, Viewer>(&w.state, w.outsider.clone(), w.project)
                .await
                .err(),
            Some(Error::NotFound)
        ));
    }

    /// The worker identity is not in the database and holds no shares, so it sees nothing.
    #[tokio::test]
    async fn worker_identity_has_no_access() {
        let w = world().await;
        let worker = User {
            id: i64::MIN,
            username: "worker".into(),
            firstname: String::new(),
            lastname: String::new(),
            email: String::new(),
            password: None,
            is_admin: false,
            disabled: false,
        };
        let err = authorize::<Project, Viewer>(&w.state, worker, w.project)
            .await
            .err()
            .unwrap();
        assert!(matches!(err, Error::NotFound));
    }

    /// Runs the `ListScope` extractor for `user` on a request to `/x?{query}`.
    async fn scope_for(
        state: &Arc<State>,
        user: &User,
        query: &str,
    ) -> Result<crate::authentication::middleware::ListScope, crate::authentication::error::Error>
    {
        use axum::extract::FromRequestParts;
        let token =
            crate::authentication::jwt::JsonWebToken::encode(user.id, state.config()).unwrap();
        let (mut parts, _) = axum::http::Request::builder()
            .uri(format!("/x?{query}"))
            .header(http::header::AUTHORIZATION, format!("JWT {token}"))
            .extension(state.clone())
            .body(())
            .unwrap()
            .into_parts();
        crate::authentication::middleware::ListScope::from_request_parts(&mut parts, &()).await
    }

    /// `?all=true` lists other users' private projects for an admin, a normal list or a
    /// non-admin asking for all is restricted or rejected.
    #[tokio::test]
    async fn list_scope_all_is_admin_only() {
        let w = world().await;
        let mut admin = ActiveUser::from(w.outsider.clone());
        admin.is_admin = Set(true);
        let admin = admin.update(w.state.db()).await.unwrap();
        let (owner, project) = (w.owner.clone(), w.project);
        let db_state = Arc::new(w.state);
        let ids = |scope: crate::authentication::middleware::ListScope| {
            let db = db_state.db();
            async move {
                crate::models::ProjectTable::find()
                    .filter(scope.filter(Project::visible_to(scope.user.id, Role::Viewer)))
                    .all(db)
                    .await
                    .unwrap()
                    .into_iter()
                    .map(|p| p.id)
                    .collect::<Vec<_>>()
            }
        };

        // Admin without the flag is an outsider here, with it sees the private project.
        assert!(
            ids(scope_for(&db_state, &admin, "").await.unwrap())
                .await
                .is_empty()
        );
        assert_eq!(
            ids(scope_for(&db_state, &admin, "all=true").await.unwrap()).await,
            vec![project]
        );
        // The owner sees it normally.
        assert_eq!(
            ids(scope_for(&db_state, &owner, "all=false").await.unwrap()).await,
            vec![project]
        );
        // A non-admin may not ask for everything, nor pass a malformed flag.
        let plain = ActiveUser::from(owner.clone())
            .update(db_state.db())
            .await
            .unwrap();
        assert!(matches!(
            scope_for(&db_state, &plain, "all=true").await.err(),
            Some(crate::authentication::error::Error::AdminRequired)
        ));
        assert!(matches!(
            scope_for(&db_state, &admin, "all=maybe").await.err(),
            Some(crate::authentication::error::Error::InvalidQuery(_))
        ));
    }

    /// Runs the `RequireAuth` / `RequireAdmin` extractors for `user`'s JWT.
    async fn auth_for(
        state: &Arc<State>,
        user: &User,
    ) -> (
        Result<crate::authentication::middleware::RequireAuth, crate::authentication::error::Error>,
        Result<
            crate::authentication::middleware::RequireAdmin,
            crate::authentication::error::Error,
        >,
    ) {
        use axum::extract::FromRequestParts;
        let token =
            crate::authentication::jwt::JsonWebToken::encode(user.id, state.config()).unwrap();
        let request = || {
            axum::http::Request::builder()
                .uri("/x")
                .header(http::header::AUTHORIZATION, format!("JWT {token}"))
                .extension(state.clone())
                .body(())
                .unwrap()
                .into_parts()
                .0
        };
        let auth =
            crate::authentication::middleware::RequireAuth::from_request_parts(&mut request(), &())
                .await;
        let admin = crate::authentication::middleware::RequireAdmin::from_request_parts(
            &mut request(),
            &(),
        )
        .await;
        (auth, admin)
    }

    /// A disabled user's still-valid JWT is rejected with `AccountDisabled` (also for an admin,
    /// before the admin check), and works again once the account is re-enabled.
    #[tokio::test]
    async fn disabled_user_token_is_rejected() {
        use crate::authentication::error::Error as AuthError;
        let w = world().await;
        let state = Arc::new(w.state);
        let mut admin = ActiveUser::from(w.outsider.clone());
        admin.is_admin = Set(true);
        admin.disabled = Set(true);
        let mut admin = admin.update(state.db()).await.unwrap();

        let (auth, admin_auth) = auth_for(&state, &admin).await;
        assert!(matches!(auth.err(), Some(AuthError::AccountDisabled)));
        assert!(matches!(admin_auth.err(), Some(AuthError::AccountDisabled)));

        let mut enabled = ActiveUser::from(admin.clone());
        enabled.disabled = Set(false);
        admin = enabled.update(state.db()).await.unwrap();
        let (auth, admin_auth) = auth_for(&state, &admin).await;
        assert!(auth.is_ok());
        assert!(admin_auth.is_ok());
    }

    /// The still-valid JWT of a deleted user is rejected with `Unauthorized`: deleting one's own
    /// account ends the session on the server, the client only has to forget the token.
    #[tokio::test]
    async fn deleted_user_token_is_rejected() {
        use crate::authentication::error::Error as AuthError;
        use sea_orm::ModelTrait;
        let w = world().await;
        let state = Arc::new(w.state);
        let user = w.outsider.clone();

        let (auth, _) = auth_for(&state, &user).await;
        assert!(auth.is_ok());

        user.clone().delete(state.db()).await.unwrap();
        let (auth, _) = auth_for(&state, &user).await;
        assert!(matches!(auth.err(), Some(AuthError::Unauthorized)));
    }
}
