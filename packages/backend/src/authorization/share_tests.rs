//! Only used when for integration tests
//!

use macworp_core::entities::Role;
use macworp_database_migration::{Migrator, MigratorTrait};
use sea_orm::{
    ColumnTrait, ConnectionTrait, Database, DatabaseConnection, EntityTrait, QueryFilter,
    QueryOrder, TransactionTrait,
};

use super::{Principal, Resource, ShareError, role::RoleDb};
use crate::models::{Project, ProjectColumn, ProjectTable, WorkflowIntegration, share};

/// Migrated in-memory database: users `owner`, `member` (in `group`) and `outsider`, and one project `thing` shared only with `owner`.
struct World {
    db: DatabaseConnection,
    owner: i64,
    member: i64,
    outsider: i64,
    group: i64,
    thing: i64,
}

/// Runs a statement, panicking on failure.
async fn exec(db: &DatabaseConnection, sql: &str) {
    db.execute_unprepared(sql).await.unwrap();
}

/// Creates the [`World`] fixtures.
async fn world() -> World {
    let db = Database::connect("sqlite::memory:").await.unwrap();
    Migrator::up(&db, None).await.unwrap();
    for (id, name) in [(1, "owner"), (2, "member"), (3, "outsider")] {
        exec(
            &db,
            &format!("INSERT INTO users (id, username, firstname, lastname, email) VALUES ({id}, '{name}', 'f', 'l', '{name}@x')"),
        )
        .await;
    }
    exec(
        &db,
        "INSERT INTO groups (id, owner_id, name) VALUES (1, 1, 'g')",
    )
    .await;
    exec(
        &db,
        "INSERT INTO users_groups (user_id, group_id) VALUES (2, 1)",
    )
    .await;
    exec(
        &db,
        "INSERT INTO projects (id, owner_id, name) VALUES (1, 1, 'p')",
    )
    .await;
    Project::shares(&db, 1)
        .grant(Principal::User(1), Role::Owner)
        .await
        .unwrap();
    World {
        db,
        owner: 1,
        member: 2,
        outsider: 3,
        group: 1,
        thing: 1,
    }
}

/// Ids of projects `user` holds at least `min` on, via [`Resource::visible_to`].
async fn visible(w: &World, user: i64, min: Role) -> Vec<i64> {
    ProjectTable::find()
        .filter(Project::visible_to(user, min))
        .order_by_asc(ProjectColumn::Id)
        .all(&w.db)
        .await
        .unwrap()
        .into_iter()
        .map(|t| t.id)
        .collect()
}

/// Without shares only the owner sees the project.
#[tokio::test]
async fn private_by_default() {
    let w = world().await;
    assert_eq!(
        Project::shares(&w.db, w.thing)
            .role_of(w.owner)
            .await
            .unwrap(),
        Some(Role::Owner)
    );
    assert_eq!(
        Project::shares(&w.db, w.thing)
            .role_of(w.outsider)
            .await
            .unwrap(),
        None
    );
    assert_eq!(visible(&w, w.owner, Role::Viewer).await, [w.thing]);
    assert!(visible(&w, w.outsider, Role::Viewer).await.is_empty());
}

/// A group share reaches its members and nobody else.
#[tokio::test]
async fn group_share_applies_to_members_only() {
    let w = world().await;
    Project::shares(&w.db, w.thing)
        .grant(Principal::Group(w.group), Role::Editor)
        .await
        .unwrap();
    assert_eq!(
        Project::shares(&w.db, w.thing)
            .role_of(w.member)
            .await
            .unwrap(),
        Some(Role::Editor)
    );
    assert_eq!(
        Project::shares(&w.db, w.thing)
            .role_of(w.outsider)
            .await
            .unwrap(),
        None
    );
    assert_eq!(visible(&w, w.member, Role::Editor).await, [w.thing]);
    assert!(visible(&w, w.member, Role::Owner).await.is_empty());
}

/// User, group and public shares combine to the highest role, the public one is capped by its own role.
#[tokio::test]
async fn effective_role_is_the_maximum() {
    let w = world().await;
    Project::shares(&w.db, w.thing)
        .grant(Principal::Group(w.group), Role::Viewer)
        .await
        .unwrap();
    Project::shares(&w.db, w.thing)
        .grant(Principal::User(w.member), Role::Editor)
        .await
        .unwrap();
    Project::shares(&w.db, w.thing)
        .grant(Principal::Public, Role::Viewer)
        .await
        .unwrap();
    assert_eq!(
        Project::shares(&w.db, w.thing)
            .role_of(w.member)
            .await
            .unwrap(),
        Some(Role::Editor)
    );
    // The public share lifts outsiders to viewer, not further.
    assert_eq!(
        Project::shares(&w.db, w.thing)
            .role_of(w.outsider)
            .await
            .unwrap(),
        Some(Role::Viewer)
    );
    assert!(visible(&w, w.outsider, Role::Editor).await.is_empty());
}

