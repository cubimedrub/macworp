use http::{Method, StatusCode};
use macworp_core::entities::RunStatus;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
use serde_json::{Value, json};

use crate::{
    models::{RunColumn, RunLogTable, RunTable},
    test_support::{TestApp, WORKER_PASSWORD},
};

/// Claims the next run as the worker and returns the response body
async fn claim(app: &TestApp) -> (StatusCode, Value) {
    app.send_as_worker(
        WORKER_PASSWORD,
        Method::POST,
        "/worker/claim",
        Some(json!({"worker_name": "w1"})),
    )
    .await
}

/// Calls a per-run endpoint with the run token
async fn with_token(
    app: &TestApp,
    token: &str,
    path: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    app.send_authorized(Some(format!("Run {token}")), Method::POST, path, body)
        .await
}

/// Whole protocol: claim returns the run with a token, heartbeat/logs/progress work with it,
/// finish stores the result and the token stops working.
#[tokio::test]
async fn claim_heartbeat_logs_progress_finish() {
    let app = TestApp::new().await;
    let alice = app.user("alice").await;
    let project = app.project(&alice, "p").await;
    let workflow = app.workflow(&alice, "w").await;
    let run = app.run(&project, &workflow).await;

    let (status, claimed) = claim(&app).await;
    assert_eq!(status, StatusCode::OK, "{claimed}");
    assert_eq!(claimed["run_id"], run.id);
    assert!(
        claimed["project_dir"]
            .as_str()
            .unwrap()
            .contains("project-p")
    );
    let token = claimed["token"].as_str().unwrap().to_string();

    // nothing else queued
    let (status, _) = claim(&app).await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let id = run.id;
    let (status, body) = with_token(&app, &token, &format!("/worker/heartbeat/{id}"), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["cancel_requested"], false);

    let (status, _) = with_token(
        &app,
        &token,
        &format!("/worker/logs/{id}"),
        Some(json!({"stream": "stdout", "lines": ["hello"]})),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let chunks = RunLogTable::list_after(app.state.db(), id, None, 10)
        .await
        .unwrap();
    assert_eq!(chunks.len(), 1);

    let (status, _) = with_token(
        &app,
        &token,
        &format!("/worker/progress/{id}"),
        Some(json!({"total": 4, "done": 2, "message": "step", "error": null})),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let stored = RunTable::find_by_id(id)
        .one(app.state.db())
        .await
        .unwrap()
        .unwrap();
    assert_eq!((stored.tasks_total, stored.tasks_done), (4, 2));

    let (status, _) = with_token(
        &app,
        &token,
        &format!("/worker/finish/{id}"),
        Some(json!({"outcome": "Succeeded"})),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let stored = RunTable::find_by_id(id)
        .one(app.state.db())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.status, RunStatus::Succeeded);

    // token is dead once the run is finished
    let (status, _) = with_token(&app, &token, &format!("/worker/heartbeat/{id}"), None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

/// Claim needs the worker credential: users, run tokens, wrong passwords and no header fail
#[tokio::test]
async fn claim_requires_worker_credential() {
    let app = TestApp::new().await;
    let alice = app.user("alice").await;
    let project = app.project(&alice, "p").await;
    let workflow = app.workflow(&alice, "w").await;
    app.run(&project, &workflow).await;

    let body = Some(json!({"worker_name": "w1"}));
    let (status, _) = app
        .send(Some(&alice), Method::POST, "/worker/claim", body.clone())
        .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, _) = app
        .send_as_worker("wrong", Method::POST, "/worker/claim", body.clone())
        .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, _) = app
        .send(None, Method::POST, "/worker/claim", body.clone())
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, _) = with_token(&app, "whatever", "/worker/claim", body).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    // the run was never taken
    let queued = RunTable::find()
        .filter(RunColumn::Status.eq(RunStatus::Queued))
        .all(app.state.db())
        .await
        .unwrap();
    assert_eq!(queued.len(), 1);
}

/// A run token only opens its own run; user JWTs and the worker credential are rejected on
/// per-run endpoints
#[tokio::test]
async fn run_token_is_scoped_to_its_run() {
    let app = TestApp::new().await;
    let alice = app.user("alice").await;
    let project = app.project(&alice, "p").await;
    let workflow = app.workflow(&alice, "w").await;
    let first = app.run(&project, &workflow).await;
    let second = app.run(&project, &workflow).await;

    let (_, claimed) = claim(&app).await;
    assert_eq!(claimed["run_id"], first.id);
    let token = claimed["token"].as_str().unwrap().to_string();
    let (_, other) = claim(&app).await;
    assert_eq!(other["run_id"], second.id);

    // first run's token on the second run
    let (status, _) = with_token(
        &app,
        &token,
        &format!("/worker/heartbeat/{}", second.id),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    // JWT and worker credential on a per-run endpoint
    let uri = format!("/worker/heartbeat/{}", first.id);
    let (status, _) = app.send(Some(&alice), Method::POST, &uri, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, _) = app
        .send_as_worker(WORKER_PASSWORD, Method::POST, &uri, None)
        .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

/// A cancel request reaches the worker through the heartbeat
#[tokio::test]
async fn heartbeat_reports_cancel_request() {
    let app = TestApp::new().await;
    let alice = app.user("alice").await;
    let project = app.project(&alice, "p").await;
    let workflow = app.workflow(&alice, "w").await;
    let run = app.run(&project, &workflow).await;
    let (_, claimed) = claim(&app).await;
    let token = claimed["token"].as_str().unwrap().to_string();

    let (status, _) = app
        .post(&alice, &format!("/runs/cancel/{}", run.id), None)
        .await;
    assert!(status.is_success(), "{status}");
    let (status, body) =
        with_token(&app, &token, &format!("/worker/heartbeat/{}", run.id), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["cancel_requested"], true);

    let (status, _) = with_token(
        &app,
        &token,
        &format!("/worker/finish/{}", run.id),
        Some(json!({"outcome": "Cancelled"})),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
}

/// After the lease expired and the run was requeued, the first worker's token is dead and a
/// second claim works
#[tokio::test]
async fn requeued_run_rejects_stale_worker() {
    let app = TestApp::new().await;
    let alice = app.user("alice").await;
    let project = app.project(&alice, "p").await;
    let workflow = app.workflow(&alice, "w").await;
    let run = app.run(&project, &workflow).await;
    let (_, claimed) = claim(&app).await;
    let stale = claimed["token"].as_str().unwrap().to_string();

    RunTable::update_many()
        .col_expr(
            RunColumn::LeasedUntil,
            sea_orm::sea_query::Expr::value(chrono::Utc::now() - chrono::Duration::seconds(5)),
        )
        .filter(RunColumn::Id.eq(run.id))
        .exec(app.state.db())
        .await
        .unwrap();
    RunTable::requeue_expired(app.state.db(), 3).await.unwrap();

    let uri = format!("/worker/finish/{}", run.id);
    let (status, _) = with_token(&app, &stale, &uri, Some(json!({"outcome": "Succeeded"}))).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let (status, again) = claim(&app).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(again["run_id"], run.id);
    assert_ne!(again["token"].as_str().unwrap(), stale);
}

/// A run whose project has no directory is failed instead of being handed out
#[tokio::test]
async fn claim_fails_runs_of_projects_without_directory() {
    let app = TestApp::new().await;
    let alice = app.user("alice").await;
    let project = app.project(&alice, "p").await;
    let workflow = app.workflow(&alice, "w").await;
    let run = app.run(&project, &workflow).await;
    crate::models::ProjectTable::update_many()
        .col_expr(
            crate::models::ProjectColumn::Path,
            sea_orm::sea_query::Expr::value(None::<String>),
        )
        .exec(app.state.db())
        .await
        .unwrap();

    let (status, _) = claim(&app).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let stored = RunTable::find_by_id(run.id)
        .one(app.state.db())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.status, RunStatus::Failed);
}
