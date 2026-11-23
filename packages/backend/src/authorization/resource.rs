use std::collections::HashSet;

use sea_orm::{
    ColumnTrait, Condition, ConnectionTrait, DbErr, EntityTrait, QueryFilter, QuerySelect,
};

use macworp_core::entities::Role;

use super::{PrincipalType, ResourceType, Shares, query::visible_to, role::RoleDb};
use crate::models::share;

/// A table whose rows can be shared.
pub trait Resource {
    /// Table of the resource
    type Entity: EntityTrait;

    /// Stored in `shares.resource_type`
    const RESOURCE_TYPE: ResourceType;
    /// Highest role the public principal may hold.
    const MAX_PUBLIC_ROLE: Role;

    /// Lowest role that keeps a resource alive when its last owner is gone: the resource becomes
    /// ownerless if somebody (user, group or the public; a public owner counts as editor) still
    /// holds at least this role, and is deleted otherwise.
    const MIN_ROLE_WITHOUT_OWNER: Role;

    /// Primary key column, matched against `shares.resource_id`.
    fn id_column() -> <Self::Entity as EntityTrait>::Column;

    /// Column with the creator, `NULL` while the resource has no creator to show.
    fn owner_id_column() -> <Self::Entity as EntityTrait>::Column;

    /// The shares of the resource with the given id.
    fn shares<C: ConnectionTrait>(db: &C, id: i64) -> Shares<'_, Self, C>
    where
        Self: Sized,
    {
        Shares::of(db, id)
    }

    /// Condition for a query on [`Self::Entity`] restricting it to rows where `user_id` holds at
    /// least the role `min`.
    fn visible_to(user_id: i64, min: Role) -> Condition
    where
        Self: Sized,
    {
        visible_to::<Self>(user_id, min)
    }

    /// The ids among `ids` that have no user or group Owner share. A public Owner does not count,
    /// matching the owner-orphan rule of [`Shares`]. One query for all ids.
    fn ownerless_among<'a, C: ConnectionTrait>(
        db: &'a C,
        ids: impl IntoIterator<Item = i64>,
    ) -> impl Future<Output = Result<HashSet<i64>, DbErr>> + Send + 'a
    where
        Self: Sized,
    {
        let mut ownerless: HashSet<i64> = ids.into_iter().collect();
        async move {
            if ownerless.is_empty() {
                return Ok(ownerless);
            }
            let owned = share::Entity::find()
                .filter(share::Column::ResourceType.eq(Self::RESOURCE_TYPE.to_db()))
                .filter(share::Column::ResourceId.is_in(ownerless.iter().copied()))
                .filter(share::Column::Role.eq(Role::Owner.to_db()))
                .filter(
                    share::Column::PrincipalType
                        .is_in([PrincipalType::User.to_db(), PrincipalType::Group.to_db()]),
                )
                .select_only()
                .column(share::Column::ResourceId)
                .into_tuple::<i64>()
                .all(db)
                .await?;
            for id in owned {
                ownerless.remove(&id);
            }
            Ok(ownerless)
        }
    }
}
