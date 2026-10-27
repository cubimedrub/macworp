use sea_orm::{
    ColumnTrait, Condition, ExprTrait,
    sea_query::{Expr, Query, SelectStatement},
};

use macworp_core::entities::Role;

use super::{PrincipalType, Resource, ResourceType, role::RoleDb};
use crate::models::{share, users_groups};

/// IDs of the groups `user_id` belongs to.
fn groups_of(user_id: i64) -> SelectStatement {
    Query::select()
        .column(users_groups::Column::GroupId)
        .from(users_groups::Entity)
        .and_where(Expr::col(users_groups::Column::UserId).eq(user_id))
        .to_owned()
}

/// Shares of one resource type that apply to `user_id` (granted to them, their groups or the public).
pub(super) fn applicable(resource_type: ResourceType, user_id: i64) -> Condition {
    Condition::all()
        .add(share::Column::ResourceType.eq(resource_type.to_db()))
        .add(
            Condition::any()
                .add(
                    Condition::all()
                        .add(share::Column::PrincipalType.eq(PrincipalType::User.to_db()))
                        .add(share::Column::PrincipalId.eq(user_id)),
                )
                .add(
                    Condition::all()
                        .add(share::Column::PrincipalType.eq(PrincipalType::Group.to_db()))
                        .add(share::Column::PrincipalId.in_subquery(groups_of(user_id))),
                )
                .add(share::Column::PrincipalType.eq(PrincipalType::Public.to_db())),
        )
}

/// Condition for a query on `R::Entity` restricting it to rows where `user_id` holds at least `min`.
pub(super) fn visible_to<R: Resource>(user_id: i64, min: Role) -> Condition {
    let shared = Query::select()
        .column(share::Column::ResourceId)
        .from(share::Entity)
        .cond_where(applicable(R::RESOURCE_TYPE, user_id))
        .and_where(Expr::col(share::Column::Role).gte(min.to_db()))
        .to_owned();
    Condition::all().add(R::id_column().in_subquery(shared))
}
