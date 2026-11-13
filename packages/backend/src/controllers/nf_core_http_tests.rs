use http::{Method, StatusCode};
use serde_json::json;

use crate::test_support::TestApp;

/// Puts a fresh index copy where the backend looks for it, so no request leaves the test.
fn seed_index(app: &TestApp) {
    let directory = app.directory.path().join("projects").join("workflows");
    std::fs::create_dir_all(&directory).unwrap();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let snapshot = json!({
        "fetched_at": now,
        "pipelines": [
            {"name": "demo", "description": "Demo", "archived": false, "releases": [
                {"tag": "1.1.0", "published_at": "2026-01-01T00:00:00Z", "has_schema": true},
                {"tag": "1.0.0", "published_at": "2025-01-01T00:00:00Z", "has_schema": true}
            ]}
        ]
    });
    std::fs::write(
        directory.join("nf_core_index.json"),
        serde_json::to_vec(&snapshot).unwrap(),
    )
    .unwrap();
}

#[tokio::test]
async fn pipelines_and_releases_are_listed() {
    let app = TestApp::new().await;
    seed_index(&app);
    let alice = app.user("alice").await;

    let (status, body) = app.get(&alice, "/utilities/nf-core/pipelines").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["pipelines"][0]["name"], "demo");
    assert_eq!(body["stale"], false);
    assert!(body["pipelines"][0].get("releases").is_none());

    let (status, body) = app
        .get(&alice, "/utilities/nf-core/pipelines/demo/releases")
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["releases"][0]["tag"], "1.1.0");
    assert_eq!(body["releases"][1]["tag"], "1.0.0");
}

#[tokio::test]
async fn unknown_pipeline_is_not_found() {
    let app = TestApp::new().await;
    seed_index(&app);
    let alice = app.user("alice").await;
    let (status, _) = app
        .get(&alice, "/utilities/nf-core/pipelines/nope/releases")
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn endpoints_need_a_user() {
    let app = TestApp::new().await;
    seed_index(&app);
    for uri in [
        "/utilities/nf-core/pipelines",
        "/utilities/nf-core/pipelines/demo/releases",
    ] {
        let (status, _) = app.send(None, Method::GET, uri, None).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{uri}");
        let (status, _) = app
            .send_as_worker(crate::test_support::WORKER_PASSWORD, Method::GET, uri, None)
            .await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{uri}");
    }
}

#[tokio::test]
async fn unreachable_index_without_copy_is_unavailable() {
    let app = TestApp::new_with(|configuration| {
        *configuration.nf_core_index_url_mut() = "http://127.0.0.1:1/pipelines.json".to_string();
    })
    .await;
    let alice = app.user("alice").await;
    let (status, _) = app.get(&alice, "/utilities/nf-core/pipelines").await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
}
