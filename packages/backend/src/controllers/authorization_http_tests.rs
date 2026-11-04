use http::StatusCode;
use macworp_core::entities::Role;
use serde_json::{Value, json};

use crate::{
    authorization::{Principal, Resource},
    models::{Project, WorkflowIntegration},
    test_support::{TestApp, passed_authorization},
};

/// Names of the items in a JSON list response, sorted.
fn names(list: &Value) -> Vec<String> {
    let mut names: Vec<String> = list
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["name"].as_str().unwrap().to_string())
        .collect();
    names.sort();
    names
}

/// Request body for `/runs/start` with no parameter values.
fn start_body(workflow_id: i64) -> Value {
    json!({"workflow_id": workflow_id, "comment": "", "parameters": []})
}

/// Project lists contain own, shared (user, group, public) projects and never private ones of
/// others; the owner column shows the creator and lists carry no `my_role`.
#[tokio::test]
async fn project_list_is_scoped() {
    let app = TestApp::new().await;
    let alice = app.user("alice").await;
    let bob = app.user("bob").await;
    let carol = app.user("carol").await;
    let own = app.project(&bob, "own").await;
    let _private = app.project(&alice, "private").await;
    let shared = app.project(&alice, "shared").await;
    let via_group = app.project(&alice, "via-group").await;
    let public = app.project(&alice, "public").await;
    let group = app.group(&alice, "g", &[&bob]).await;
    app.share::<Project>(shared.id, Principal::User(bob.id), Role::Viewer)
        .await;
    app.share::<Project>(via_group.id, Principal::Group(group.id), Role::Viewer)
        .await;
    app.share::<Project>(public.id, Principal::Public, Role::Viewer)
        .await;
    let _ = own;

    let (status, body) = app.get(&bob, "/projects/index").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(names(&body), ["own", "public", "shared", "via-group"]);
    let shared_item = body
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["name"] == "shared")
        .unwrap();
    assert_eq!(shared_item["owner_username"], "alice");
    assert!(shared_item["my_role"].is_null());

    // Only the public project is visible to someone without a share
    let (_, body) = app.get(&carol, "/projects/index").await;
    assert_eq!(names(&body), ["public"]);
}

/// Workflow lists are scoped the same way.
#[tokio::test]
async fn workflow_list_is_scoped() {
    let app = TestApp::new().await;
    let alice = app.user("alice").await;
    let bob = app.user("bob").await;
    let _private = app.workflow(&alice, "private").await;
    let shared = app.workflow(&alice, "shared").await;
    let public = app.workflow(&alice, "public").await;
    app.share::<WorkflowIntegration>(shared.id, Principal::User(bob.id), Role::Viewer)
        .await;
    app.share::<WorkflowIntegration>(public.id, Principal::Public, Role::Viewer)
        .await;

    let (status, body) = app.get(&bob, "/workflow-integrations/index").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(names(&body), ["public", "shared"]);
}

/// `?all=true` lists everything for administrators and is rejected for everyone else.
#[tokio::test]
async fn show_all_is_admin_only() {
    let app = TestApp::new().await;
    let alice = app.user("alice").await;
    let bob = app.user("bob").await;
    let admin = app.admin("admin").await;
    app.project(&alice, "a").await;
    app.workflow(&alice, "w").await;
    app.group(&alice, "g", &[]).await;

    for (uri, expected) in [
        ("/projects/index?all=true", "a"),
        ("/workflow-integrations/index?all=true", "w"),
        ("/groups/index?all=true", "g"),
    ] {
        let (status, body) = app.get(&admin, uri).await;
        assert_eq!(status, StatusCode::OK, "{uri}");
        assert_eq!(names(&body), [expected], "{uri}");
        let (status, _) = app.get(&bob, uri).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{uri}");
    }
    // Without the flag the admin sees only what is shared with them
    let (_, body) = app.get(&admin, "/projects/index").await;
    assert!(names(&body).is_empty());
}

