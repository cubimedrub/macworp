use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
};
use macworp_core::{
    requests::{GroupMemberRequest, NewGroupRequest, UpdateGroupRequest},
    responses::{GroupResponse, UserSummaryResponse},
    routing::GroupsRouting,
};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, ModelTrait, QueryFilter,
    QueryOrder, TransactionTrait,
};
use thiserror::Error;

use crate::{
    authentication::middleware::{ListScope, RequireAuth},
    authorization::Principal,
    models::{
        ActiveGroup, ActiveUsersGroups, Group, GroupTable, User, UserTable, UsersGroupsTable,
    },
    state::State as AppState,
};

/// Why a groups request failed
#[derive(Error, Debug)]
pub enum Error {
    /// Database failure, 500
    #[error("{0}")]
    Database(#[from] sea_orm::DbErr),
    /// Missing or invalid credentials
    #[error("{0}")]
    Authentication(#[from] crate::authentication::error::Error),
    /// No such group, or the user is neither its owner nor a member, 404
    #[error("Not found")]
    NotFound,
    /// The user may see the group but only its owner may change it, 403
    #[error("Only the owner may change the group")]
    NotOwner,
    /// The user to add does not exist, 404
    #[error("No such user")]
    UnknownUser,
    /// The group name is empty, 400
    #[error("The group name must not be empty")]
    EmptyName,
    /// The owner cannot be removed from their own group, 409
    #[error("The owner cannot be removed from the group")]
    RemoveOwner,
}

impl IntoResponse for Error {
    fn into_response(self) -> axum::response::Response {
        match self {
            Self::Authentication(e) => e.into_response(),
            Self::NotFound | Self::UnknownUser => {
                (StatusCode::NOT_FOUND, self.to_string()).into_response()
            }
            Self::NotOwner => (StatusCode::FORBIDDEN, self.to_string()).into_response(),
            Self::EmptyName => (StatusCode::BAD_REQUEST, self.to_string()).into_response(),
            Self::RemoveOwner => (StatusCode::CONFLICT, self.to_string()).into_response(),
            Self::Database(_) => {
                tracing::error!("{self}");
                StatusCode::INTERNAL_SERVER_ERROR.into_response()
            }
        }
    }
}

/// Endpoints to manage groups of users, which projects and workflows can be shared with
pub struct GroupsController;

impl GroupsController {
    pub fn routes(state: Arc<AppState>) -> Router<Arc<AppState>> {
        Router::new()
            .route(GroupsRouting::INDEX_PATH, get(Self::index))
            .route(GroupsRouting::NEW_PATH, post(Self::new))
            .route(GroupsRouting::EDIT_PATH, get(Self::edit))
            .route(GroupsRouting::UPDATE_PATH, post(Self::update))
            .route(GroupsRouting::DELETE_PATH, post(Self::delete))
            .route(GroupsRouting::ADD_MEMBER_PATH, post(Self::add_member))
            .route(GroupsRouting::REMOVE_MEMBER_PATH, post(Self::remove_member))
            .with_state(state)
    }

    /// Loads a group the user owns or is a member of; others get [`Error::NotFound`]. Instance
    /// admins can load every group.
    async fn visible(state: &AppState, user: &User, id: i64) -> Result<Group, Error> {
        let mut query = GroupTable::find_by_id(id);
        if !user.is_admin {
            query = query.filter(Group::visible_to(user.id));
        }
        query.one(state.db()).await?.ok_or(Error::NotFound)
    }

    /// Like [`Self::visible`] but only for the owner of the group, or an instance admin (logged
    /// with target `audit` when the admin is not the owner).
    async fn owned(state: &AppState, user: &User, id: i64) -> Result<Group, Error> {
        let group = Self::visible(state, user, id).await?;
        if group.owner_id != Some(user.id) {
            if !user.is_admin {
                return Err(Error::NotOwner);
            }
            tracing::info!(
                target: "audit",
                admin = %user.username,
                group = group.id,
                "admin changes a group they do not own"
            );
        }
        Ok(group)
    }

    /// The response for `group`, members only if `with_members`.
    async fn response(
        state: &AppState,
        user: &User,
        group: Group,
        with_members: bool,
    ) -> Result<GroupResponse, Error> {
        let owner = User::usernames(state.db(), group.owner_id).await?;
        let members = if with_members {
            group
                .member_names(state.db())
                .await?
                .into_iter()
                .map(|(id, username)| UserSummaryResponse { id, username })
                .collect()
        } else {
            Vec::new()
        };
        Ok(GroupResponse {
            id: group.id,
            name: group.name,
            description: group.description,
            owner_username: group
                .owner_id
                .and_then(|id| owner.get(&id).cloned())
                .unwrap_or_default(),
            ownerless: group.owner_id.is_none(),
            is_owner: group.owner_id == Some(user.id) || user.is_admin,
            members,
        })
    }

    /// Lists the groups the user owns or belongs to, with `?all=true` (administrators only) every
    /// group. Members are not loaded.
    async fn index(
        State(state): State<Arc<AppState>>,
        scope: ListScope,
    ) -> Result<Json<Vec<GroupResponse>>, Error> {
        let user = &scope.user;
        let groups = GroupTable::find()
            .filter(scope.filter(Group::visible_to(user.id)))
            .order_by_asc(crate::models::group::Column::Name)
            .all(state.db())
            .await?;
        let mut responses = Vec::with_capacity(groups.len());
        for group in groups {
            responses.push(Self::response(&state, user, group, false).await?);
        }
        Ok(Json(responses))
    }

    #[allow(clippy::new_ret_no_self)]
    async fn new(
        State(state): State<Arc<AppState>>,
        RequireAuth(user): RequireAuth,
        Json(payload): Json<NewGroupRequest>,
    ) -> Result<Json<GroupResponse>, Error> {
        if payload.name.trim().is_empty() {
            return Err(Error::EmptyName);
        }
        // The owner is a member from the start, so shares granted to the group reach them too.
        let transaction = state.db().begin().await?;
        let group = ActiveGroup {
            owner_id: Set(Some(user.id)),
            name: Set(payload.name.trim().to_string()),
            description: Set(payload.description),
            ..Default::default()
        }
        .insert(&transaction)
        .await?;
        ActiveUsersGroups {
            user_id: Set(user.id),
            group_id: Set(group.id),
        }
        .insert(&transaction)
        .await?;
        transaction.commit().await?;
        Ok(Json(Self::response(&state, &user, group, true).await?))
    }

    async fn edit(
        State(state): State<Arc<AppState>>,
        RequireAuth(user): RequireAuth,
        Path(id): Path<i64>,
    ) -> Result<Json<GroupResponse>, Error> {
        let group = Self::visible(&state, &user, id).await?;
        Ok(Json(Self::response(&state, &user, group, true).await?))
    }

    async fn update(
        State(state): State<Arc<AppState>>,
        RequireAuth(user): RequireAuth,
        Path(id): Path<i64>,
        Json(payload): Json<UpdateGroupRequest>,
    ) -> Result<Json<GroupResponse>, Error> {
        if payload.name.trim().is_empty() {
            return Err(Error::EmptyName);
        }
        let group = Self::owned(&state, &user, id).await?;
        let mut active: ActiveGroup = group.into();
        active.name = Set(payload.name.trim().to_string());
        active.description = Set(payload.description);
        let group = active.update(state.db()).await?;
        Ok(Json(Self::response(&state, &user, group, true).await?))
    }

    async fn delete(
        State(state): State<Arc<AppState>>,
        RequireAuth(user): RequireAuth,
        Path(id): Path<i64>,
    ) -> Result<StatusCode, Error> {
        let group = Self::owned(&state, &user, id).await?;
        let txn = state.db().begin().await?;
        Principal::delete_group_shares(&txn, group.id).await?;
        group.delete(&txn).await?;
        txn.commit().await?;
        Ok(StatusCode::OK)
    }

    async fn add_member(
        State(state): State<Arc<AppState>>,
        RequireAuth(user): RequireAuth,
        Path(id): Path<i64>,
        Json(payload): Json<GroupMemberRequest>,
    ) -> Result<Json<GroupResponse>, Error> {
        let group = Self::owned(&state, &user, id).await?;
        UserTable::find_by_id(payload.user_id)
            .one(state.db())
            .await?
            .ok_or(Error::UnknownUser)?;
        let already = UsersGroupsTable::find_by_id((payload.user_id, group.id))
            .one(state.db())
            .await?
            .is_some();
        if !already {
            ActiveUsersGroups {
                user_id: Set(payload.user_id),
                group_id: Set(group.id),
            }
            .insert(state.db())
            .await?;
        }
        Ok(Json(Self::response(&state, &user, group, true).await?))
    }

    async fn remove_member(
        State(state): State<Arc<AppState>>,
        RequireAuth(user): RequireAuth,
        Path(id): Path<i64>,
        Json(payload): Json<GroupMemberRequest>,
    ) -> Result<Json<GroupResponse>, Error> {
        let group = Self::owned(&state, &user, id).await?;
        if Some(payload.user_id) == group.owner_id {
            return Err(Error::RemoveOwner);
        }
        UsersGroupsTable::delete_many()
            .filter(crate::models::users_groups::Column::GroupId.eq(group.id))
            .filter(crate::models::users_groups::Column::UserId.eq(payload.user_id))
            .exec(state.db())
            .await?;
        Ok(Json(Self::response(&state, &user, group, true).await?))
    }
}
