use http::{Method, StatusCode};
use macworp_core::entities::{FailureReason, RunStatus};
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
        Some(
            json!({"worker_name": "w1", "engines": ["Nextflow", "Snakemake", "Cwl", "Container"]}),
        ),
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

/// A worker reporting a failure stores status `Failed` with the given reason, exit code and error;
/// without a reason the backend falls back to `exit_code`.
#[tokio::test]
async fn finish_failed_stores_reason() {
    let app = TestApp::new().await;
    let alice = app.user("alice").await;
    let project = app.project(&alice, "p").await;
    let workflow = app.workflow(&alice, "w").await;
    let first = app.run(&project, &workflow).await;
    let second = app.run(&project, &workflow).await;

    for (expected_id, outcome, reason) in [
        (
            first.id,
            json!({"Failed": {"exit_code": null, "error": "Killed by signal 9", "reason": "signal"}}),
            FailureReason::Signal,
        ),
        (
            second.id,
            json!({"Failed": {"exit_code": 2, "error": null}}),
            FailureReason::ExitCode,
        ),
    ] {
        let (status, claimed) = claim(&app).await;
        assert_eq!(status, StatusCode::OK, "{claimed}");
        assert_eq!(claimed["run_id"], expected_id);
        let token = claimed["token"].as_str().unwrap();
        let (status, _) = with_token(
            &app,
            token,
            &format!("/worker/finish/{expected_id}"),
            Some(json!({ "outcome": outcome })),
        )
        .await;
        assert_eq!(status, StatusCode::NO_CONTENT);
        let stored = RunTable::find_by_id(expected_id)
            .one(app.state.db())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(stored.status, RunStatus::Failed);
        assert_eq!(stored.failure_reason, Some(reason));
    }
}

/// Claim needs the worker credential: users, run tokens, wrong passwords and no header fail
#[tokio::test]
async fn claim_requires_worker_credential() {
    let app = TestApp::new().await;
    let alice = app.user("alice").await;
    let project = app.project(&alice, "p").await;
    let workflow = app.workflow(&alice, "w").await;
    app.run(&project, &workflow).await;

    let body = Some(
        json!({"worker_name": "w1", "engines": ["Nextflow", "Snakemake", "Cwl", "Container"]}),
    );
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

/// Announces a worker with the given engines through the API
async fn announce(app: &TestApp, name: &str, engines: Value) -> (StatusCode, Value) {
    app.send_as_worker(
        WORKER_PASSWORD,
        Method::POST,
        "/worker/announce",
        Some(json!({
            "worker_name": name,
            "runtime": "podman",
            "runtime_version": "5.0",
            "engines": engines,
        })),
    )
    .await
}

/// Engines `/utilities/engines` reports as unavailable, plus the number of online workers
async fn engines(app: &TestApp, user: &crate::models::User) -> (Vec<String>, u64) {
    let (status, body) = app.get(user, "/utilities/engines").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let unavailable = body["unavailable"]
        .as_array()
        .unwrap()
        .iter()
        .map(|engine| engine.as_str().unwrap().to_string())
        .collect();
    (unavailable, body["workers_online"].as_u64().unwrap())
}

/// Without workers every engine is unavailable and starting a run is refused; announcing
/// workers frees the union of their engines.
#[tokio::test]
async fn announced_workers_define_available_engines() {
    let app = TestApp::new().await;
    let alice = app.user("alice").await;
    let project = app.project(&alice, "p").await;
    let workflow = app.workflow(&alice, "w").await;
    let start = format!("/runs/start/{}", project.id);
    let body = json!({"workflow_id": workflow.id, "comment": "", "parameters": []});

    let (unavailable, online) = engines(&app, &alice).await;
    assert_eq!(unavailable.len(), 4);
    assert_eq!(online, 0);
    let (status, _) = app.post(&alice, &start, Some(body.clone())).await;
    assert_eq!(status, StatusCode::CONFLICT);

    let (status, response) = announce(&app, "w1", json!(["Nextflow", "Container"])).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        response["ttl_seconds"],
        app.state.config().worker_lease_seconds()
    );
    let (mut unavailable, online) = engines(&app, &alice).await;
    unavailable.sort();
    assert_eq!(unavailable, ["Cwl", "Snakemake"]);
    assert_eq!(online, 1);

    // A second worker adds its engines
    announce(&app, "w2", json!(["Snakemake"])).await;
    let (unavailable, online) = engines(&app, &alice).await;
    assert_eq!(unavailable, ["Cwl"]);
    assert_eq!(online, 2);
}