/// Project endpoints per role: viewer reads, editor changes files, owner deletes. Outsiders get
/// 404 and a too low role 403.
#[tokio::test]
async fn project_endpoints_need_their_role() {
    let app = TestApp::new().await;
    let owner = app.user("owner").await;
    let editor = app.user("editor").await;
    let viewer = app.user("viewer").await;
    let outsider = app.user("outsider").await;
    let admin = app.admin("admin").await;
    let project = app.project(&owner, "p").await;
    app.share::<Project>(project.id, Principal::User(editor.id), Role::Editor)
        .await;
    app.share::<Project>(project.id, Principal::User(viewer.id), Role::Viewer)
        .await;
    let id = project.id;

    let edit = format!("/projects/edit/{id}?subpath=");
    let folder = format!("/projects/create-folder/{id}");
    let delete_path = format!("/projects/delete-path/{id}");
    let delete = format!("/projects/delete/{id}");

    // (user, expected status for edit, create-folder, delete-path)
    let ok = StatusCode::OK;
    let no = StatusCode::FORBIDDEN;
    let missing = StatusCode::NOT_FOUND;
    for (user, edit_status, write_status) in [
        (&owner, ok, ok),
        (&admin, ok, ok),
        (&editor, ok, ok),
        (&viewer, ok, no),
        (&outsider, missing, missing),
    ] {
        let (status, body) = app.get(user, &edit).await;
        assert_eq!(status, edit_status, "edit as {}", user.username);
        if status == ok {
            assert!(body["my_role"].is_string());
        }
        let (status, _) = app
            .post(user, &folder, Some(json!({"folder_path": "new"})))
            .await;
        assert_eq!(status, write_status, "create-folder as {}", user.username);
        let (status, _) = app
            .post(user, &delete_path, Some(json!({"path": "new"})))
            .await;
        assert_eq!(status, write_status, "delete-path as {}", user.username);
    }

    // Deleting needs Owner
    for (user, expected) in [
        (&editor, no),
        (&viewer, no),
        (&outsider, missing),
        (&owner, ok),
    ] {
        let (status, _) = app.post(user, &delete, None).await;
        assert_eq!(status, expected, "delete as {}", user.username);
    }
    // The delete removed the shares
    assert!(
        Project::shares(app.state.db(), id)
            .list()
            .await
            .unwrap()
            .is_empty()
    );
}

/// Workflow endpoints per role: viewer reads, editor updates, owner deletes.
#[tokio::test]
async fn workflow_endpoints_need_their_role() {
    let app = TestApp::new().await;
    let owner = app.user("owner").await;
    let editor = app.user("editor").await;
    let viewer = app.user("viewer").await;
    let outsider = app.user("outsider").await;
    let admin = app.admin("admin").await;
    let workflow = app.workflow(&owner, "w").await;
    app.share::<WorkflowIntegration>(workflow.id, Principal::User(editor.id), Role::Editor)
        .await;
    app.share::<WorkflowIntegration>(workflow.id, Principal::User(viewer.id), Role::Viewer)
        .await;
    let id = workflow.id;

    let (_, current) = app
        .get(&owner, &format!("/workflow-integrations/edit/{id}"))
        .await;
    let update = json!({
        "name": "renamed",
        "description": "d",
        "configuration": current["configuration"],
    });

    let ok = StatusCode::OK;
    let no = StatusCode::FORBIDDEN;
    let missing = StatusCode::NOT_FOUND;
    for (user, view_status, update_status) in [
        (&owner, ok, ok),
        (&admin, ok, ok),
        (&editor, ok, ok),
        (&viewer, ok, no),
        (&outsider, missing, missing),
    ] {
        let (status, body) = app
            .get(user, &format!("/workflow-integrations/edit/{id}"))
            .await;
        assert_eq!(status, view_status, "view as {}", user.username);
        if status == ok {
            assert!(body["my_role"].is_string());
        }
        let (status, _) = app
            .post(
                user,
                &format!("/workflow-integrations/update/{id}"),
                Some(update.clone()),
            )
            .await;
        assert_eq!(status, update_status, "update as {}", user.username);
    }

    for (user, expected) in [
        (&editor, no),
        (&viewer, no),
        (&outsider, missing),
        (&owner, ok),
    ] {
        let (status, _) = app
            .post(user, &format!("/workflow-integrations/delete/{id}"), None)
            .await;
        assert_eq!(status, expected, "delete as {}", user.username);
    }
    assert!(
        WorkflowIntegration::shares(app.state.db(), id)
            .list()
            .await
            .unwrap()
            .is_empty()
    );
}

