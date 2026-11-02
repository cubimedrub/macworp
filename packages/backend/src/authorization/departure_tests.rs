//! Orphan rules of [`Departure`]: what happens to projects, workflows and groups when a user is
//! deleted. Only used for integration tests.

use macworp_core::{entities::Role, workflow_configuration::WorkflowConfiguration};
use macworp_database_migration::{Migrator, MigratorTrait};
use sea_orm::{
    ColumnTrait, ConnectionTrait, Database, DatabaseConnection, EntityTrait, PaginatorTrait,
    QueryFilter,
};

use super::{Departed, Departure, Principal, Resource};
use crate::models::{
    GroupTable, Project, ProjectTable, WorkflowIntegration, WorkflowIntegrationTable, share,
};

/// Id of the user who leaves in every test
const LEAVING: i64 = 1;

/// Migrated in-memory database with users 1 (`leaving`) to 4, enabled, and no resources.
async fn database() -> DatabaseConnection {
    let db = Database::connect("sqlite::memory:").await.unwrap();
    Migrator::up(&db, None).await.unwrap();
    for id in 1..=4 {
        exec(
            &db,
            &format!("INSERT INTO users (id, username, firstname, lastname, email) VALUES ({id}, 'u{id}', 'f', 'l', 'u{id}@x')"),
        )
        .await;
    }
    db
}

/// Runs a statement, panicking on failure.
async fn exec(db: &DatabaseConnection, sql: &str) {
    db.execute_unprepared(sql).await.unwrap();
}

/// Inserts project 1 created by `LEAVING` and grants `grants`.
async fn project(db: &DatabaseConnection, grants: &[(Principal, Role)]) {
    exec(
        db,
        "INSERT INTO projects (id, owner_id, name) VALUES (1, 1, 'p')",
    )
    .await;
    for (principal, role) in grants {
        Project::shares(db, 1)
            .grant(*principal, *role)
            .await
            .unwrap();
    }
}

/// Inserts workflow 1 created by `LEAVING` and grants `grants`.
async fn workflow(db: &DatabaseConnection, grants: &[(Principal, Role)]) {
    exec(db, "INSERT INTO workflow_integrations (id, owner_id, name, description) VALUES (1, 1, 'w', '')").await;
    for (principal, role) in grants {
        WorkflowIntegration::shares(db, 1)
            .grant(*principal, *role)
            .await
            .unwrap();
    }
}

/// Inserts group `id` with `owner` and the given members.
async fn group(db: &DatabaseConnection, id: i64, owner: i64, members: &[i64]) {
    exec(
        db,
        &format!("INSERT INTO groups (id, owner_id, name) VALUES ({id}, {owner}, 'g{id}')"),
    )
    .await;
    for member in members {
        exec(
            db,
            &format!("INSERT INTO users_groups (user_id, group_id) VALUES ({member}, {id})"),
        )
        .await;
    }
}

/// Settles the departure of [`LEAVING`] and deletes the user row, as the controller does.
async fn leave(db: &DatabaseConnection) -> Departed {
    let departed = Departure::of(db, LEAVING).settle().await.unwrap();
    exec(db, "DELETE FROM users WHERE id = 1").await;
    departed
}

/// `owner_id` of project 1, `None` if the project is gone or has no creator.
async fn project_creator(db: &DatabaseConnection) -> Option<i64> {
    ProjectTable::find_by_id(1)
        .one(db)
        .await
        .unwrap()
        .and_then(|p| p.owner_id)
}

/// Whether project 1 exists.
async fn project_exists(db: &DatabaseConnection) -> bool {
    ProjectTable::find_by_id(1).one(db).await.unwrap().is_some()
}

/// Whether workflow 1 exists.
async fn workflow_exists(db: &DatabaseConnection) -> bool {
    WorkflowIntegrationTable::find_by_id(1)
        .one(db)
        .await
        .unwrap()
        .is_some()
}

