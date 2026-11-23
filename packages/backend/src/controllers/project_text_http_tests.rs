use http::{Method, StatusCode};
use macworp_core::{entities::Role, requests::TEXT_FILE_LIMIT};
use serde_json::json;

use crate::{authorization::Principal, models::Project, test_support::TestApp};

/// Reading a file that was written returns the content; writing replaces it atomically and
/// creates missing folders, leaving no temporary file behind.
#[tokio::test]
async fn write_then_read_round_trips() {
    let app = TestApp::new().await;
    let alice = app.user("alice").await;
    let project = app.project_in_storage(&alice, "p").await;
    let uri = |path: &str, verb: &str| format!("/projects/{verb}/{}?path={path}", project.id);

    for content in ["a,b\n1,2\n", "replaced\n"] {
        let (status, _) = app
            .send(
                Some(&alice),
                Method::PUT,
                &uri("sheets/s.csv", "write-text"),
                Some(json!({"content": content})),
            )
            .await;
        assert_eq!(status, StatusCode::OK);
        let (status, body) = app.get(&alice, &uri("sheets/s.csv", "read-text")).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["content"], content);
    }
    let names: Vec<_> = std::fs::read_dir(app.state.config().projects_directory().join("p/sheets"))
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    assert_eq!(names, ["s.csv"]);
}

/// Missing file 404, directory 404, binary 422, too large 413 on read and write.
#[tokio::test]
async fn read_and_write_limits() {
    let app = TestApp::new().await;
    let alice = app.user("alice").await;
    let project = app.project_in_storage(&alice, "p").await;
    let root = app.state.config().projects_directory().join("p");
    std::fs::create_dir(root.join("dir")).unwrap();
    std::fs::write(root.join("binary"), [0xff, 0xfe, 0x00]).unwrap();
    std::fs::write(root.join("big"), vec![b'x'; TEXT_FILE_LIMIT + 1]).unwrap();
    let read = |path: &str| format!("/projects/read-text/{}?path={path}", project.id);

    assert_eq!(
        app.get(&alice, &read("nope")).await.0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(app.get(&alice, &read("dir")).await.0, StatusCode::NOT_FOUND);
    assert_eq!(
        app.get(&alice, &read("binary")).await.0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(
        app.get(&alice, &read("big")).await.0,
        StatusCode::PAYLOAD_TOO_LARGE
    );
    let (status, _) = app
        .send(
            Some(&alice),
            Method::PUT,
            &format!("/projects/write-text/{}?path=huge", project.id),
            Some(json!({"content": "x".repeat(TEXT_FILE_LIMIT + 1)})),
        )
        .await;
    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
    assert!(!root.join("huge").exists());
}

/// `..` cannot leave the project, symlinks are refused and the reference folder is reserved.
#[cfg(unix)]
#[tokio::test]
async fn paths_stay_confined() {
    let app = TestApp::new().await;
    let alice = app.user("alice").await;
    let project = app.project_in_storage(&alice, "p").await;
    let root = app.state.config().projects_directory().join("p");
    let outside = app.directory.path().join("outside.txt");
    std::fs::write(&outside, "secret").unwrap();
    std::os::unix::fs::symlink(&outside, root.join("link")).unwrap();
    let read = |path: &str| format!("/projects/read-text/{}?path={path}", project.id);

    assert_eq!(
        app.get(&alice, &read("link")).await.0,
        StatusCode::FORBIDDEN
    );
    // `..` is dropped, so this is `<project>/outside.txt`, which does not exist
    assert_eq!(
        app.get(&alice, &read("../../outside.txt")).await.0,
        StatusCode::NOT_FOUND
    );
    let (status, _) = app
        .send(
            Some(&alice),
            Method::PUT,
            &format!("/projects/write-text/{}?path=link", project.id),
            Some(json!({"content": "overwritten"})),
        )
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(std::fs::read_to_string(&outside).unwrap(), "secret");
    assert_eq!(
        app.get(&alice, &read("macworp_global_reference_data/x"))
            .await
            .0,
        StatusCode::BAD_REQUEST
    );
}

/// Viewer may read but not write, an outsider gets 404 for both.
#[tokio::test]
async fn roles_are_enforced() {
    let app = TestApp::new().await;
    let alice = app.user("alice").await;
    let viewer = app.user("viewer").await;
    let editor = app.user("editor").await;
    let outsider = app.user("outsider").await;
    let project = app.project_in_storage(&alice, "p").await;
    app.share::<Project>(project.id, Principal::User(viewer.id), Role::Viewer)
        .await;
    app.share::<Project>(project.id, Principal::User(editor.id), Role::Editor)
        .await;
    std::fs::write(
        app.state.config().projects_directory().join("p/f.txt"),
        "hi",
    )
    .unwrap();
    let read = format!("/projects/read-text/{}?path=f.txt", project.id);
    let write = format!("/projects/write-text/{}?path=g.txt", project.id);
    let put = |user| {
        app.send(
            Some(user),
            Method::PUT,
            &write,
            Some(json!({"content": "x"})),
        )
    };

    assert_eq!(app.get(&viewer, &read).await.0, StatusCode::OK);
    assert_eq!(put(&viewer).await.0, StatusCode::FORBIDDEN);
    assert_eq!(put(&editor).await.0, StatusCode::OK);
    assert_eq!(app.get(&outsider, &read).await.0, StatusCode::NOT_FOUND);
    assert_eq!(put(&outsider).await.0, StatusCode::NOT_FOUND);
}