/// Announce accepts only the worker credential, the engines list only users
#[tokio::test]
async fn announce_and_engines_authentication() {
    let app = TestApp::new().await;
    let alice = app.user("alice").await;
    let request =
        json!({"worker_name": "w", "runtime": "podman", "runtime_version": null, "engines": []});

    let (status, _) = app
        .post(&alice, "/worker/announce", Some(request.clone()))
        .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, _) = app
        .send_authorized(
            Some("Run abc".into()),
            Method::POST,
            "/worker/announce",
            Some(request),
        )
        .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let (status, _) = app
        .send_as_worker(WORKER_PASSWORD, Method::GET, "/utilities/engines", None)
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _) = app
        .send(None, Method::GET, "/utilities/engines", None)
        .await;
    assert_ne!(status, StatusCode::OK);
}

/// A list fixed by the embedding application (desktop) wins over announcements
#[tokio::test]
async fn fixed_engine_list_overrides_workers() {
    use macworp_core::workflow_configuration::engine::Engine;
    // Own state, the router of `TestApp` has none fixed
    let mut configuration = crate::configuration::Configuration::default();
    *configuration.fixed_unavailable_engines_mut() = Some(vec![Engine::Cwl]);
    let directory = tempfile::tempdir().unwrap();
    *configuration.database_url_mut() =
        format!("sqlite://{}/fixed.db?mode=rwc", directory.path().display());
    let state = crate::state::State::new_for_integration(configuration)
        .await
        .unwrap();
    state
        .workers()
        .announce("w/x", vec![Engine::Nextflow], std::time::Instant::now());
    assert_eq!(state.unavailable_engines(), vec![Engine::Cwl]);
}

/// A worker is only leased runs of engines it offers; the run waits for one that can run it
#[tokio::test]
async fn claim_skips_runs_of_other_engines() {
    let app = TestApp::new().await;
    let alice = app.user("alice").await;
    let project = app.project(&alice, "p").await;
    let workflow = app.workflow(&alice, "w").await;
    let run = app.run(&project, &workflow).await;
    let engine = serde_json::to_value(run.configuration.engine()).unwrap();
    let other = if engine == json!("Snakemake") {
        json!("Container")
    } else {
        json!("Snakemake")
    };
    let claim_with = |engines: Value| {
        app.send_as_worker(
            WORKER_PASSWORD,
            Method::POST,
            "/worker/claim",
            Some(json!({"worker_name": "w1", "engines": engines})),
        )
    };

    let (status, _) = claim_with(json!([other])).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, _) = claim_with(json!([])).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let stored = RunTable::find_by_id(run.id)
        .one(app.state.db())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.status, RunStatus::Queued);

    let (status, body) = claim_with(json!([other, engine])).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["run_id"], run.id);
}

