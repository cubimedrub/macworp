use macworp_core::entities::Role;
use sea_orm::{
    ColumnTrait, ConnectionTrait, DbErr, EntityTrait, QueryFilter, QueryOrder, sea_query::Expr,
};

use super::{PrincipalType, Resource, Shares, role::RoleDb};
use crate::models::{share, user};

/// What [`Shares::settle`] did to a resource.
pub enum Settled<M> {
    /// The resource does not exist (any more)
    Missing,
    /// Another user or group still owns the resource
    Owned,
    /// Nobody owns the resource any more but others can still use it: its creator is cleared and
    /// an administrator may assign a new owner
    Ownerless,
    /// Nobody holds a role that keeps the resource alive: its shares and the row are gone, the
    /// removed row is returned so the caller can clean up what lives outside the database
    Deleted(M),
}

impl<R: Resource, C: ConnectionTrait> Shares<'_, R, C> {
    /// Applies the orphan rules to the resource after a user's shares were removed.
    ///
    /// - A user or group still holds Owner: the resource stays. If `departing_user` is shown as
    ///   its creator, that becomes the enabled user Owner with the oldest share (`NULL` if only
    ///   groups own it).
    /// - Else, if somebody holds at least [`Resource::MIN_ROLE_WITHOUT_OWNER`] (the public never
    ///   counts as Owner, a public Owner counts as Editor): the creator is cleared, the resource
    ///   is ownerless.
    /// - Else the resource and its shares are deleted.
    ///
    /// Idempotent, so it can be repeated after further shares disappeared.
    pub async fn settle(
        &self,
        departing_user: i64,
    ) -> Result<Settled<<R::Entity as EntityTrait>::Model>, DbErr> {
        let Some(model) = R::Entity::find()
            .filter(R::id_column().eq(self.resource_id))
            .one(self.db)
            .await?
        else {
            return Ok(Settled::Missing);
        };

        let shares = self.list().await?;
        let is_public =
            |share: &share::Model| share.principal_type == PrincipalType::Public.to_db();
        let owners = shares
            .iter()
            .filter(|share| share.role == Role::Owner.to_db() && !is_public(share));
        if owners.clone().next().is_some() {
            let successor = self
                .oldest_enabled_user(owners.filter_map(|s| {
                    (s.principal_type == PrincipalType::User.to_db())
                        .then_some(s.principal_id)
                        .flatten()
                }))
                .await?;
            R::Entity::update_many()
                .col_expr(R::owner_id_column(), Expr::value(successor))
                .filter(R::id_column().eq(self.resource_id))
                .filter(R::owner_id_column().eq(departing_user))
                .exec(self.db)
                .await?;
            return Ok(Settled::Owned);
        }

        // Without a concrete owner a public Owner share only grants what an Editor could do.
        let kept = shares.iter().any(|share| {
            let role = if is_public(share) && share.role == Role::Owner.to_db() {
                Role::Editor.to_db()
            } else {
                share.role
            };
            role >= R::MIN_ROLE_WITHOUT_OWNER.to_db()
        });
        if kept {
            R::Entity::update_many()
                .col_expr(R::owner_id_column(), Expr::value(Option::<i64>::None))
                .filter(R::id_column().eq(self.resource_id))
                .exec(self.db)
                .await?;
            return Ok(Settled::Ownerless);
        }

        self.delete_all().await?;
        R::Entity::delete_many()
            .filter(R::id_column().eq(self.resource_id))
            .exec(self.db)
            .await?;
        Ok(Settled::Deleted(model))
    }

    /// The first of `user_ids` (in the given order) that exists and is not disabled.
    async fn oldest_enabled_user(
        &self,
        user_ids: impl Iterator<Item = i64>,
    ) -> Result<Option<i64>, DbErr> {
        let user_ids: Vec<i64> = user_ids.collect();
        let enabled: Vec<i64> = user::Entity::find()
            .filter(user::Column::Id.is_in(user_ids.clone()))
            .filter(user::Column::Disabled.eq(false))
            .order_by_asc(user::Column::Id)
            .all(self.db)
            .await?
            .into_iter()
            .map(|user| user.id)
            .collect();
        Ok(user_ids.into_iter().find(|id| enabled.contains(id)))
    }
}