/// Granting twice to the same principal updates the share instead of adding a second row.
#[tokio::test]
async fn regrant_replaces_instead_of_duplicating() {
    let w = world().await;
    Project::shares(&w.db, w.thing)
        .grant(Principal::Public, Role::Viewer)
        .await
        .unwrap();
    Project::shares(&w.db, w.thing)
        .grant(Principal::Public, Role::Viewer)
        .await
        .unwrap();
    Project::shares(&w.db, w.thing)
        .grant(Principal::User(w.member), Role::Viewer)
        .await
        .unwrap();
    Project::shares(&w.db, w.thing)
        .grant(Principal::User(w.member), Role::Editor)
        .await
        .unwrap();
    let rows = share::Entity::find().all(&w.db).await.unwrap();
    assert_eq!(rows.len(), 3);
}

/// Projects may be public up to owner, workflows only up to viewer.
#[tokio::test]
async fn public_role_is_capped() {
    let w = world().await;
    // Projects may be public up to owner, workflows only up to viewer.
    Project::shares(&w.db, w.thing)
        .grant(Principal::Public, Role::Editor)
        .await
        .unwrap();
    let err = WorkflowIntegration::shares(&w.db, 1)
        .grant(Principal::Public, Role::Editor)
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        ShareError::PublicRoleTooHigh { max: Role::Viewer }
    ));
}

/// The last user or group owner cannot be demoted or removed, a second owner frees the first.
#[tokio::test]
async fn last_owner_is_protected() {
    let w = world().await;
    let demote = Project::shares(&w.db, w.thing)
        .grant(Principal::User(w.owner), Role::Editor)
        .await;
    assert!(matches!(demote, Err(ShareError::LastOwner)));
    let remove = Project::shares(&w.db, w.thing)
        .revoke(Principal::User(w.owner))
        .await;
    assert!(matches!(remove, Err(ShareError::LastOwner)));

    // A second owner (here a group) frees the first one.
    Project::shares(&w.db, w.thing)
        .grant(Principal::Group(w.group), Role::Owner)
        .await
        .unwrap();
    Project::shares(&w.db, w.thing)
        .revoke(Principal::User(w.owner))
        .await
        .unwrap();
    assert_eq!(
        Project::shares(&w.db, w.thing)
            .role_of(w.owner)
            .await
            .unwrap(),
        None
    );
    assert_eq!(
        Project::shares(&w.db, w.thing)
            .role_of(w.member)
            .await
            .unwrap(),
        Some(Role::Owner)
    );
}

/// A demotion waits for a concurrent transaction holding the resource lock and then hits the
/// last-owner guard (file-backed database so the transactions use separate connections).
#[tokio::test]
async fn concurrent_demotions_keep_one_owner() {
    let directory = tempfile::tempdir().unwrap();
    let db = Database::connect(format!(
        "sqlite://{}/race.db?mode=rwc",
        directory.path().display()
    ))
    .await
    .unwrap();
    Migrator::up(&db, None).await.unwrap();
    exec(
        &db,
        "INSERT INTO users (id, username, firstname, lastname, email) VALUES (1, 'a', 'f', 'l', 'a@x')",
    )
    .await;
    exec(
        &db,
        "INSERT INTO groups (id, owner_id, name) VALUES (1, 1, 'g')",
    )
    .await;
    exec(
        &db,
        "INSERT INTO projects (id, owner_id, name) VALUES (1, 1, 'p')",
    )
    .await;
    Project::shares(&db, 1)
        .grant(Principal::User(1), Role::Owner)
        .await
        .unwrap();
    Project::shares(&db, 1)
        .grant(Principal::Group(1), Role::Owner)
        .await
        .unwrap();

    /// Demotes `principal` to Viewer inside its own transaction.
    async fn demote(db: &DatabaseConnection, principal: Principal) -> Result<(), ShareError> {
        let transaction = db.begin().await?;
        Project::shares(&transaction, 1)
            .grant(principal, Role::Viewer)
            .await?;
        transaction.commit().await?;
        Ok(())
    }
    // The first transaction takes the lock; the second demotion must wait for its commit, so it
    // cannot count owners before the first one wrote.
    let first = db.begin().await.unwrap();
    Project::shares(&first, 1).lock().await.unwrap();
    let second_db = db.clone();
    let second = tokio::spawn(async move { demote(&second_db, Principal::Group(1)).await });
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    assert!(!second.is_finished(), "second demotion ran without waiting");
    Project::shares(&first, 1)
        .grant(Principal::User(1), Role::Viewer)
        .await
        .unwrap();
    first.commit().await.unwrap();
    assert!(matches!(second.await.unwrap(), Err(ShareError::LastOwner)));
    let owners = share::Entity::find()
        .filter(share::Column::Role.eq(Role::Owner.to_db()))
        .all(&db)
        .await
        .unwrap();
    assert_eq!(owners.len(), 1);
}

