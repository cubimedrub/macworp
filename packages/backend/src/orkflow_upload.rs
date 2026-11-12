use std::io::Write;

use http::{Method, StatusCode};
use macworp_core::workflow_configuration::{
    WorkflowConfiguration,
    source::{RemoteSource, Source},
};
use sea_orm::EntityTrait;
use serde_json::{Value, json};

use crate::test_support::TestApp;

/// A zip with the given `(name, content)` files.
///
/// # Arguments
/// * `files` - File names and contents
fn zip_of(files: &[(&str, &str)]) -> Vec<u8> {
    let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for (name, content) in files {
        writer
            .start_file(*name, zip::write::SimpleFileOptions::default())
            .unwrap();
        writer.write_all(content.as_bytes()).unwrap();
    }
    writer.finish().unwrap().into_inner()
}

/// Update body for a workflow with the given configuration.
///
/// # Arguments
/// * `configuration` - Configuration to store
fn update_body(configuration: &WorkflowConfiguration) -> Value {
    json!({"name": "w", "description": "", "configuration": configuration})
}

/// An editor uploads a zip: files land in `<storage_path>/local/<id>`, the stored source
/// points there, viewers and outsiders cannot upload, and re-uploading replaces the files.
#[tokio::test]
async fn upload_installs_the_workflow() {
    let app = TestApp::new().await;
    let alice = app.user("alice").await;
    let viewer = app.user("viewer").await;
    let outsider = app.user("outsider").await;
    let workflow = app.workflow(&alice, "w").await;
    app.share::<crate::models::WorkflowIntegration>(
        workflow.id,
        crate::authorization::Principal::User(viewer.id),
        macworp_core::entities::Role::Viewer,
    )
    .await;
    let uri = format!("/workflow-integrations/upload/{}", workflow.id);
    let archive = zip_of(&[("flow/main.nf", "v1"), ("flow/bin/tool.sh", "x")]);

    for (user, expected) in [
        (&viewer, StatusCode::FORBIDDEN),
        (&outsider, StatusCode::NOT_FOUND),
    ] {
        let (status, _) = app
            .send_bytes(user, Method::POST, &uri, archive.clone())
            .await;
        assert_eq!(status, expected, "{}", user.username);
    }

    let (status, body) = app.send_bytes(&alice, Method::POST, &uri, archive).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let directory = app.state.config().local_workflow_directory(workflow.id);
    assert_eq!(
        std::fs::read_to_string(directory.join("main.nf")).unwrap(),
        "v1"
    );
    assert!(directory.join("bin/tool.sh").is_file());
    assert_eq!(
        body["configuration"]["source"]["Local"]["location"],
        // Relative to the storage root, workers resolve it against their own mount
        format!("local/{}", workflow.id)
    );

    let archive = zip_of(&[("main.nf", "v2")]);
    let (status, _) = app.send_bytes(&alice, Method::POST, &uri, archive).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        std::fs::read_to_string(directory.join("main.nf")).unwrap(),
        "v2"
    );
    assert!(!directory.join("bin").exists());
}