/// Paths below the storage root reach the worker relative to it, so a worker that mounts the
/// storage elsewhere resolves them itself; paths outside the storage stay absolute.
#[tokio::test]
async fn claim_sends_storage_relative_paths() {
    let app = TestApp::new().await;
    let alice = app.user("alice").await;
    let workflow = app.workflow(&alice, "w").await;
    let inside = app.project_in_storage(&alice, "inside").await;
    let outside = app.project(&alice, "outside").await;
    let storage = app.state.config().storage_directory();
    app.state
        .reference_directory()
        .set(storage.join("global_reference_data"));
    app.run(&outside, &workflow).await;
    app.run(&inside, &workflow).await;

    let (status, first) = claim(&app).await;
    assert_eq!(status, StatusCode::OK, "{first}");
    assert!(
        std::path::Path::new(first["project_dir"].as_str().unwrap()).is_absolute(),
        "outside the storage: absolute"
    );
    assert_eq!(first["reference_dir"], "global_reference_data");

    let (status, second) = claim(&app).await;
    assert_eq!(status, StatusCode::OK, "{second}");
    assert_eq!(second["project_dir"], "projects/inside");
}

/// A `Path` parameter with a file pattern: saving an invalid pattern is refused with 400, and a
/// start request whose value does not match the pattern is refused with 400 as well, while a
/// matching value passes the parameter check.
#[tokio::test]
async fn path_patterns_are_enforced() {
    use macworp_core::workflow_configuration::{
        WorkflowConfiguration,
        parameters::{Parameter, path::Path},
    };

    let app = TestApp::new().await;
    let alice = app.user("alice").await;
    let project = app.project(&alice, "p").await;
    let workflow = app.workflow(&alice, "w").await;
    announce(&app, "w1", json!(["Nextflow"])).await;
    let update = format!("/workflow-integrations/update/{}", workflow.id);

    let mut path = Path::new("reads".into(), "Reads".into(), String::new());
    *path.is_excepting_files_mut() = true;
    path.set_file_pattern("(");
    let mut configuration = WorkflowConfiguration::default();
    configuration
        .dynamic_parameters_mut()
        .push(Parameter::Path(path.clone()));
    let body = |configuration: &WorkflowConfiguration| json!({"name": "w", "description": "", "configuration": configuration});
    let (status, _) = app.post(&alice, &update, Some(body(&configuration))).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    path.set_file_pattern(r".*\.(fastq|fq)(\.gz)?");
    *configuration.dynamic_parameters_mut() = vec![Parameter::Path(path)];
    let (status, response) = app.post(&alice, &update, Some(body(&configuration))).await;
    assert_eq!(status, StatusCode::OK, "{response}");

    let start = format!("/runs/start/{}", project.id);
    let start_with = |value: &str| {
        json!({
            "workflow_id": workflow.id,
            "comment": "",
            "parameters": [{"name": "reads", "value": [value]}],
        })
    };
    let (status, response) = app
        .post(&alice, &start, Some(start_with("a/reads.txt")))
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{response}");
    let (status, response) = app
        .post(&alice, &start, Some(start_with("a/reads.fq.gz")))
        .await;
    assert_ne!(status, StatusCode::BAD_REQUEST, "{response}");
}

