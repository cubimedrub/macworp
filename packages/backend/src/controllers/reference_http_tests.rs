use http::{Method, StatusCode};
use macworp_core::{entities::Role, routing::ReferenceRouting};
use serde_json::json;

use crate::{
    authorization::{Principal, Resource},
    models::ReferenceData,
    test_support::TestApp,
};

/// Browse URL of the singleton.
const EDIT: &str = "/reference/edit/1";
/// Create-folder URL of the singleton.
const CREATE_FOLDER: &str = "/reference/create-folder/1";
/// Grant URL of the singleton's shares.
const GRANT: &str = "/shares/reference/1/grant";
/// Revoke URL of the singleton's shares.
const REVOKE: &str = "/shares/reference/1/revoke";

/// Every user may browse, files show up, and the role is Viewer.
#[tokio::test]
async fn everybody_can_read() {
    let app = TestApp::new().await;
    let alice = app.user("alice").await;
    std::fs::write(app.state.reference_directory().get().join("genome.fa"), "x").unwrap();
    let (status, body) = app.get(&alice, EDIT).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["files"], json!(["genome.fa"]));
    assert_eq!(body["my_role"], json!("Viewer"));
    assert_eq!(ReferenceRouting::SINGLETON_ID, 1);
}

/// Anonymous requests are rejected, a missing id is not found.
#[tokio::test]
async fn anonymous_and_unknown_id_are_rejected() {
    let app = TestApp::new().await;
    let alice = app.user("alice").await;
    let (status, _) = app.send(None, Method::GET, EDIT, None).await;
    assert!(status == StatusCode::BAD_REQUEST || status == StatusCode::UNAUTHORIZED);
    let (status, _) = app.get(&alice, "/reference/edit/2").await;
    // Not shared with anyone: invisible
    assert_eq!(status, StatusCode::NOT_FOUND);
}

/// Viewers cannot write (403); Editors and admins pass authorization (405 here: the desktop
/// filesystem used by the tests is read-only).
#[tokio::test]
async fn writing_needs_editor_or_admin() {
    let app = TestApp::new().await;
    let viewer = app.user("viewer").await;
    let editor = app.user("editor").await;
    let grouped = app.user("grouped").await;
    let admin = app.admin("root").await;
    let group = app.group(&admin, "curators", &[&grouped]).await;
    app.share::<ReferenceData>(1, Principal::User(editor.id), Role::Editor)
        .await;
    app.share::<ReferenceData>(1, Principal::Group(group.id), Role::Editor)
        .await;
    let body = || Some(json!({"folder_path": "x"}));
    let (status, _) = app.post(&viewer, CREATE_FOLDER, body()).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    for user in [&editor, &grouped, &admin] {
        let (status, _) = app.post(user, CREATE_FOLDER, body()).await;
        assert_eq!(status, StatusCode::METHOD_NOT_ALLOWED, "{}", user.username);
    }
}

/// Only admins manage shares; no Owner and no change of the public Viewer share.
#[tokio::test]
async fn share_rules() {
    let app = TestApp::new().await;
    let editor = app.user("editor").await;
    let other = app.user("other").await;
    let admin = app.admin("root").await;
    app.share::<ReferenceData>(1, Principal::User(editor.id), Role::Editor)
        .await;
    let grant = json!({"principal": {"User": other.id}, "role": "Editor"});
    let (status, _) = app.post(&editor, GRANT, Some(grant.clone())).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _) = app.post(&admin, GRANT, Some(grant)).await;
    assert_eq!(status, StatusCode::OK);

    let owner = json!({"principal": {"User": other.id}, "role": "Owner"});
    let (status, _) = app.post(&admin, GRANT, Some(owner)).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let public = json!({"principal": "Public", "role": "Viewer"});
    let (status, _) = app.post(&admin, GRANT, Some(public)).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, _) = app
        .post(&admin, REVOKE, Some(json!({"principal": "Public"})))
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // Everybody still reads
    let (status, _) = app.get(&other, EDIT).await;
    assert_eq!(status, StatusCode::OK);
}

/// Deleting a user who is an Editor keeps the reference data and its public read access.
#[tokio::test]
async fn user_deletion_keeps_reference_data() {
    let app = TestApp::new().await;
    let editor = app.user("editor").await;
    let reader = app.user("reader").await;
    app.share::<ReferenceData>(1, Principal::User(editor.id), Role::Editor)
        .await;
    let (status, _) = app
        .post(&editor, &format!("/user/delete/{}", editor.id), None)
        .await;
    assert!(status.is_success(), "{status}");
    let (status, _) = app.get(&reader, EDIT).await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        ReferenceData::shares(app.state.db(), 1)
            .role_of(reader.id)
            .await
            .unwrap()
            .is_some()
    );
}

/// Symlinks inside the reference directory are not followed.
#[cfg(unix)]
#[tokio::test]
async fn symlinks_are_rejected() {
    let app = TestApp::new().await;
    let alice = app.user("alice").await;
    let outside = tempfile::tempdir().unwrap();
    std::os::unix::fs::symlink(
        outside.path(),
        app.state.reference_directory().get().join("link"),
    )
    .unwrap();
    let (status, _) = app.get(&alice, &format!("{EDIT}?subpath=link")).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

/// A project's top level shows the virtual reference folder (a real project folder of that name
/// is hidden), its content is the reference data and read-only, and projects cannot write there.
#[tokio::test]
async fn project_browser_shows_the_reference_folder() {
    let app = TestApp::new().await;
    let alice = app.user("alice").await;
    let project = app.project(&alice, "p").await;
    let project_directory = std::path::PathBuf::from(project.path.clone().unwrap());
    std::fs::create_dir_all(project_directory.join("macworp_global_reference_data")).unwrap();
    std::fs::create_dir_all(project_directory.join("results")).unwrap();
    let reference = app.state.reference_directory().get();
    std::fs::create_dir_all(reference.join("hg38")).unwrap();
    std::fs::write(reference.join("hg38/a.fa"), "x").unwrap();

    let (status, body) = app
        .get(&alice, &format!("/projects/edit/{}?subpath=", project.id))
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body["directories"],
        json!(["macworp_global_reference_data", "results"])
    );
    assert_eq!(body["read_only"], json!(false));

    let uri = format!(
        "/projects/edit/{}?subpath=macworp_global_reference_data/hg38",
        project.id
    );
    let (status, body) = app.get(&alice, &uri).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["files"], json!(["a.fa"]));
    assert_eq!(body["read_only"], json!(true));
    assert_eq!(body["current"], json!("macworp_global_reference_data/hg38"));

    let (status, _) = app
        .post(
            &alice,
            &format!("/projects/create-folder/{}", project.id),
            Some(json!({"folder_path": "macworp_global_reference_data/x"})),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}