/// A public share on a workflow is capped at Viewer, on a project it may reach Owner.
#[tokio::test]
async fn public_share_caps_apply_over_http() {
    let app = TestApp::new().await;
    let owner = app.user("owner").await;
    let other = app.user("other").await;
    let workflow = app.workflow(&owner, "w").await;
    let project = app.project(&owner, "p").await;

    let grant = |role: &str| json!({"principal": "Public", "role": role});
    let workflow_uri = format!("/shares/workflows/{}/grant", workflow.id);
    for role in ["Editor", "Owner"] {
        let (status, _) = app.post(&owner, &workflow_uri, Some(grant(role))).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "workflow public {role}");
    }
    let (status, _) = app.post(&owner, &workflow_uri, Some(grant("Viewer"))).await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = app
        .get(
            &other,
            &format!("/workflow-integrations/edit/{}", workflow.id),
        )
        .await;
    assert_eq!(status, StatusCode::OK);

    let (status, _) = app
        .post(
            &owner,
            &format!("/shares/projects/{}/grant", project.id),
            Some(grant("Owner")),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
}

/// Granting needs Owner on the resource: an editor is refused.
#[tokio::test]
async fn only_owners_manage_shares() {
    let app = TestApp::new().await;
    let owner = app.user("owner").await;
    let editor = app.user("editor").await;
    let target = app.user("target").await;
    let project = app.project(&owner, "p").await;
    app.share::<Project>(project.id, Principal::User(editor.id), Role::Editor)
        .await;

    let uri = format!("/shares/projects/{}/grant", project.id);
    let body = json!({"principal": {"User": target.id}, "role": "Viewer"});
    let (status, _) = app.post(&editor, &uri, Some(body.clone())).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _) = app.post(&owner, &uri, Some(body)).await;
    assert_eq!(status, StatusCode::OK);
}

/// Starting a run needs project Editor and workflow Viewer.
#[tokio::test]
async fn run_start_needs_both_roles() {
    let app = TestApp::new().await;
    let alice = app.user("alice").await;
    let bob = app.user("bob").await;
    let project = app.project(&alice, "p").await;
    let workflow = app.workflow(&alice, "w").await;
    let uri = format!("/runs/start/{}", project.id);
    let body = start_body(workflow.id);

    // No share at all: project is invisible
    let (status, _) = app.post(&bob, &uri, Some(body.clone())).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // Project Viewer: too low, even with a visible workflow
    app.share::<Project>(project.id, Principal::User(bob.id), Role::Viewer)
        .await;
    app.share::<WorkflowIntegration>(workflow.id, Principal::User(bob.id), Role::Viewer)
        .await;
    let (status, _) = app.post(&bob, &uri, Some(body.clone())).await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // Project Editor but the workflow is shared with nobody else
    app.share::<Project>(project.id, Principal::User(bob.id), Role::Editor)
        .await;
    let hidden = app.workflow(&alice, "hidden").await;
    let (status, _) = app.post(&bob, &uri, Some(start_body(hidden.id))).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // Both roles: authorization passes
    let (status, _) = app.post(&bob, &uri, Some(body)).await;
    assert!(passed_authorization(status), "got {status}");
}

/// A public Editor share on the project plus a public workflow let anyone run it.
#[tokio::test]
async fn public_editor_project_allows_running() {
    let app = TestApp::new().await;
    let alice = app.user("alice").await;
    let bob = app.user("bob").await;
    let project = app.project(&alice, "p").await;
    let workflow = app.workflow(&alice, "w").await;
    app.share::<Project>(project.id, Principal::Public, Role::Editor)
        .await;
    app.share::<WorkflowIntegration>(workflow.id, Principal::Public, Role::Viewer)
        .await;

    let (status, _) = app
        .post(
            &bob,
            &format!("/runs/start/{}", project.id),
            Some(start_body(workflow.id)),
        )
        .await;
    assert!(passed_authorization(status), "got {status}");
}

