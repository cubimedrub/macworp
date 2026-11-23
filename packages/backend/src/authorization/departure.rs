use std::collections::BTreeSet;

use sea_orm::{
    ColumnTrait, ConnectionTrait, DbErr, EntityTrait, PaginatorTrait, QueryFilter, QuerySelect,
    sea_query::Expr,
};

use super::{Principal, Resource, ResourceType, Settled};
use crate::models::{GroupTable, Project, WorkflowIntegration, group, share, users_groups};

/// Everything a user's departure changed, for audit logs and cleanup outside the database.
#[derive(Debug, Default)]
pub struct Departed {
    /// Projects deleted because nobody could maintain them any more; their files are still on disk
    pub deleted_projects: Vec<Project>,
    /// Projects that lost their last owner but still have editors
    pub ownerless_projects: Vec<i64>,
    /// Workflows deleted because nobody could use them any more
    pub deleted_workflows: Vec<WorkflowIntegration>,
    /// Workflows that lost their last owner but still have viewers or editors
    pub ownerless_workflows: Vec<i64>,
    /// Groups deleted because they are unused or have no members left
    pub deleted_groups: Vec<i64>,
    /// Groups the user owned that are still used by shares and have members
    pub ownerless_groups: Vec<i64>,
}

/// Removes a user from everything they took part in and decides what becomes of what they owned.
///
/// Run it in a transaction right before deleting the user row. Rules, per resource the user held a
/// share on or created, after their shares and memberships are gone (see [`super::Shares::settle`]
/// for resources):
/// - another user or group owns it: nothing changes besides the creator shown;
/// - projects without an owner stay as ownerless while an editor remains, workflows while a viewer
///   or editor remains; otherwise they are deleted;
/// - groups the user owned or belonged to are deleted if no member is left, or if they are
///   ownerless and no share names them; used groups with members become ownerless.
///
/// Deleting groups removes their shares, which can orphan further resources, so the steps repeat
/// until nothing changes.
///
/// The rules are also written for users in `macworp_core::account_deletion::RULES` and the
/// README section "Deleting an account": keep them in sync.
pub struct Departure<'a, C> {
    /// Connection or transaction used for all queries
    db: &'a C,
    /// The user who leaves
    user_id: i64,
}

impl<'a, C: ConnectionTrait> Departure<'a, C> {
    /// The departure of the user with the given id.
    pub fn of(db: &'a C, user_id: i64) -> Self {
        Self { db, user_id }
    }

    /// Removes the user's shares and memberships and settles everything they leave behind.
    /// Does not delete the user row.
    pub async fn settle(&self) -> Result<Departed, DbErr> {
        let mut projects = self.touched_resources::<Project>().await?;
        let mut workflows = self.touched_resources::<WorkflowIntegration>().await?;
        let mut groups = self.touched_groups().await?;

        // Serialize with concurrent share changes on these resources (ids are sorted, so two
        // departures lock in the same order).
        for id in &projects {
            Project::shares(self.db, *id).lock().await?;
        }
        for id in &workflows {
            WorkflowIntegration::shares(self.db, *id).lock().await?;
        }

        Principal::delete_user_shares(self.db, self.user_id).await?;
        users_groups::Entity::delete_many()
            .filter(users_groups::Column::UserId.eq(self.user_id))
            .exec(self.db)
            .await?;
        // The user's groups may be shared with others, so the creator link goes now too.
        GroupTable::update_many()
            .col_expr(group::Column::OwnerId, Expr::value(Option::<i64>::None))
            .filter(group::Column::OwnerId.eq(self.user_id))
            .exec(self.db)
            .await?;

        let mut departed = Departed::default();
        loop {
            for id in &projects {
                match Project::shares(self.db, *id).settle(self.user_id).await? {
                    Settled::Deleted(project) => departed.deleted_projects.push(project),
                    Settled::Ownerless => {
                        if !departed.ownerless_projects.contains(id) {
                            departed.ownerless_projects.push(*id);
                        }
                    }
                    Settled::Owned | Settled::Missing => {}
                }
            }
            for id in &workflows {
                match WorkflowIntegration::shares(self.db, *id)
                    .settle(self.user_id)
                    .await?
                {
                    Settled::Deleted(workflow) => departed.deleted_workflows.push(workflow),
                    Settled::Ownerless => {
                        if !departed.ownerless_workflows.contains(id) {
                            departed.ownerless_workflows.push(*id);
                        }
                    }
                    Settled::Owned | Settled::Missing => {}
                }
            }

            // Deleting a group removes shares: the resources it was shared on need another pass.
            let mut deleted_a_group = false;
            for id in groups.clone() {
                if !self.is_abandoned(id).await? {
                    continue;
                }
                projects.extend(self.shared_on(id, ResourceType::Project).await?);
                workflows.extend(self.shared_on(id, ResourceType::Workflow).await?);
                Principal::delete_group_shares(self.db, id).await?;
                GroupTable::delete_by_id(id).exec(self.db).await?;
                groups.remove(&id);
                departed.deleted_groups.push(id);
                deleted_a_group = true;
            }
            if !deleted_a_group {
                break;
            }
        }

        for id in groups {
            if GroupTable::find_by_id(id).one(self.db).await?.is_some() {
                departed.ownerless_groups.push(id);
            }
        }
        departed
            .ownerless_projects
            .retain(|id| !projects_deleted(&departed.deleted_projects, *id));
        departed
            .ownerless_workflows
            .retain(|id| !workflows_deleted(&departed.deleted_workflows, *id));
        Ok(departed)
    }