/// A run reads the reference data as it was when the run was started: files replaced, deleted or
/// added afterwards are not in what the worker gets, the snapshot is removed after the run
/// ended, and a start that is not stored leaves no snapshot behind.
#[tokio::test]
async fn run_gets_a_snapshot_of_the_reference_data() {
    let app = TestApp::new_with_reference_snapshots().await;
    let alice = app.user("alice").await;
    let project = app.project(&alice, "p").await;
    let workflow = app.workflow(&alice, "w").await;
    announce(
        &app,
        "w1",
        json!(["Nextflow", "Container", "Snakemake", "Cwl"]),
    )
    .await;
    let reference = app.state.reference_directory().get();
    std::fs::write(reference.join("genome.fa"), "old").unwrap();
    std::fs::write(reference.join("genome.fa.fai"), "old index").unwrap();

    let (status, response) = app
        .post(
            &alice,
            &format!("/runs/start/{}", project.id),
            Some(json!({"workflow_id": workflow.id, "comment": "", "parameters": []})),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{response}");
    // The fingerprint describes the reference data at start and stays with the run
    let fingerprint = &response["reference_fingerprint"];
    assert_eq!(fingerprint["file_count"], 2, "{response}");
    assert_eq!(fingerprint["total_bytes"], 3 + 9);
    assert!(fingerprint["last_modified"].is_string());
    assert!(fingerprint["snapshot_at"].is_string());

    // An admin replaces one file, deletes the other and adds a new one
    std::fs::write(reference.join("genome.fa.new"), "new").unwrap();
    std::fs::rename(reference.join("genome.fa.new"), reference.join("genome.fa")).unwrap();
    std::fs::remove_file(reference.join("genome.fa.fai")).unwrap();
    std::fs::write(reference.join("added.txt"), "added").unwrap();

    let (status, claimed) = claim(&app).await;
    assert_eq!(status, StatusCode::OK, "{claimed}");
    let snapshot = std::path::PathBuf::from(claimed["reference_dir"].as_str().unwrap());
    assert!(snapshot.is_absolute());
    assert_ne!(
        snapshot, reference,
        "the worker must not get the live folder"
    );
    assert_eq!(std::fs::read(snapshot.join("genome.fa")).unwrap(), b"old");
    assert_eq!(
        std::fs::read(snapshot.join("genome.fa.fai")).unwrap(),
        b"old index"
    );
    assert!(!snapshot.join("added.txt").exists());

    // Referenced while the run is active, removed once it is over
    let grace = std::time::Duration::ZERO;
    assert_eq!(app.state.sweep_reference_snapshots(grace).await.unwrap(), 0);
    assert!(snapshot.exists());
    RunTable::finish(
        app.state.db(),
        claimed["run_id"].as_i64().unwrap(),
        RunStatus::Succeeded,
        Some(0),
        None,
        None,
    )
    .await
    .unwrap();
    assert_eq!(app.state.sweep_reference_snapshots(grace).await.unwrap(), 1);
    assert!(!snapshot.exists());
}

/// Runs started at different times each read the reference data of their own start, however
/// long they wait in the queue: the first run, claimed after the data changed twice, still gets
/// the first state, the second run the second.
#[tokio::test]
async fn queued_runs_keep_the_reference_data_of_their_start() {
    let app = TestApp::new_with_reference_snapshots().await;
    let alice = app.user("alice").await;
    let project = app.project(&alice, "p").await;
    let workflow = app.workflow(&alice, "w").await;
    announce(
        &app,
        "w1",
        json!(["Nextflow", "Container", "Snakemake", "Cwl"]),
    )
    .await;
    let reference = app.state.reference_directory().get();
    let start = format!("/runs/start/{}", project.id);
    let body = json!({"workflow_id": workflow.id, "comment": "", "parameters": []});

    std::fs::write(reference.join("version.txt"), "one").unwrap();
    let (status, first) = app.post(&alice, &start, Some(body.clone())).await;
    assert_eq!(status, StatusCode::OK, "{first}");

    std::fs::write(reference.join("version.txt.new"), "two").unwrap();
    std::fs::rename(
        reference.join("version.txt.new"),
        reference.join("version.txt"),
    )
    .unwrap();
    let (status, second) = app.post(&alice, &start, Some(body)).await;
    assert_eq!(status, StatusCode::OK, "{second}");

    std::fs::write(reference.join("version.txt.new"), "three").unwrap();
    std::fs::rename(
        reference.join("version.txt.new"),
        reference.join("version.txt"),
    )
    .unwrap();

    // Claimed oldest first, after the data changed twice since the first start
    let (status, claimed_first) = claim(&app).await;
    assert_eq!(status, StatusCode::OK, "{claimed_first}");
    let (status, claimed_second) = claim(&app).await;
    assert_eq!(status, StatusCode::OK, "{claimed_second}");
    assert_eq!(claimed_first["run_id"], first["id"]);
    assert_eq!(claimed_second["run_id"], second["id"]);

    let first_dir = std::path::PathBuf::from(claimed_first["reference_dir"].as_str().unwrap());
    let second_dir = std::path::PathBuf::from(claimed_second["reference_dir"].as_str().unwrap());
    assert_ne!(first_dir, second_dir);
    assert_eq!(
        std::fs::read(first_dir.join("version.txt")).unwrap(),
        b"one"
    );
    assert_eq!(
        std::fs::read(second_dir.join("version.txt")).unwrap(),
        b"two"
    );
    assert_eq!(
        std::fs::read(reference.join("version.txt")).unwrap(),
        b"three"
    );
}

/// Starts a run of `workflow` on `project`, optionally resuming `resume_from_run`, and returns
/// the status and body of the response.
///
/// # Arguments
/// * `app` - The test application
/// * `user` - Who starts the run
/// * `project_id` - Project to run on
/// * `workflow_id` - Workflow to run
/// * `resume_from_run` - Run to continue, `None` for a fresh run
async fn start_run(
    app: &TestApp,
    user: &crate::models::User,
    project_id: i64,
    workflow_id: i64,
    resume_from_run: Option<i64>,
) -> (StatusCode, Value) {
    app.post(
        user,
        &format!("/runs/start/{project_id}"),
        Some(json!({
            "workflow_id": workflow_id,
            "comment": "",
            "parameters": [],
            "resume_from_run": resume_from_run,
        })),
    )
    .await
}

/// Claims the next run and ends it with `status`, returns the run id and the reference folder
/// the worker was given.
///
/// # Arguments
/// * `app` - The test application
/// * `status` - Terminal status to store
async fn claim_and_finish(app: &TestApp, status: RunStatus) -> (i64, std::path::PathBuf) {
    let (code, claimed) = claim(app).await;
    assert_eq!(code, StatusCode::OK, "{claimed}");
    let run_id = claimed["run_id"].as_i64().unwrap();
    RunTable::finish(app.state.db(), run_id, status, Some(1), None, None)
        .await
        .unwrap();
    (
        run_id,
        std::path::PathBuf::from(claimed["reference_dir"].as_str().unwrap()),
    )
}

/// Announces a worker that runs every engine, so claims succeed.
///
/// # Arguments
/// * `app` - The test application
async fn announce_all_engines(app: &TestApp) {
    announce(
        app,
        "w1",
        json!(["Nextflow", "Container", "Snakemake", "Cwl"]),
    )
    .await;
}

/// A failed run is resumed on the snapshot it started with, however the reference data changed
/// since. The engine's resume parameter is added. The snapshot is kept while the failed run is
/// the newest one and released when the resumed run succeeds.
#[tokio::test]
async fn resume_reuses_the_snapshot_of_the_failed_run() {
    let app = TestApp::new_with_reference_snapshots().await;
    let alice = app.user("alice").await;
    let project = app.project(&alice, "p").await;
    let workflow = app.workflow(&alice, "w").await;
    announce_all_engines(&app).await;
    let reference = app.state.reference_directory().get();
    std::fs::write(reference.join("genome.fa"), "old").unwrap();
    let grace = std::time::Duration::ZERO;

    let (status, first) = start_run(&app, &alice, project.id, workflow.id, None).await;
    assert_eq!(status, StatusCode::OK, "{first}");
    let (first_id, first_snapshot) = claim_and_finish(&app, RunStatus::Failed).await;

    // The failed run is the newest of its project: its snapshot survives the sweep
    std::fs::write(reference.join("genome.fa.new"), "new").unwrap();
    std::fs::rename(reference.join("genome.fa.new"), reference.join("genome.fa")).unwrap();
    assert_eq!(app.state.sweep_reference_snapshots(grace).await.unwrap(), 0);
    assert!(first_snapshot.exists());

    let (status, second) = start_run(&app, &alice, project.id, workflow.id, Some(first_id)).await;
    assert_eq!(status, StatusCode::OK, "{second}");
    assert_eq!(
        second["reference_fingerprint"], first["reference_fingerprint"],
        "the resumed run reports the data of the first start"
    );
    let stored = RunTable::find_by_id(second["id"].as_i64().unwrap())
        .one(app.state.db())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        stored
            .configuration
            .engine_parameters()
            .iter()
            .map(|parameter| parameter.name())
            .collect::<Vec<_>>(),
        ["resume"]
    );

    let (second_id, second_snapshot) = claim_and_finish(&app, RunStatus::Failed).await;
    assert_eq!(second_id, second["id"].as_i64().unwrap());
    assert_eq!(second_snapshot, first_snapshot);
    assert_eq!(
        std::fs::read(second_snapshot.join("genome.fa")).unwrap(),
        b"old"
    );

    // Resumed once more, then it succeeds: nothing is left to resume, the snapshot goes
    let (status, third) = start_run(&app, &alice, project.id, workflow.id, Some(second_id)).await;
    assert_eq!(status, StatusCode::OK, "{third}");
    assert_eq!(app.state.sweep_reference_snapshots(grace).await.unwrap(), 0);
    claim_and_finish(&app, RunStatus::Succeeded).await;
    assert_eq!(app.state.sweep_reference_snapshots(grace).await.unwrap(), 1);
    assert!(!first_snapshot.exists());
}

