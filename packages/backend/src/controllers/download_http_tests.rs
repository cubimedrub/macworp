use std::io::{Cursor, Read};

use http::{StatusCode, header};
use macworp_core::entities::Role;

use crate::{authorization::Principal, models::Project, test_support::TestApp};

/// Names and contents of every file in a zip archive, sorted by name, plus its folder entries.
///
/// # Arguments
/// * `bytes` - The archive
fn archive_entries(bytes: &[u8]) -> (Vec<(String, String)>, Vec<String>) {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).expect("valid zip archive");
    let mut files = Vec::new();
    let mut folders = Vec::new();
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).unwrap();
        if entry.is_dir() {
            folders.push(entry.name().unwrap().to_string());
        } else {
            let mut content = String::new();
            entry.read_to_string(&mut content).unwrap();
            files.push((entry.name().unwrap().to_string(), content));
        }
    }
    files.sort();
    folders.sort();
    (files, folders)
}

/// A project `p` in storage with `a.txt`, `data/b.txt` and an empty folder `data/empty`.
///
/// # Arguments
/// * `app` - The test application
/// * `owner` - Owner of the project
async fn project_with_files(app: &TestApp, owner: &crate::models::User) -> Project {
    let project = app.project_in_storage(owner, "p").await;
    let root = app.state.config().projects_directory().join("p");
    std::fs::create_dir_all(root.join("data/empty")).unwrap();
    std::fs::write(root.join("a.txt"), "alpha").unwrap();
    std::fs::write(root.join("data/b.txt"), "beta").unwrap();
    project
}

/// A file is sent as it is, with length and a download file name.
#[tokio::test]
async fn file_is_downloaded_as_it_is() {
    let app = TestApp::new().await;
    let alice = app.user("alice").await;
    let project = project_with_files(&app, &alice).await;

    let (status, headers, body) = app
        .get_raw(
            Some(&alice),
            &format!("/projects/download/{}?path=data/b.txt", project.id),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, b"beta");
    assert_eq!(headers[header::CONTENT_LENGTH], "4");
    assert!(
        headers[header::CONTENT_DISPOSITION]
            .to_str()
            .unwrap()
            .starts_with("attachment; filename=\"b.txt\"")
    );
    assert_eq!(headers[header::X_CONTENT_TYPE_OPTIONS], "nosniff");
}

/// A folder is zipped below its own name, empty folders included.
#[tokio::test]
async fn folder_is_zipped_on_demand() {
    let app = TestApp::new().await;
    let alice = app.user("alice").await;
    let project = project_with_files(&app, &alice).await;

    let (status, headers, body) = app
        .get_raw(
            Some(&alice),
            &format!("/projects/download/{}?path=data", project.id),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers[header::CONTENT_TYPE], "application/zip");
    assert!(
        headers[header::CONTENT_DISPOSITION]
            .to_str()
            .unwrap()
            .contains("filename=\"data.zip\"")
    );
    let (files, folders) = archive_entries(&body);
    assert_eq!(files, [("data/b.txt".to_string(), "beta".to_string())]);
    assert_eq!(folders, ["data/", "data/empty/"]);
}

/// Without a path the whole project is zipped, below the project name; unfinished uploads and
/// a real folder named like the virtual reference folder are left out.
#[tokio::test]
async fn whole_project_is_zipped() {
    let app = TestApp::new().await;
    let alice = app.user("alice").await;
    let project = project_with_files(&app, &alice).await;
    let root = app.state.config().projects_directory().join("p");
    std::fs::write(root.join(".big.macworp_partial_upload"), "x").unwrap();
    std::fs::create_dir(root.join(macworp_core::paths::REFERENCE_PREFIX)).unwrap();
    std::fs::write(
        root.join(macworp_core::paths::REFERENCE_PREFIX)
            .join("hidden"),
        "x",
    )
    .unwrap();

    let (status, _, body) = app
        .get_raw(Some(&alice), &format!("/projects/download/{}", project.id))
        .await;
    assert_eq!(status, StatusCode::OK);
    let (files, _) = archive_entries(&body);
    let names: Vec<_> = files.iter().map(|(name, _)| name.as_str()).collect();
    assert_eq!(names, ["p/a.txt", "p/data/b.txt"]);
}

/// A folder big enough to need many chunks arrives complete.
#[tokio::test]
async fn large_folder_streams_completely() {
    let app = TestApp::new().await;
    let alice = app.user("alice").await;
    let project = app.project_in_storage(&alice, "p").await;
    let root = app.state.config().projects_directory().join("p");
    let content = "0123456789abcdef".repeat(1024 * 1024);
    std::fs::write(root.join("big.txt"), &content).unwrap();

    let (status, _, body) = app
        .get_raw(Some(&alice), &format!("/projects/download/{}", project.id))
        .await;
    assert_eq!(status, StatusCode::OK);
    let (files, _) = archive_entries(&body);
    assert_eq!(files.len(), 1);
    assert!(files[0].1 == content);
}

/// Viewers may download, outsiders do not see the project, anonymous requests are rejected.
#[tokio::test]
async fn roles_are_enforced() {
    let app = TestApp::new().await;
    let alice = app.user("alice").await;
    let bob = app.user("bob").await;
    let project = project_with_files(&app, &alice).await;
    let uri = format!("/projects/download/{}?path=a.txt", project.id);

    assert_eq!(app.get_raw(Some(&bob), &uri).await.0, StatusCode::NOT_FOUND);
    assert_eq!(app.get_raw(None, &uri).await.0, StatusCode::BAD_REQUEST);
    app.share::<Project>(project.id, Principal::User(bob.id), Role::Viewer)
        .await;
    assert_eq!(app.get_raw(Some(&bob), &uri).await.0, StatusCode::OK);
}