    /// Ids of resources of type `R` the user holds a share on or is shown as creator of.
    async fn touched_resources<R: Resource>(&self) -> Result<BTreeSet<i64>, DbErr> {
        let shared = share::Entity::find()
            .filter(Principal::User(self.user_id).condition())
            .filter(share::Column::ResourceType.eq(R::RESOURCE_TYPE.to_db()))
            .select_only()
            .column(share::Column::ResourceId)
            .into_tuple::<i64>()
            .all(self.db)
            .await?;
        let created = R::Entity::find()
            .filter(R::owner_id_column().eq(self.user_id))
            .select_only()
            .column(R::id_column())
            .into_tuple::<i64>()
            .all(self.db)
            .await?;
        Ok(shared.into_iter().chain(created).collect())
    }

    /// Ids of groups the user owns or belongs to.
    async fn touched_groups(&self) -> Result<BTreeSet<i64>, DbErr> {
        let owned = GroupTable::find()
            .filter(group::Column::OwnerId.eq(self.user_id))
            .select_only()
            .column(group::Column::Id)
            .into_tuple::<i64>()
            .all(self.db)
            .await?;
        let member_of = users_groups::Entity::find()
            .filter(users_groups::Column::UserId.eq(self.user_id))
            .select_only()
            .column(users_groups::Column::GroupId)
            .into_tuple::<i64>()
            .all(self.db)
            .await?;
        Ok(owned.into_iter().chain(member_of).collect())
    }

    /// Whether the group has to go: no member is left, or nobody owns it and no share names it.
    async fn is_abandoned(&self, group_id: i64) -> Result<bool, DbErr> {
        let Some(group) = GroupTable::find_by_id(group_id).one(self.db).await? else {
            return Ok(false);
        };
        if group.owner_id.is_some() {
            // A live owner keeps their group, even a still empty one.
            return Ok(false);
        }
        let members = users_groups::Entity::find()
            .filter(users_groups::Column::GroupId.eq(group_id))
            .count(self.db)
            .await?;
        let used = share::Entity::find()
            .filter(Principal::Group(group_id).condition())
            .count(self.db)
            .await?
            > 0;
        Ok(members == 0 || !used)
    }

    /// Ids of resources of the given type the group holds a share on.
    async fn shared_on(
        &self,
        group_id: i64,
        resource_type: ResourceType,
    ) -> Result<Vec<i64>, DbErr> {
        share::Entity::find()
            .filter(Principal::Group(group_id).condition())
            .filter(share::Column::ResourceType.eq(resource_type.to_db()))
            .select_only()
            .column(share::Column::ResourceId)
            .into_tuple::<i64>()
            .all(self.db)
            .await
    }
}

/// Whether `id` is among the deleted projects.
fn projects_deleted(deleted: &[Project], id: i64) -> bool {
    deleted.iter().any(|project| project.id == id)
}

/// Whether `id` is among the deleted workflows.
fn workflows_deleted(deleted: &[WorkflowIntegration], id: i64) -> bool {
    deleted.iter().any(|workflow| workflow.id == id)
}