/// Only the newest run of a project keeps its snapshot after it failed: a fresh run after a
/// failure releases the old snapshot, and a successful run keeps nothing.
#[tokio::test]
async fn only_the_newest_failed_run_keeps_its_snapshot() {
    let app = TestApp::new_with_reference_snapshots().await;
    let alice = app.user("alice").await;
    let project = app.project(&alice, "p").await;
    let other_project = app.project(&alice, "q").await;
    let workflow = app.workflow(&alice, "w").await;
    announce_all_engines(&app).await;
    std::fs::write(app.state.reference_directory().get().join("a.txt"), "a").unwrap();
    let grace = std::time::Duration::ZERO;

    // A failed run of each project, both newest in their project
    for id in [project.id, other_project.id] {
        let (status, run) = start_run(&app, &alice, id, workflow.id, None).await;
        assert_eq!(status, StatusCode::OK, "{run}");
        claim_and_finish(&app, RunStatus::Failed).await;
    }
    assert_eq!(app.state.sweep_reference_snapshots(grace).await.unwrap(), 0);

    // A fresh run on the first project: its failed predecessor is no longer resumable
    let (status, run) = start_run(&app, &alice, project.id, workflow.id, None).await;
    assert_eq!(status, StatusCode::OK, "{run}");
    assert_eq!(app.state.sweep_reference_snapshots(grace).await.unwrap(), 1);
    let (_, snapshot) = claim_and_finish(&app, RunStatus::Succeeded).await;
    // The second project keeps its failed run's snapshot, the successful run keeps none
    assert_eq!(app.state.sweep_reference_snapshots(grace).await.unwrap(), 1);
    assert!(!snapshot.exists());
    let remaining = std::fs::read_dir(app.state.reference_snapshots().unwrap().root())
        .unwrap()
        .count();
    assert_eq!(remaining, 1);
}