/// Missing path 404, `..` stays inside the project, symlinks are refused and left out of archives.
#[cfg(unix)]
#[tokio::test]
async fn paths_stay_confined() {
    let app = TestApp::new().await;
    let alice = app.user("alice").await;
    let project = project_with_files(&app, &alice).await;
    let root = app.state.config().projects_directory().join("p");
    let outside = tempfile::tempdir().unwrap();
    std::fs::write(outside.path().join("secret"), "secret").unwrap();
    std::os::unix::fs::symlink(outside.path(), root.join("link")).unwrap();
    std::os::unix::fs::symlink(outside.path().join("secret"), root.join("file-link")).unwrap();
    let download = |path: &str| format!("/projects/download/{}?path={path}", project.id);

    assert_eq!(
        app.get_raw(Some(&alice), &download("nope")).await.0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        app.get_raw(Some(&alice), &download("link/secret")).await.0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        app.get_raw(Some(&alice), &download("file-link")).await.0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        app.get_raw(Some(&alice), &download("../../../etc/passwd"))
            .await
            .0,
        StatusCode::NOT_FOUND
    );
    let (_, _, body) = app
        .get_raw(Some(&alice), &format!("/projects/download/{}", project.id))
        .await;
    let (files, _) = archive_entries(&body);
    assert!(files.iter().all(|(name, _)| !name.contains("link")));
}

/// The virtual reference folder inside a project is served from the reference data.
#[tokio::test]
async fn reference_folder_of_project_serves_reference_data() {
    let app = TestApp::new().await;
    let alice = app.user("alice").await;
    let project = project_with_files(&app, &alice).await;
    std::fs::write(
        app.state.reference_directory().get().join("genome.fa"),
        "ACGT",
    )
    .unwrap();

    let (status, _, body) = app
        .get_raw(
            Some(&alice),
            &format!(
                "/projects/download/{}?path={}/genome.fa",
                project.id,
                macworp_core::paths::REFERENCE_PREFIX
            ),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, b"ACGT");
}

/// A one-time token replaces the header once and only for downloads of its user's resources.
#[tokio::test]
async fn one_time_token_authenticates_a_single_download() {
    let app = TestApp::new().await;
    let alice = app.user("alice").await;
    let bob = app.user("bob").await;
    let project = project_with_files(&app, &alice).await;
    let uri = |token: &str| {
        format!(
            "/projects/download/{}?path=a.txt&one-time-token={token}",
            project.id
        )
    };

    let token = app.one_time_token(&alice).await;
    let (status, _, body) = app.get_raw(None, &uri(&token)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, b"alpha");
    // Used up
    assert_eq!(
        app.get_raw(None, &uri(&token)).await.0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        app.get_raw(None, &uri("made-up")).await.0,
        StatusCode::UNAUTHORIZED
    );
    // Authorization still applies to the user the token was issued for
    let bobs = app.one_time_token(&bob).await;
    assert_eq!(
        app.get_raw(None, &uri(&bobs)).await.0,
        StatusCode::NOT_FOUND
    );
}

/// A token does not work on endpoints that do not opt in, and issuing needs a login.
#[tokio::test]
async fn one_time_token_is_limited_to_downloads() {
    let app = TestApp::new().await;
    let alice = app.user("alice").await;
    let project = project_with_files(&app, &alice).await;

    let token = app.one_time_token(&alice).await;
    let (status, _) = app
        .send(
            None,
            http::Method::GET,
            &format!(
                "/projects/read-text/{}?path=a.txt&one-time-token={token}",
                project.id
            ),
            None,
        )
        .await;
    assert!(status == StatusCode::BAD_REQUEST || status == StatusCode::UNAUTHORIZED);
    let (status, _) = app
        .send(None, http::Method::POST, "/user/one-time-token", None)
        .await;
    assert!(status == StatusCode::BAD_REQUEST || status == StatusCode::UNAUTHORIZED);
}

/// Reference data downloads work for every user, as file and as folder.
#[tokio::test]
async fn reference_data_can_be_downloaded() {
    let app = TestApp::new().await;
    let alice = app.user("alice").await;
    let reference = app.state.reference_directory().get();
    std::fs::create_dir_all(reference.join("hg38")).unwrap();
    std::fs::write(reference.join("hg38/genome.fa"), "ACGT").unwrap();

    let (status, _, body) = app
        .get_raw(Some(&alice), "/reference/download/1?path=hg38/genome.fa")
        .await;
    assert_eq!((status, body.as_slice()), (StatusCode::OK, &b"ACGT"[..]));

    let token = app.one_time_token(&alice).await;
    let (status, _, body) = app
        .get_raw(
            None,
            &format!("/reference/download/1?path=hg38&one-time-token={token}"),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let (files, _) = archive_entries(&body);
    assert_eq!(files, [("hg38/genome.fa".to_string(), "ACGT".to_string())]);
    assert_eq!(
        app.get_raw(Some(&alice), "/reference/download/1?path=nope")
            .await
            .0,
        StatusCode::NOT_FOUND
    );
}
