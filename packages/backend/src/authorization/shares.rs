use std::marker::PhantomData;

use macworp_core::entities::Role;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, Condition, ConnectionTrait, DbErr, EntityTrait, PaginatorTrait,
    QueryFilter, QuerySelect, Set,
};

use super::{PrincipalType, Resource, query::applicable, role::RoleDb};
use crate::models::share;

/// Who a share is granted to
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Principal {
    /// A single user, by id
    User(i64),
    /// All members of a group, by id
    Group(i64),
    /// Every user
    Public,
}

impl Principal {
    /// Stored form: type and, except for the public, the principal's id
    fn parts(self) -> (PrincipalType, Option<i64>) {
        match self {
            Self::User(id) => (PrincipalType::User, Some(id)),
            Self::Group(id) => (PrincipalType::Group, Some(id)),
            Self::Public => (PrincipalType::Public, None),
        }
    }

    /// Filter matching the shares granted to exactly this principal; the public has a `NULL`
    /// principal id.
    fn condition(self) -> Condition {
        let (kind, id) = self.parts();
        let cond = Condition::all().add(share::Column::PrincipalType.eq(kind.to_db()));
        match id {
            Some(id) => cond.add(share::Column::PrincipalId.eq(id)),
            None => cond.add(share::Column::PrincipalId.is_null()),
        }
    }
}

/// Why changing a share failed
#[derive(Debug, thiserror::Error)]
pub enum ShareError {
    /// Database failure
    #[error("database error: {0}")]
    Database(#[from] DbErr),
    /// The resource type caps what the public may hold, see [`Resource::MAX_PUBLIC_ROLE`]
    #[error("the public may hold at most the {max:?} role on this resource")]
    PublicRoleTooHigh {
        /// Highest role allowed for the public
        max: Role,
    },
    /// The change would leave the resource without a user or group owner
    #[error("a resource needs at least one user or group owner")]
    LastOwner,
}

/// The shares of one resource: who holds which role on it. Get it with [`Resource::shares`].
///
/// Run changes inside a transaction when the resource is created or deleted in the same request.
pub struct Shares<'a, R, C> {
    /// Connection or transaction used for all queries
    db: &'a C,
    /// Id of the shared project or workflow
    resource_id: i64,
    /// The kind of resource, decides the `resource_type` and the public role cap
    resource: PhantomData<fn() -> R>,
}

impl<'a, R: Resource, C: ConnectionTrait> Shares<'a, R, C> {
    /// Shares of the resource with the given id.
    pub fn of(db: &'a C, resource_id: i64) -> Self {
        Self {
            db,
            resource_id,
            resource: PhantomData,
        }
    }

    /// Highest role `user_id` holds on the resource, `None` if it is not visible to them.
    pub async fn role_of(&self, user_id: i64) -> Result<Option<Role>, DbErr> {
        let roles = share::Entity::find()
            .filter(applicable(R::RESOURCE_TYPE, user_id))
            .filter(share::Column::ResourceId.eq(self.resource_id))
            .select_only()
            .column(share::Column::Role)
            .into_tuple::<i16>()
            .all(self.db)
            .await?;
        Ok(roles.into_iter().filter_map(Role::from_db).max())
    }

    /// Sets the role of `principal` on the resource, replacing an existing grant.
    ///
    /// The last-owner check is not safe against concurrent demotions of two different owners.
    pub async fn grant(&self, principal: Principal, role: Role) -> Result<(), ShareError> {
        if principal == Principal::Public && role > R::MAX_PUBLIC_ROLE {
            return Err(ShareError::PublicRoleTooHigh {
                max: R::MAX_PUBLIC_ROLE,
            });
        }
        match self.find(principal).await? {
            Some(existing) => {
                if role < Role::Owner && self.is_last_owner(&existing).await? {
                    return Err(ShareError::LastOwner);
                }
                let mut active: share::ActiveModel = existing.into();
                active.role = Set(role.to_db());
                active.update(self.db).await?;
            }
            None => {
                let (kind, id) = principal.parts();
                share::ActiveModel {
                    principal_type: Set(kind.to_db()),
                    principal_id: Set(id),
                    resource_type: Set(R::RESOURCE_TYPE.to_db()),
                    resource_id: Set(self.resource_id),
                    role: Set(role.to_db()),
                    ..Default::default()
                }
                .insert(self.db)
                .await?;
            }
        }
        Ok(())
    }

    /// Removes the grant of `principal`, if any.
    pub async fn revoke(&self, principal: Principal) -> Result<(), ShareError> {
        if let Some(existing) = self.find(principal).await? {
            if self.is_last_owner(&existing).await? {
                return Err(ShareError::LastOwner);
            }
            share::Entity::delete_by_id(existing.id)
                .exec(self.db)
                .await?;
        }
        Ok(())
    }

    /// Removes every grant on the resource, to be called when the resource is deleted (shares
    /// have no foreign key to it).
    pub async fn delete_all(&self) -> Result<(), DbErr> {
        share::Entity::delete_many()
            .filter(self.on_resource())
            .exec(self.db)
            .await?;
        Ok(())
    }

    /// Filter matching all shares of this resource.
    fn on_resource(&self) -> Condition {
        Condition::all()
            .add(share::Column::ResourceType.eq(R::RESOURCE_TYPE.to_db()))
            .add(share::Column::ResourceId.eq(self.resource_id))
    }

    /// The share of `principal` on the resource, if any.
    async fn find(&self, principal: Principal) -> Result<Option<share::Model>, DbErr> {
        share::Entity::find()
            .filter(self.on_resource())
            .filter(principal.condition())
            .one(self.db)
            .await
    }

    /// Whether `existing` is the only user/group owner of the resource.
    async fn is_last_owner(&self, existing: &share::Model) -> Result<bool, DbErr> {
        if existing.role != Role::Owner.to_db()
            || existing.principal_type == PrincipalType::Public.to_db()
        {
            return Ok(false);
        }
        let owners = share::Entity::find()
            .filter(self.on_resource())
            .filter(share::Column::Role.eq(Role::Owner.to_db()))
            .filter(share::Column::PrincipalType.ne(PrincipalType::Public.to_db()))
            .count(self.db)
            .await?;
        Ok(owners <= 1)
    }
}