/// Resuming is refused for runs that cannot continue: unfinished, succeeded, not the newest of
/// the project, of another workflow, of another project, or without their snapshot.
#[tokio::test]
async fn resume_is_refused_where_it_cannot_continue() {
    let app = TestApp::new_with_reference_snapshots().await;
    let alice = app.user("alice").await;
    let project = app.project(&alice, "p").await;
    let other_project = app.project(&alice, "q").await;
    let workflow = app.workflow(&alice, "w").await;
    let other_workflow = app.workflow(&alice, "v").await;
    announce_all_engines(&app).await;

    // Still queued
    let (_, queued) = start_run(&app, &alice, project.id, workflow.id, None).await;
    let queued_id = queued["id"].as_i64().unwrap();
    let (status, _) = start_run(&app, &alice, project.id, workflow.id, Some(queued_id)).await;
    assert_eq!(status, StatusCode::CONFLICT);

    // Succeeded
    claim_and_finish(&app, RunStatus::Succeeded).await;
    let (status, _) = start_run(&app, &alice, project.id, workflow.id, Some(queued_id)).await;
    assert_eq!(status, StatusCode::CONFLICT);

    // Failed, but another workflow, another project, or an unknown run
    let (_, failed) = start_run(&app, &alice, project.id, workflow.id, None).await;
    let failed_id = failed["id"].as_i64().unwrap();
    let (_, snapshot) = claim_and_finish(&app, RunStatus::Failed).await;
    let (status, _) =
        start_run(&app, &alice, project.id, other_workflow.id, Some(failed_id)).await;
    assert_eq!(status, StatusCode::CONFLICT);
    let (status, _) =
        start_run(&app, &alice, other_project.id, workflow.id, Some(failed_id)).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = start_run(&app, &alice, project.id, workflow.id, Some(9999)).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // Its snapshot vanished
    std::fs::rename(&snapshot, snapshot.with_extension("moved")).unwrap();
    let (status, body) = start_run(&app, &alice, project.id, workflow.id, Some(failed_id)).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    std::fs::rename(snapshot.with_extension("moved"), &snapshot).unwrap();

    // A newer run exists
    let (status, newer) = start_run(&app, &alice, project.id, workflow.id, None).await;
    assert_eq!(status, StatusCode::OK, "{newer}");
    let (status, _) = start_run(&app, &alice, project.id, workflow.id, Some(failed_id)).await;
    assert_eq!(status, StatusCode::CONFLICT);
}