/// Bad uploads are 400 and keep the old files; a script missing from the archive is rejected
/// too; a custom script is honoured; non-local sources cannot take uploads.
#[tokio::test]
async fn upload_validates_the_archive() {
    let app = TestApp::new().await;
    let alice = app.user("alice").await;
    let workflow = app.workflow(&alice, "w").await;
    let uri = format!("/workflow-integrations/upload/{}", workflow.id);
    let directory = app.state.config().local_workflow_directory(workflow.id);

    let good = zip_of(&[("main.nf", "good")]);
    let (status, _) = app.send_bytes(&alice, Method::POST, &uri, good).await;
    assert_eq!(status, StatusCode::OK);

    for bad in [
        b"not a zip".to_vec(),
        zip_of(&[("../escape.nf", "x")]),
        zip_of(&[("other.nf", "x")]),
    ] {
        let (status, _) = app.send_bytes(&alice, Method::POST, &uri, bad).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }
    assert_eq!(
        std::fs::read_to_string(directory.join("main.nf")).unwrap(),
        "good"
    );

    let custom = zip_of(&[("workflows/run.nf", "y")]);
    let (status, body) = app
        .send_bytes(
            &alice,
            Method::POST,
            &format!("{uri}?script=workflows/run.nf"),
            custom,
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body["configuration"]["source"]["Local"]["script"],
        "workflows/run.nf"
    );

    let mut remote = WorkflowConfiguration::default();
    *remote.source_mut() = Source::Remote(RemoteSource::default());
    let (status, _) = app
        .post(
            &alice,
            &format!("/workflow-integrations/update/{}", workflow.id),
            Some(update_body(&remote)),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = app
        .send_bytes(&alice, Method::POST, &uri, zip_of(&[("main.nf", "x")]))
        .await;
    assert_eq!(status, StatusCode::CONFLICT);
}

/// On a server the location of a local source is not client controlled: an update naming
/// another directory is rewritten, and running a workflow without upload is refused.
#[tokio::test]
async fn local_location_is_confined_on_servers() {
    let app = TestApp::new().await;
    let alice = app.user("alice").await;
    let workflow = app.workflow(&alice, "w").await;
    let project = app.project(&alice, "p").await;

    let mut configuration = WorkflowConfiguration::default();
    *configuration.source_mut() = Source::Local(
        macworp_core::workflow_configuration::source::LocalSource::new(
            "/etc".into(),
            "main.nf".into(),
        ),
    );
    let (status, body) = app
        .post(
            &alice,
            &format!("/workflow-integrations/update/{}", workflow.id),
            Some(update_body(&configuration)),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body["configuration"]["source"]["Local"]["location"],
        format!("local/{}", workflow.id)
    );

    let (status, _) = app
        .post(
            &alice,
            &format!("/runs/start/{}", project.id),
            Some(json!({"workflow_id": workflow.id, "comment": "", "parameters": []})),
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT);
}

/// Deleting a workflow removes its uploaded files.
#[tokio::test]
async fn delete_removes_the_files() {
    let app = TestApp::new().await;
    let alice = app.user("alice").await;
    let workflow = app.workflow(&alice, "w").await;
    let (status, _) = app
        .send_bytes(
            &alice,
            Method::POST,
            &format!("/workflow-integrations/upload/{}", workflow.id),
            zip_of(&[("main.nf", "x")]),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let directory = app.state.config().local_workflow_directory(workflow.id);
    assert!(directory.is_dir());
    let (status, _) = app
        .post(
            &alice,
            &format!("/workflow-integrations/delete/{}", workflow.id),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert!(!directory.exists());
}

/// The `permit_local_workflows` policy: `no` blocks everybody (new workflows start with a remote
/// source, switching to local and uploading are 403), `admin-only` lets only administrators,
/// `all` everybody; existing local workflows stay editable and runnable.
#[tokio::test]
async fn policy_controls_who_creates_local_workflows() {
    use crate::configuration::LocalWorkflowPolicy;
    for (policy, user_allowed, admin_allowed) in [
        (LocalWorkflowPolicy::No, false, false),
        (LocalWorkflowPolicy::AdminOnly, false, true),
        (LocalWorkflowPolicy::All, true, true),
    ] {
        let app = TestApp::new_with(|c| *c.permit_local_workflows_mut() = policy).await;
        let alice = app.user("alice").await;
        let root = app.admin("root").await;
        for (user, allowed) in [(&alice, user_allowed), (&root, admin_allowed)] {
            let who = &user.username;
            // The frontend learns the policy from the current user.
            let (status, current) = app.get(user, "/user/current").await;
            assert_eq!(status, StatusCode::OK);
            assert_eq!(
                current["can_create_local_workflows"],
                json!(allowed),
                "current user {policy:?} {who}"
            );
            // A new workflow starts local only where allowed.
            let (status, body) = app
                .post(
                    user,
                    "/workflow-integrations/new",
                    Some(json!({"name": "n", "description": ""})),
                )
                .await;
            assert_eq!(status, StatusCode::OK, "{policy:?} {who}");
            let id = body["id"].as_i64().unwrap();
            assert_eq!(
                body["configuration"]["source"].get("Local").is_some(),
                allowed,
                "initial source {policy:?} {who}"
            );

            // Switching to local.
            let mut local = WorkflowConfiguration::default();
            *local.source_mut() =
                Source::Local(macworp_core::workflow_configuration::source::LocalSource::default());
            let (status, _) = app
                .post(
                    user,
                    &format!("/workflow-integrations/update/{id}"),
                    Some(update_body(&local)),
                )
                .await;
            let expected = if allowed {
                StatusCode::OK
            } else {
                StatusCode::FORBIDDEN
            };
            assert_eq!(status, expected, "update {policy:?} {who}");

            // Uploading.
            let (status, _) = app
                .send_bytes(
                    user,
                    Method::POST,
                    &format!("/workflow-integrations/upload/{id}"),
                    zip_of(&[("main.nf", "x")]),
                )
                .await;
            let expected = if allowed {
                StatusCode::OK
            } else {
                StatusCode::FORBIDDEN
            };
            assert_eq!(status, expected, "upload {policy:?} {who}");
        }
    }
}

/// A run of an uploaded workflow keeps the storage-relative location in its snapshot and hands
/// it to the worker that way.
#[tokio::test]
async fn run_snapshot_keeps_the_relative_location() {
    let app = TestApp::new_with(|c| *c.fixed_unavailable_engines_mut() = Some(Vec::new())).await;
    let alice = app.user("alice").await;
    let workflow = app.workflow(&alice, "w").await;
    let project = app.project_in_storage(&alice, "p").await;
    let (status, _) = app
        .send_bytes(
            &alice,
            Method::POST,
            &format!("/workflow-integrations/upload/{}", workflow.id),
            zip_of(&[("main.nf", "x")]),
        )
        .await;
    assert_eq!(status, StatusCode::OK);

    let (status, run) = app
        .post(
            &alice,
            &format!("/runs/start/{}", project.id),
            Some(json!({"workflow_id": workflow.id, "comment": "", "parameters": []})),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{run}");
    let stored = crate::models::RunTable::find_by_id(run["id"].as_i64().unwrap())
        .one(app.state.db())
        .await
        .unwrap()
        .unwrap();
    match stored.configuration.source() {
        Source::Local(local) => assert_eq!(
            local.location(),
            &std::path::PathBuf::from(format!("local/{}", workflow.id))
        ),
        other => panic!("unexpected source {other:?}"),
    }
}