/// Number of shares of the user or group.
async fn shares_of(db: &DatabaseConnection, principal_type: i16, id: i64) -> u64 {
    share::Entity::find()
        .filter(share::Column::PrincipalType.eq(principal_type))
        .filter(share::Column::PrincipalId.eq(id))
        .count(db)
        .await
        .unwrap()
}

/// The oldest enabled user owner becomes the creator, disabled users are skipped, and the
/// leaving user's own shares are gone.
#[tokio::test]
async fn project_with_another_owner_goes_to_the_oldest_enabled_owner() {
    let db = database().await;
    exec(&db, "UPDATE users SET disabled = 1 WHERE id = 2").await;
    project(
        &db,
        &[
            (Principal::User(LEAVING), Role::Owner),
            (Principal::User(2), Role::Owner),
            (Principal::User(4), Role::Owner),
            (Principal::User(3), Role::Owner),
        ],
    )
    .await;

    let departed = leave(&db).await;

    assert!(departed.deleted_projects.is_empty() && departed.ownerless_projects.is_empty());
    assert_eq!(project_creator(&db).await, Some(4));
    assert_eq!(shares_of(&db, 1, LEAVING).await, 0);
}

/// With only a group owner left the project stays owned and has no creator to show.
#[tokio::test]
async fn project_with_only_a_group_owner_has_no_creator() {
    let db = database().await;
    group(&db, 1, 2, &[3]).await;
    project(
        &db,
        &[
            (Principal::User(LEAVING), Role::Owner),
            (Principal::Group(1), Role::Owner),
        ],
    )
    .await;

    let departed = leave(&db).await;

    assert!(departed.ownerless_projects.is_empty());
    assert!(project_exists(&db).await);
    assert_eq!(project_creator(&db).await, None);
    assert_eq!(
        Project::shares(&db, 1).role_of(3).await.unwrap(),
        Some(Role::Owner)
    );
}

/// An editor, whether user, group or public, keeps an ownerless project; a public owner counts
/// as an editor.
#[tokio::test]
async fn project_with_editors_becomes_ownerless() {
    for editor in [
        (Principal::User(2), Role::Editor),
        (Principal::Public, Role::Editor),
        (Principal::Public, Role::Owner),
    ] {
        let db = database().await;
        project(&db, &[(Principal::User(LEAVING), Role::Owner), editor]).await;

        let departed = leave(&db).await;

        assert_eq!(departed.ownerless_projects, vec![1], "{editor:?}");
        assert!(project_exists(&db).await);
        assert_eq!(project_creator(&db).await, None);
    }
}

/// Without owner and editor, viewers or nobody, the project is deleted with its shares.
#[tokio::test]
async fn project_without_owner_and_editor_is_deleted() {
    for others in [
        vec![],
        vec![(Principal::User(2), Role::Viewer)],
        vec![(Principal::Public, Role::Viewer)],
    ] {
        let db = database().await;
        let mut grants = vec![(Principal::User(LEAVING), Role::Owner)];
        grants.extend(others);
        project(&db, &grants).await;

        let departed = leave(&db).await;

        assert_eq!(departed.deleted_projects.len(), 1);
        assert!(!project_exists(&db).await);
        assert_eq!(share::Entity::find().count(&db).await.unwrap(), 0);
    }
}

/// Workflows survive ownerless with a viewer or editor and are deleted without anyone.
#[tokio::test]
async fn workflow_rules() {
    let db = database().await;
    workflow(
        &db,
        &[
            (Principal::User(LEAVING), Role::Owner),
            (Principal::User(2), Role::Viewer),
        ],
    )
    .await;
    let departed = leave(&db).await;
    assert_eq!(departed.ownerless_workflows, vec![1]);
    assert!(workflow_exists(&db).await);

    let db = database().await;
    workflow(&db, &[(Principal::User(LEAVING), Role::Owner)]).await;
    let departed = leave(&db).await;
    assert_eq!(departed.deleted_workflows.len(), 1);
    assert!(!workflow_exists(&db).await);

    let db = database().await;
    workflow(
        &db,
        &[
            (Principal::User(LEAVING), Role::Owner),
            (Principal::User(2), Role::Owner),
        ],
    )
    .await;
    let departed = leave(&db).await;
    assert!(departed.ownerless_workflows.is_empty());
    assert_eq!(
        WorkflowIntegrationTable::find_by_id(1)
            .one(&db)
            .await
            .unwrap()
            .unwrap()
            .owner_id,
        Some(2)
    );
}