/// A resumed run uses the parameter values the client sends, so a user can correct the values
/// that made the first run fail; the snapshot and the resume parameter still apply.
#[tokio::test]
async fn resume_takes_the_parameter_values_as_sent() {
    use macworp_core::workflow_configuration::{
        WorkflowConfiguration,
        parameters::{Parameter, text::Text},
    };

    let app = TestApp::new_with_reference_snapshots().await;
    let alice = app.user("alice").await;
    let project = app.project(&alice, "p").await;
    let workflow = app.workflow(&alice, "w").await;
    announce_all_engines(&app).await;
    let mut configuration = WorkflowConfiguration::default();
    configuration
        .dynamic_parameters_mut()
        .push(Parameter::Text(Text::new(
            "sample".into(),
            "Sample".into(),
            String::new(),
        )));
    let (status, response) = app
        .post(
            &alice,
            &format!("/workflow-integrations/update/{}", workflow.id),
            Some(json!({"name": "w", "description": "", "configuration": configuration})),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{response}");

    let (status, first) = app
        .post(
            &alice,
            &format!("/runs/start/{}", project.id),
            Some(json!({
                "workflow_id": workflow.id,
                "comment": "",
                "parameters": [{"name": "sample", "value": "s1"}],
            })),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{first}");
    let (first_id, _) = claim_and_finish(&app, RunStatus::Failed).await;

    // The client sends a corrected value, the resumed run gets it
    let (status, second) = app
        .post(
            &alice,
            &format!("/runs/start/{}", project.id),
            Some(json!({
                "workflow_id": workflow.id,
                "comment": "",
                "parameters": [{"name": "sample", "value": "other"}],
                "resume_from_run": first_id,
            })),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{second}");
    let stored = RunTable::find_by_id(second["id"].as_i64().unwrap())
        .one(app.state.db())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.parameters, json!({"sample": "other"}));
    assert_eq!(
        second["reference_fingerprint"], first["reference_fingerprint"],
        "still the reference data of the first start"
    );
}
