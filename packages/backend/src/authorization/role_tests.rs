use macworp_core::entities::Role;
use sea_orm::EntityTrait;

use super::*;
use crate::{
    models::{Project, UsersGroupsTable, WorkflowIntegration},
    test_support::TestApp,
};

/// Runs [`authorize`] for a resource type and minimum role, returning the effective role or the error.
macro_rules! check {
    ($resource:ty, $minimum:ty, $app:expr, $user:expr, $id:expr) => {
        authorize::<$resource, $minimum>(&$app.state, $user.clone(), $id)
            .await
            .map(|authorized| authorized.role)
    };
}

/// A workflow shared with a user grants exactly the shared role, each role passing its own
/// minimum and failing the higher ones.
#[tokio::test]
async fn workflow_user_shares_per_role() {
    let app = TestApp::new().await;
    let owner = app.user("owner").await;
    let workflow = app.workflow(&owner, "w").await;
    for (role, name) in [
        (Role::Viewer, "viewer"),
        (Role::Editor, "editor"),
        (Role::Owner, "owner2"),
    ] {
        let user = app.user(name).await;
        app.share::<WorkflowIntegration>(workflow.id, Principal::User(user.id), role)
            .await;
        assert_eq!(
            check!(WorkflowIntegration, Viewer, app, user, workflow.id).unwrap(),
            role
        );
        assert_eq!(
            check!(WorkflowIntegration, Editor, app, user, workflow.id).is_ok(),
            role >= Role::Editor
        );
        assert_eq!(
            check!(WorkflowIntegration, Owner, app, user, workflow.id).is_ok(),
            role >= Role::Owner
        );
    }
}

/// Group shares reach members through `authorize`, for projects and workflows, and stop reaching
/// a user who leaves the group.
#[tokio::test]
async fn group_share_follows_membership() {
    let app = TestApp::new().await;
    let owner = app.user("owner").await;
    let member = app.user("member").await;
    let outsider = app.user("outsider").await;
    let project = app.project(&owner, "p").await;
    let workflow = app.workflow(&owner, "w").await;
    let group = app.group(&owner, "g", &[&member]).await;
    app.share::<Project>(project.id, Principal::Group(group.id), Role::Editor)
        .await;
    app.share::<WorkflowIntegration>(workflow.id, Principal::Group(group.id), Role::Viewer)
        .await;

    assert_eq!(
        check!(Project, Viewer, app, member, project.id).unwrap(),
        Role::Editor
    );
    assert_eq!(
        check!(WorkflowIntegration, Viewer, app, member, workflow.id).unwrap(),
        Role::Viewer
    );
    assert!(matches!(
        check!(Project, Viewer, app, outsider, project.id),
        Err(Error::NotFound)
    ));

    UsersGroupsTable::delete_by_id((member.id, group.id))
        .exec(app.state.db())
        .await
        .unwrap();
    assert!(matches!(
        check!(Project, Viewer, app, member, project.id),
        Err(Error::NotFound)
    ));
    assert!(matches!(
        check!(WorkflowIntegration, Viewer, app, member, workflow.id),
        Err(Error::NotFound)
    ));
}

/// Changing a grant takes effect on the next check, in both directions.
#[tokio::test]
async fn role_change_takes_effect() {
    let app = TestApp::new().await;
    let owner = app.user("owner").await;
    let user = app.user("user").await;
    let project = app.project(&owner, "p").await;

    app.share::<Project>(project.id, Principal::User(user.id), Role::Viewer)
        .await;
    assert!(matches!(
        check!(Project, Editor, app, user, project.id),
        Err(Error::Forbidden)
    ));
    app.share::<Project>(project.id, Principal::User(user.id), Role::Editor)
        .await;
    assert_eq!(
        check!(Project, Editor, app, user, project.id).unwrap(),
        Role::Editor
    );
    app.share::<Project>(project.id, Principal::User(user.id), Role::Viewer)
        .await;
    assert!(matches!(
        check!(Project, Editor, app, user, project.id),
        Err(Error::Forbidden)
    ));
}

/// An administrator is Owner on workflows and projects nobody shared with them.
#[tokio::test]
async fn admin_owns_everything() {
    let app = TestApp::new().await;
    let owner = app.user("owner").await;
    let admin = app.admin("admin").await;
    let project = app.project(&owner, "p").await;
    let workflow = app.workflow(&owner, "w").await;
    assert_eq!(
        check!(Project, Owner, app, admin, project.id).unwrap(),
        Role::Owner
    );
    assert_eq!(
        check!(WorkflowIntegration, Owner, app, admin, workflow.id).unwrap(),
        Role::Owner
    );
}