/// The leaving user was only the last editor of an ownerless project: it is deleted now.
#[tokio::test]
async fn ownerless_project_losing_its_last_editor_is_deleted() {
    let db = database().await;
    project(
        &db,
        &[
            (Principal::User(LEAVING), Role::Editor),
            (Principal::User(2), Role::Viewer),
        ],
    )
    .await;
    exec(&db, "UPDATE projects SET owner_id = NULL").await;

    let departed = leave(&db).await;

    assert_eq!(departed.deleted_projects.len(), 1);
    assert!(!project_exists(&db).await);
}

/// A group the user owned is ownerless when shares name it and it has members, and deleted when
/// unused or without members.
#[tokio::test]
async fn group_rules() {
    let db = database().await;
    group(&db, 1, LEAVING, &[LEAVING, 2]).await; // used, has members
    group(&db, 2, LEAVING, &[LEAVING, 2]).await; // unused
    group(&db, 3, LEAVING, &[LEAVING]).await; // used, nobody left
    group(&db, 4, 2, &[LEAVING, 3]).await; // owned by someone else
    exec(
        &db,
        "INSERT INTO projects (id, owner_id, name) VALUES (1, 2, 'p')",
    )
    .await;
    Project::shares(&db, 1)
        .grant(Principal::User(2), Role::Owner)
        .await
        .unwrap();
    Project::shares(&db, 1)
        .grant(Principal::Group(1), Role::Viewer)
        .await
        .unwrap();
    Project::shares(&db, 1)
        .grant(Principal::Group(3), Role::Viewer)
        .await
        .unwrap();

    let departed = leave(&db).await;

    assert_eq!(departed.ownerless_groups, vec![1, 4]);
    let mut deleted = departed.deleted_groups.clone();
    deleted.sort();
    assert_eq!(deleted, vec![2, 3]);
    assert_eq!(GroupTable::find().count(&db).await.unwrap(), 2);
    assert_eq!(
        shares_of(&db, 2, 3).await,
        0,
        "shares of a deleted group are removed"
    );
}

/// A group deleted for being empty took the only editor role of a project with it, so the project
/// that lost its owner earlier is deleted in a second pass.
#[tokio::test]
async fn deleting_a_group_settles_resources_again() {
    let db = database().await;
    group(&db, 1, LEAVING, &[LEAVING]).await;
    project(
        &db,
        &[
            (Principal::User(LEAVING), Role::Owner),
            (Principal::Group(1), Role::Editor),
        ],
    )
    .await;

    let departed = leave(&db).await;

    assert_eq!(departed.deleted_groups, vec![1]);
    assert_eq!(departed.deleted_projects.len(), 1);
    assert!(departed.ownerless_projects.is_empty());
    assert!(!project_exists(&db).await);
}

/// Runs the user started stay without a starter.
#[tokio::test]
async fn runs_keep_no_starter() {
    let db = database().await;
    exec(
        &db,
        "INSERT INTO projects (id, owner_id, name) VALUES (1, 2, 'p')",
    )
    .await;
    let configuration = serde_json::to_string(&WorkflowConfiguration::default()).unwrap();
    exec(&db, &format!("INSERT INTO runs (id, project_id, started_by, configuration, parameters) VALUES (1, 1, 1, '{configuration}', '{{}}')")).await;

    leave(&db).await;

    let run = crate::models::RunTable::find_by_id(1)
        .one(&db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(run.started_by, None);
}