/// Reading runs needs project Viewer, cancelling project Editor; workflow names the caller
/// cannot see are blanked.
#[tokio::test]
async fn run_endpoints_need_their_role() {
    let app = TestApp::new().await;
    let alice = app.user("alice").await;
    let viewer = app.user("viewer").await;
    let editor = app.user("editor").await;
    let outsider = app.user("outsider").await;
    let project = app.project(&alice, "p").await;
    let workflow = app.workflow(&alice, "secret").await;
    app.share::<Project>(project.id, Principal::User(viewer.id), Role::Viewer)
        .await;
    app.share::<Project>(project.id, Principal::User(editor.id), Role::Editor)
        .await;
    let run = app.run(&project, &workflow).await;

    let reads = [
        format!("/runs/index/{}", project.id),
        format!("/runs/get/{}", run.id),
        format!("/runs/logs/{}", run.id),
    ];
    for uri in &reads {
        for user in [&alice, &viewer, &editor] {
            let (status, _) = app.get(user, uri).await;
            assert_eq!(status, StatusCode::OK, "{uri} as {}", user.username);
        }
        let (status, _) = app.get(&outsider, uri).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{uri} as outsider");
    }

    // The viewer has no share on the workflow, its name stays hidden
    let (_, body) = app.get(&viewer, &reads[0]).await;
    assert_eq!(body[0]["workflow_name"], "");
    let (_, body) = app.get(&alice, &reads[0]).await;
    assert_eq!(body[0]["workflow_name"], "secret");

    let cancel = format!("/runs/cancel/{}", run.id);
    let (status, _) = app.post(&viewer, &cancel, None).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _) = app.post(&outsider, &cancel, None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = app.post(&editor, &cancel, None).await;
    assert_eq!(status, StatusCode::OK);
}

/// Creating gives the creator an Owner share and `my_role`; lists carry none.
#[tokio::test]
async fn creator_becomes_owner() {
    let app = TestApp::new().await;
    let alice = app.user("alice").await;
    let path = app.directory.path().join("created");
    std::fs::create_dir_all(&path).unwrap();

    let (status, project) = app
        .post(
            &alice,
            "/projects/new",
            Some(json!({"name": "p", "path": path})),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{project}");
    assert_eq!(project["my_role"], "Owner");
    let id = project["id"].as_i64().unwrap();
    let shares = Project::shares(app.state.db(), id).list().await.unwrap();
    assert_eq!(shares.len(), 1);
    assert_eq!(shares[0].principal_id, Some(alice.id));

    let (status, workflow) = app
        .post(
            &alice,
            "/workflow-integrations/new",
            Some(json!({"name": "w", "description": ""})),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{workflow}");
    assert_eq!(workflow["my_role"], "Owner");
    let id = workflow["id"].as_i64().unwrap();
    assert_eq!(
        WorkflowIntegration::shares(app.state.db(), id)
            .list()
            .await
            .unwrap()
            .len(),
        1
    );
}

/// The `ownerless` flag is set once the creator is gone and no Owner share is left.
#[tokio::test]
async fn ownerless_flag_is_reported() {
    let app = TestApp::new().await;
    let alice = app.user("alice").await;
    let admin = app.admin("admin").await;
    let project = app.project(&alice, "p").await;
    let (_, body) = app
        .get(&admin, &format!("/projects/edit/{}?subpath=", project.id))
        .await;
    assert_eq!(body["ownerless"], false);

    Project::shares(app.state.db(), project.id)
        .delete_all()
        .await
        .unwrap();
    let (_, body) = app
        .get(&admin, &format!("/projects/edit/{}?subpath=", project.id))
        .await;
    assert_eq!(body["ownerless"], true);
}

/// User administration is admin only, except for deleting one's own account and reading the
/// current user.
#[tokio::test]
async fn user_administration_is_admin_only() {
    let app = TestApp::new().await;
    let admin = app.admin("admin").await;
    let bob = app.user("bob").await;

    let (status, _) = app.get(&bob, "/user/index").await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, body) = app.get(&admin, "/user/index").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body.as_array().unwrap().len(), 2);

    for action in ["disable", "enable"] {
        let (status, _) = app
            .post(&bob, &format!("/user/{action}/{}", admin.id), None)
            .await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{action}");
    }
    let (status, _) = app
        .post(&bob, &format!("/user/delete/{}", admin.id), None)
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    let (status, _) = app.get(&bob, "/user/current").await;
    assert_eq!(status, StatusCode::OK);
}

/// Requests without credentials are rejected before any resource lookup.
#[tokio::test]
async fn anonymous_requests_are_rejected() {
    let app = TestApp::new().await;
    for uri in [
        "/projects/index",
        "/workflow-integrations/index",
        "/groups/index",
        "/user/current",
    ] {
        let (status, _) = app.send(None, http::Method::GET, uri, None).await;
        assert!(
            status == StatusCode::BAD_REQUEST || status == StatusCode::UNAUTHORIZED,
            "{uri}: {status}"
        );
    }
}
