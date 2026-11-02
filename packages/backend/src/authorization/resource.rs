use sea_orm::{Condition, ConnectionTrait, EntityTrait};

use macworp_core::entities::Role;

use super::{ResourceType, Shares, query::visible_to};

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
}
