use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
};
use macworp_core::{
    requests::{GrantShareRequest, PrincipalRequest, RevokeShareRequest},
    responses::{PrincipalResponse, ShareResponse},
    routing::SharesRouting,
};
use sea_orm::{EntityTrait, TransactionTrait};
use thiserror::Error;

use crate::{
    authentication::middleware::RequireUser,
    authorization::{Owner, Principal, Resource, RoleDb, ShareError, authorize},
    models::{Group, GroupTable, Project, User, UserTable, WorkflowIntegration},
    state::State as AppState,
};

/// Why a shares request failed
#[derive(Error, Debug)]
pub enum Error {
    /// Database failure, 500
    #[error("{0}")]
    Database(#[from] sea_orm::DbErr),
    /// A share rule was violated, see [`ShareError`]
    #[error("{0}")]
    Share(#[from] ShareError),
    /// Authentication or authorization failed
    #[error("{0}")]
    Authorization(#[from] crate::authorization::Error),
    /// The path names no shareable resource kind, 404
    #[error("Unknown resource kind")]
    UnknownKind,
    /// The user or group to share with does not exist or the user may not use it, 404
    #[error("No such user or group")]
    UnknownPrincipal,
}

impl IntoResponse for Error {
    fn into_response(self) -> axum::response::Response {
        match self {
            Self::Database(_) => {
                tracing::error!("{self}");
                StatusCode::INTERNAL_SERVER_ERROR.into_response()
            }
            Self::Share(e) => e.into_response(),
            Self::Authorization(e) => e.into_response(),
            Self::UnknownKind | Self::UnknownPrincipal => {
                (StatusCode::NOT_FOUND, self.to_string()).into_response()
            }
        }
    }
}

/// The kinds of resources that can be shared, as named in the route
enum Kind {
    /// `projects`
    Project,
    /// `workflows`
    Workflow,
}

impl Kind {
    /// Parses the `{kind}` route segment.
    fn parse(segment: &str) -> Result<Self, Error> {
        match segment {
            "projects" => Ok(Self::Project),
            "workflows" => Ok(Self::Workflow),
            _ => Err(Error::UnknownKind),
        }
    }
}

/// Endpoints to list, grant and revoke the shares of projects and workflows
pub struct SharesController;

impl SharesController {
    pub fn routes(state: Arc<AppState>) -> Router<Arc<AppState>> {
        Router::new()
            .route(SharesRouting::LIST_PATH, get(Self::list))
            .route(SharesRouting::GRANT_PATH, post(Self::grant))
            .route(SharesRouting::REVOKE_PATH, post(Self::revoke))
            .with_state(state)
    }

    /// Shares of resource `R`, after checking that `user` owns it.
    async fn list_of<R: Resource + Send>(
        state: &AppState,
        user: User,
        id: i64,
    ) -> Result<Vec<ShareResponse>, Error>
    where
        <R::Entity as EntityTrait>::Model: Send,
    {
        authorize::<R, Owner>(state, user, id).await?;
        let rows = R::shares(state.db(), id).list().await?;
        let users = User::usernames(
            state.db(),
            rows.iter()
                .filter(|r| r.principal_type == crate::authorization::PrincipalType::User as i16)
                .filter_map(|r| r.principal_id),
        )
        .await?;
        let groups = Group::names(
            state.db(),
            rows.iter()
                .filter(|r| r.principal_type == crate::authorization::PrincipalType::Group as i16)
                .filter_map(|r| r.principal_id),
        )
        .await?;
        Ok(rows
            .into_iter()
            .filter_map(|row| {
                let role = macworp_core::entities::Role::from_db(row.role)?;
                let principal = match Principal::from_db(row.principal_type, row.principal_id)? {
                    Principal::User(id) => PrincipalResponse::User {
                        id,
                        username: users.get(&id).cloned().unwrap_or_default(),
                    },
                    Principal::Group(id) => PrincipalResponse::Group {
                        id,
                        name: groups.get(&id).cloned().unwrap_or_default(),
                    },
                    Principal::Public => PrincipalResponse::Public,
                };
                Some(ShareResponse { principal, role })
            })
            .collect())
    }

    /// Checks, after the resource check, that `user` may share with `principal`: the user or group must exist, and a group
    /// must be one the user owns or belongs to (instance admins may use any group).
    async fn check_principal(
        state: &AppState,
        user: &User,
        principal: PrincipalRequest,
    ) -> Result<Principal, Error> {
        match principal {
            PrincipalRequest::Public => Ok(Principal::Public),
            PrincipalRequest::User(id) => {
                UserTable::find_by_id(id)
                    .one(state.db())
                    .await?
                    .ok_or(Error::UnknownPrincipal)?;
                Ok(Principal::User(id))
            }
            PrincipalRequest::Group(id) => {
                let group = GroupTable::find_by_id(id)
                    .one(state.db())
                    .await?
                    .ok_or(Error::UnknownPrincipal)?;
                if !user.is_admin && !group.is_visible_to(state.db(), user.id).await? {
                    return Err(Error::UnknownPrincipal);
                }
                Ok(Principal::Group(id))
            }
        }
    }

    async fn list(
        State(state): State<Arc<AppState>>,
        RequireUser(user): RequireUser,
        Path((kind, id)): Path<(String, i64)>,
    ) -> Result<Json<Vec<ShareResponse>>, Error> {
        let shares = match Kind::parse(&kind)? {
            Kind::Project => Self::list_of::<Project>(&state, user, id).await?,
            Kind::Workflow => Self::list_of::<WorkflowIntegration>(&state, user, id).await?,
        };
        Ok(Json(shares))
    }

    async fn grant(
        State(state): State<Arc<AppState>>,
        RequireUser(user): RequireUser,
        Path((kind, id)): Path<(String, i64)>,
        Json(payload): Json<GrantShareRequest>,
    ) -> Result<StatusCode, Error> {
        match Kind::parse(&kind)? {
            Kind::Project => {
                Self::grant_on::<Project>(&state, user, id, payload.principal, payload.role).await?
            }
            Kind::Workflow => {
                Self::grant_on::<WorkflowIntegration>(
                    &state,
                    user,
                    id,
                    payload.principal,
                    payload.role,
                )
                .await?
            }
        }
        Ok(StatusCode::OK)
    }

    /// Grants `role` on resource `R` to `principal`. The caller's Owner right on the resource is
    /// checked first, so a caller without access never learns whether a principal exists.
    async fn grant_on<R: Resource + Send>(
        state: &AppState,
        user: User,
        id: i64,
        principal: PrincipalRequest,
        role: macworp_core::entities::Role,
    ) -> Result<(), Error>
    where
        <R::Entity as EntityTrait>::Model: Send,
    {
        authorize::<R, Owner>(state, user.clone(), id).await?;
        let principal = Self::check_principal(state, &user, principal).await?;
        let transaction = state.db().begin().await?;
        R::shares(&transaction, id).grant(principal, role).await?;
        transaction.commit().await?;
        Ok(())
    }

    async fn revoke(
        State(state): State<Arc<AppState>>,
        RequireUser(user): RequireUser,
        Path((kind, id)): Path<(String, i64)>,
        Json(payload): Json<RevokeShareRequest>,
    ) -> Result<StatusCode, Error> {
        let principal = match payload.principal {
            PrincipalRequest::User(id) => Principal::User(id),
            PrincipalRequest::Group(id) => Principal::Group(id),
            PrincipalRequest::Public => Principal::Public,
        };
        match Kind::parse(&kind)? {
            Kind::Project => {
                authorize::<Project, Owner>(&state, user, id).await?;
                let transaction = state.db().begin().await?;
                Project::shares(&transaction, id).revoke(principal).await?;
                transaction.commit().await?;
            }
            Kind::Workflow => {
                authorize::<WorkflowIntegration, Owner>(&state, user, id).await?;
                let transaction = state.db().begin().await?;
                WorkflowIntegration::shares(&transaction, id)
                    .revoke(principal)
                    .await?;
                transaction.commit().await?;
            }
        }
        Ok(StatusCode::OK)
    }
}