/// The migration turns creators into owners.
#[tokio::test]
async fn migration_backfills_existing_rows() {
    let db = Database::connect("sqlite::memory:").await.unwrap();
    // Everything before the groups/shares migration
    Migrator::up(&db, Some(4)).await.unwrap();
    exec(&db, "INSERT INTO users (id, username, firstname, lastname, email) VALUES (1, 'a', 'f', 'l', 'a@x'), (2, 'b', 'f', 'l', 'b@x')").await;
    exec(
        &db,
        "INSERT INTO projects (id, owner_id, name) VALUES (1, 1, 'p')",
    )
    .await;
    exec(&db, "INSERT INTO workflow_integrations (id, owner_id, name, description) VALUES (1, 1, 'w1', ''), (2, 1, 'w2', '')").await;
    Migrator::up(&db, None).await.unwrap();

    assert_eq!(
        Project::shares(&db, 1).role_of(1).await.unwrap(),
        Some(Role::Owner)
    );
    assert_eq!(Project::shares(&db, 1).role_of(2).await.unwrap(), None);
    assert_eq!(
        WorkflowIntegration::shares(&db, 2)
            .role_of(1)
            .await
            .unwrap(),
        Some(Role::Owner)
    );
    assert_eq!(
        WorkflowIntegration::shares(&db, 1)
            .role_of(2)
            .await
            .unwrap(),
        None
    );
    assert_eq!(
        WorkflowIntegration::shares(&db, 2)
            .role_of(2)
            .await
            .unwrap(),
        None
    );
}

/// `list` returns every grant of the resource in grant order, and nothing of other resources.
#[tokio::test]
async fn list_returns_grants_of_the_resource() {
    let w = world().await;
    exec(
        &w.db,
        "INSERT INTO projects (id, owner_id, name) VALUES (2, 1, 'other')",
    )
    .await;
    Project::shares(&w.db, 2)
        .grant(Principal::User(w.owner), Role::Owner)
        .await
        .unwrap();
    let shares = Project::shares(&w.db, w.thing);
    shares
        .grant(Principal::Group(w.group), Role::Editor)
        .await
        .unwrap();
    shares.grant(Principal::Public, Role::Viewer).await.unwrap();

    let listed: Vec<_> = shares
        .list()
        .await
        .unwrap()
        .into_iter()
        .map(|s| (Principal::from_db(s.principal_type, s.principal_id), s.role))
        .collect();
    assert_eq!(
        listed,
        vec![
            (Some(Principal::User(w.owner)), Role::Owner as i16),
            (Some(Principal::Group(w.group)), Role::Editor as i16),
            (Some(Principal::Public), Role::Viewer as i16),
        ]
    );
}

/// Deleting a group's shares removes them on all resources and leaves other principals alone.
#[tokio::test]
async fn delete_group_shares_cleans_up() {
    let w = world().await;
    let shares = Project::shares(&w.db, w.thing);
    shares
        .grant(Principal::Group(w.group), Role::Editor)
        .await
        .unwrap();
    assert_eq!(shares.role_of(w.member).await.unwrap(), Some(Role::Editor));

    Principal::delete_group_shares(&w.db, w.group)
        .await
        .unwrap();

    assert_eq!(shares.role_of(w.member).await.unwrap(), None);
    assert_eq!(shares.role_of(w.owner).await.unwrap(), Some(Role::Owner));
}

/// `ownerless_among` reports projects without a user or group Owner: viewer-only and public-owner
/// projects are ownerless, user-owned and group-owned ones are not.
#[tokio::test]
async fn ownerless_among_ignores_viewers_and_public_owners() {
    let w = world().await;
    for id in 2..=4 {
        exec(
            &w.db,
            &format!("INSERT INTO projects (id, owner_id, name) VALUES ({id}, 1, 'p{id}')"),
        )
        .await;
    }
    Project::shares(&w.db, 2)
        .grant(Principal::User(w.member), Role::Viewer)
        .await
        .unwrap();
    Project::shares(&w.db, 3)
        .grant(Principal::Group(w.group), Role::Owner)
        .await
        .unwrap();
    Project::shares(&w.db, 4)
        .grant(Principal::Public, Role::Owner)
        .await
        .unwrap();

    let ownerless = Project::ownerless_among(&w.db, [w.thing, 2, 3, 4])
        .await
        .unwrap();
    assert_eq!(ownerless, std::collections::HashSet::from([2, 4]));
    assert!(
        Project::ownerless_among(&w.db, [])
            .await
            .unwrap()
            .is_empty()
    );
}
