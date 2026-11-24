use std::sync::{Arc, Mutex};

use http::{Method, StatusCode};
use macworp_core::runtime_status::{RuntimeStatus, StatusCheck, StatusLevel};
use serde_json::json;

use crate::{
    runtime_control::{BoxFuture, RuntimeControl},
    test_support::TestApp,
};

/// Status path of the container runtime routes.
const STATUS: &str = "/runtime/status";
/// Refresh path.
const REFRESH: &str = "/runtime/refresh";
/// Memory path.
const MEMORY: &str = "/runtime/memory";

/// A runtime that remembers what it was asked to do.
#[derive(Default)]
struct FakeRuntime {
    /// Memory values passed to `adjust_memory`
    adjusted: Mutex<Vec<u64>>,
    /// Number of `refresh` calls
    refreshed: Mutex<u32>,
    /// Error `adjust_memory` answers with
    failure: Option<&'static str>,
}

impl FakeRuntime {
    /// A status with one check named `name`.
    ///
    /// # Arguments
    /// * `name` - name of the check, tells the calls apart
    fn status_named(name: &str) -> RuntimeStatus {
        RuntimeStatus {
            kind: Some("podman".into()),
            version: Some("6.1.4".into()),
            rootless: Some(true),
            ok: true,
            checks: vec![StatusCheck {
                name: name.into(),
                level: StatusLevel::Ok,
                message: String::new(),
            }],
            unavailable_engines: vec![],
            memory_fix: None,
        }
    }
}

impl RuntimeControl for FakeRuntime {
    fn status(&self) -> BoxFuture<'_, RuntimeStatus> {
        Box::pin(async { Self::status_named("cached") })
    }

    fn refresh(&self) -> BoxFuture<'_, RuntimeStatus> {
        Box::pin(async {
            *self.refreshed.lock().unwrap() += 1;
            Self::status_named("fresh")
        })
    }

    fn adjust_memory(&self, mebibytes: u64) -> BoxFuture<'_, Result<RuntimeStatus, String>> {
        Box::pin(async move {
            self.adjusted.lock().unwrap().push(mebibytes);
            match self.failure {
                Some(message) => Err(message.to_string()),
                None => Ok(Self::status_named("adjusted")),
            }
        })
    }
}

/// Status and refresh forward to the runtime of the desktop.
#[tokio::test]
async fn status_and_refresh_forward_to_the_runtime() {
    let fake = Arc::new(FakeRuntime::default());
    let app = TestApp::new_with_runtime_control(fake.clone()).await;
    let alice = app.user("alice").await;
    let (status, body) = app.get(&alice, STATUS).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["checks"][0]["name"], json!("cached"));
    let (status, body) = app.post(&alice, REFRESH, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["checks"][0]["name"], json!("fresh"));
    assert_eq!(*fake.refreshed.lock().unwrap(), 1);
}

/// The memory is changed when no run is active, and the answer is the new status.
#[tokio::test]
async fn memory_is_changed_without_active_runs() {
    let fake = Arc::new(FakeRuntime::default());
    let app = TestApp::new_with_runtime_control(fake.clone()).await;
    let alice = app.user("alice").await;
    let (status, body) = app
        .post(&alice, MEMORY, Some(json!({"mebibytes": 8192})))
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["checks"][0]["name"], json!("adjusted"));
    assert_eq!(*fake.adjusted.lock().unwrap(), [8192]);
}

/// A queued or running run keeps the engine from being restarted (409), nothing is called.
#[tokio::test]
async fn memory_is_refused_while_a_run_is_active() {
    let fake = Arc::new(FakeRuntime::default());
    let app = TestApp::new_with_runtime_control(fake.clone()).await;
    let alice = app.user("alice").await;
    let project = app.project(&alice, "p").await;
    let workflow = app.workflow(&alice, "w").await;
    app.run(&project, &workflow).await;
    let (status, _) = app
        .post(&alice, MEMORY, Some(json!({"mebibytes": 8192})))
        .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert!(fake.adjusted.lock().unwrap().is_empty());
}

/// A failing change is reported (502); the reason is the plain-text body.
#[tokio::test]
async fn failed_change_is_reported() {
    let fake = Arc::new(FakeRuntime {
        failure: Some("podman machine start failed"),
        ..Default::default()
    });
    let app = TestApp::new_with_runtime_control(fake).await;
    let alice = app.user("alice").await;
    let (status, _) = app
        .post(&alice, MEMORY, Some(json!({"mebibytes": 8192})))
        .await;
    assert_eq!(status, StatusCode::BAD_GATEWAY);
}

/// Without a runtime provided by the app the routes answer 404, anonymous requests are rejected.
#[tokio::test]
async fn routes_need_a_runtime_and_a_user() {
    let app = TestApp::new().await;
    let alice = app.user("alice").await;
    let (status, _) = app.get(&alice, STATUS).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = app.send(None, Method::GET, STATUS, None).await;
    assert!(status == StatusCode::BAD_REQUEST || status == StatusCode::UNAUTHORIZED);
}
