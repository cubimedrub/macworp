//! Full stack test: backend + SQLite + embedded worker + a real Nextflow engine container.
//!
//! Uses every parameter type as fixed and as dynamic parameter, runs
//! `tests/fixtures/end_to_end/workflow`, and checks the files the workflow wrote.
//!
//! Needs podman or docker and the `macworp-nextflow:latest` image (see
//! `docker/engine-nextflow.dockerfile`), so it is ignored by default:
//!
//! ```text
//! cargo test -p macworp_desktop end_to_end_test -- --ignored --nocapture
//! ```
//!
//! `MACWORP_CONTAINER_RUNTIME=podman|docker` enforces the container runtime (as for the app and
//! the worker binary), by default podman is preferred over docker.
//!
//! The project lives in a temporary directory below `$MACWORP_E2E_DIR` (default: the home
//! directory), because VM based runtimes on macOS only share the home directory with containers.

use std::{
    fs,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use clap::ValueEnum;
use macworp_backend::{filesystems::local_filesystem::LocalFilesystem, Backend};
use macworp_core::{
    entities::RunStatus,
    requests::ParameterValue,
    responses::RunResponse,
    workflow_configuration::{engine::Engine, WorkflowConfiguration},
};
use macworp_ui_core::client::Client;
use macworp_worker::{container_runtime::RuntimePreference, executors::nextflow::DEFAULT_IMAGE};
use serde_json::{json, Value};

const USER_NAME: &str = "admin";
const SEED: &str = "seed";
const RUN_TIMEOUT: Duration = Duration::from_secs(300);

/// Values of one parameter set, used for the fixed parameters and as defaults of the dynamic ones
struct Values {
    text: &'static str,
    multiline: &'static str,
    integer: i64,
    float: f64,
    glob: &'static str,
    file: &'static str,
    files: [&'static str; 2],
    folder: &'static str,
    folders: [&'static str; 2],
    select: &'static str,
    multiselect: [&'static str; 2],
}

const FIXED: Values = Values {
    text: "fixed single line",
    multiline: "fixed line one\nfixed line two",
    integer: 7,
    float: 2.5,
    glob: "data/fixed/*.txt",
    file: "data/fixed_file.txt",
    files: ["data/fixed_files/a.txt", "data/fixed_files/b.txt"],
    folder: "fixed_folder",
    folders: ["fixed_folders/one", "fixed_folders/two"],
    select: "beta",
    multiselect: ["x", "z"],
};

const DYNAMIC_DEFAULTS: Values = Values {
    text: "dyn default text",
    multiline: "dyn line one\ndyn line two",
    integer: 3,
    float: 0.25,
    glob: "data/dyn/*.txt",
    file: "data/dyn_file.txt",
    files: ["data/dyn_files/a.txt", "data/dyn_files/b.txt"],
    folder: "dyn_folder",
    folders: ["dyn_folders/one", "dyn_folders/two"],
    select: "alpha",
    multiselect: ["x", "y"],
};

fn text(name: &str, is_multiline: bool, value: &str) -> Value {
    json!({"Text": {"name": name, "label": name, "desc": "", "value": value, "is_multiline": is_multiline}})
}

fn path(name: &str, folders: bool, limit: usize, value: &[&str]) -> Value {
    json!({"Path": {
        "name": name, "label": name, "desc": "", "value": value,
        "is_excepting_files": folders, "is_excepting_folders": !folders, "limit": limit,
    }})
}

fn select(name: &str, is_multiselect: bool, value: &[&str]) -> Value {
    json!({"ValueSelect": {
        "name": name, "label": name, "desc": "", "value": value, "is_multiselect": is_multiselect,
        "options": [
            {"label": "Alpha", "value": "alpha"}, {"label": "Beta", "value": "beta"},
            {"label": "Gamma", "value": "gamma"}, {"label": "X", "value": "x"},
            {"label": "Y", "value": "y"}, {"label": "Z", "value": "z"},
        ],
    }})
}

/// One parameter of every type, named `<prefix>_<kind>`
fn parameter_set(prefix: &str, v: &Values) -> Vec<Value> {
    let n = |kind: &str| format!("{prefix}_{kind}");
    vec![
        json!({"Separator": {"name": "", "label": format!("{prefix} parameters"), "desc": "", "value": null}}),
        text(&n("text"), false, v.text),
        text(&n("multiline"), true, v.multiline),
        json!({"Integer": {"name": n("integer"), "label": "", "desc": "", "value": v.integer, "min": 1, "max": 10}}),
        json!({"Float": {"name": n("float"), "label": "", "desc": "", "value": v.float, "min": 0.0, "max": 10.0}}),
        json!({"Glob": {"name": n("glob"), "label": "", "desc": "", "value": v.glob}}),
        path(&n("file"), false, 1, &[v.file]),
        path(&n("files"), false, 3, &v.files),
        path(&n("folder"), true, 1, &[v.folder]),
        path(&n("folders"), true, 3, &v.folders),
        select(&n("select"), false, &[v.select]),
        select(&n("multiselect"), true, &v.multiselect),
    ]
}

fn workflow_configuration(workflow_dir: &Path) -> WorkflowConfiguration {
    let mut dynamic = parameter_set("dyn", &DYNAMIC_DEFAULTS);
    // No default: the test always sends it
    dynamic.push(text("token", false, ""));
    if let Some(Value::Object(token)) = dynamic.last_mut().and_then(|t| t.get_mut("Text")) {
        token.insert("value".into(), Value::Null);
    }
    serde_json::from_value(json!({
        "engine": "Nextflow",
        "engine_parameters": [{"name": "ansi-log", "value": "false"}],
        "source": {"Local": {"location": workflow_dir, "script": "main.nf"}},
        "fixed_parameters": parameter_set("fixed", &FIXED),
        "dynamic_parameters": dynamic,
    }))
    .expect("test workflow configuration is valid")
}

/// Values sent for some dynamic parameters. The others keep the stored defaults.
fn overrides(token: &str) -> Vec<ParameterValue> {
    let value = |name: &str, value: Value| ParameterValue {
        name: name.into(),
        value,
    };
    vec![
        value("dyn_text", json!("dyn override text")),
        value("dyn_integer", json!(4)),
        value("dyn_file", json!("data/dyn_file_override.txt")),
        value("dyn_folders", json!(["dyn_folders/two"])),
        value("dyn_multiselect", json!(["y", "z"])),
        value("token", json!(token)),
    ]
}

fn write(project: &Path, relative: &str, content: &str) {
    let file = project.join(relative);
    fs::create_dir_all(file.parent().unwrap()).unwrap();
    fs::write(file, content).unwrap();
}

/// Everything the workflow touches (or must not touch) before the run
fn seed_project(project: &Path) {
    for file in [
        "data/fixed_file.txt",
        "data/fixed_files/a.txt",
        "data/fixed_files/b.txt",
        "data/fixed/g1.txt",
        "data/fixed/g2.txt",
        "data/fixed/ignored.csv",
        "data/dyn_file.txt",
        "data/dyn_file_override.txt",
        "data/dyn_files/a.txt",
        "data/dyn_files/b.txt",
        "data/dyn/g1.txt",
        "data/dyn/g2.txt",
        "data/dyn/ignored.csv",
    ] {
        write(project, file, SEED);
    }
    for folder in [
        "fixed_folder",
        "fixed_folders/one",
        "fixed_folders/two",
        "dyn_folder",
        "dyn_folders/one",
        "dyn_folders/two",
    ] {
        write(project, &format!("{folder}/old.txt"), SEED);
    }
}

fn read(project: &Path, relative: &str) -> String {
    fs::read_to_string(project.join(relative))
        .unwrap_or_else(|e| panic!("cannot read {relative}: {e}"))
}

/// Files the workflow had to overwrite with the token, and the ones it had to leave alone
fn assert_files(project: &Path, token: &str) {
    for file in [
        "data/fixed_file.txt",
        "data/fixed_files/a.txt",
        "data/fixed_files/b.txt",
        "data/fixed/g1.txt",
        "data/fixed/g2.txt",
        "data/dyn_file_override.txt",
        "data/dyn_files/a.txt",
        "data/dyn_files/b.txt",
        "data/dyn/g1.txt",
        "data/dyn/g2.txt",
    ] {
        assert_eq!(read(project, file), token, "{file} was not overwritten");
    }
    // Not matched by the glob / replaced by the override
    for file in [
        "data/fixed/ignored.csv",
        "data/dyn/ignored.csv",
        "data/dyn_file.txt",
    ] {
        assert_eq!(read(project, file), SEED, "{file} must stay untouched");
    }
}

/// Folders get `old.txt` removed and `<token>.marker` created
fn assert_folders(project: &Path, token: &str) {
    for folder in [
        "fixed_folder",
        "fixed_folders/one",
        "fixed_folders/two",
        "dyn_folder",
        "dyn_folders/two",
    ] {
        assert!(
            !project.join(folder).join("old.txt").exists(),
            "{folder}/old.txt was not deleted"
        );
        assert_eq!(
            read(project, &format!("{folder}/{token}.marker")),
            token,
            "{folder} has no marker"
        );
    }
    // Overridden away
    assert_eq!(read(project, "dyn_folders/one/old.txt"), SEED);
    assert!(!project
        .join(format!("dyn_folders/one/{token}.marker"))
        .exists());
}

fn expected_params(token: &str) -> String {
    include_str!("../tests/fixtures/end_to_end/expected_params.txt").replace("{{token}}", token)
}

/// Directory for the test data, shared with the container runtime
fn base_directory() -> PathBuf {
    std::env::var_os("MACWORP_E2E_DIR")
        .map(PathBuf::from)
        .or_else(|| directories::BaseDirs::new().map(|d| d.home_dir().to_path_buf()))
        .expect("no home directory, set MACWORP_E2E_DIR")
}

/// Runtime enforced with `MACWORP_CONTAINER_RUNTIME`, same values as the `--container-runtime` option
fn runtime_preference() -> RuntimePreference {
    match std::env::var("MACWORP_CONTAINER_RUNTIME") {
        Ok(value) => RuntimePreference::from_str(&value, true)
            .unwrap_or_else(|e| panic!("invalid MACWORP_CONTAINER_RUNTIME: {e}")),
        Err(_) => RuntimePreference::Auto,
    }
}

async fn wait_for_run(client: &Client, jwt: &str, run_id: i64) -> RunResponse {
    let started = Instant::now();
    loop {
        let run = client.runs().get(jwt, run_id).await.unwrap();
        if run.status.is_terminal() {
            return run;
        }
        assert!(
            started.elapsed() < RUN_TIMEOUT,
            "run still {:?} after {RUN_TIMEOUT:?}",
            run.status
        );
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs a container runtime and the macworp-nextflow image"]
async fn runs_nextflow_workflow_with_all_parameter_types() {
    let report = macworp_worker::container_runtime::doctor(runtime_preference()).await;
    if !report.is_ok() {
        eprintln!("SKIPPED, no container runtime:\n{report}");
        return;
    }
    let runtime = report.into_runtime().unwrap();
    // e.g. wslc only runs plain container workflows
    if !runtime.supports_engine(&Engine::Nextflow) {
        eprintln!("SKIPPED, {} cannot run Nextflow workflows", runtime.kind());
        return;
    }
    let has_image = tokio::process::Command::new(runtime.binary())
        .args(["image", "inspect", DEFAULT_IMAGE])
        .output()
        .await
        .map(|o| o.status.success())
        .unwrap_or(false);
    if !has_image {
        eprintln!(
            "SKIPPED, image {DEFAULT_IMAGE} not found (see docker/engine-nextflow.dockerfile)"
        );
        return;
    }

    // Everything below lives in one temporary directory that is removed on drop
    let temp = tempfile::Builder::new()
        .prefix("macworp-e2e-")
        .tempdir_in(base_directory())
        .unwrap();
    let root = temp.path().canonicalize().unwrap();
    let workflow_dir = root.join("workflow");
    let project_dir = root.join("project");
    fs::create_dir_all(&workflow_dir).unwrap();
    fs::copy(
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/end_to_end/workflow/main.nf"
        ),
        workflow_dir.join("main.nf"),
    )
    .unwrap();
    seed_project(&project_dir);
    let token = format!("e2e-{}", uuid::Uuid::now_v7());

    // Backend
    let password = "end-to-end-password";
    let database_url = format!("sqlite://{}", root.join("db.sqlite").display());
    let mut backend_config = macworp_backend::configuration::Configuration::default();
    let max_log_bytes = backend_config.max_log_bytes_per_run();
    *backend_config.database_url_mut() = database_url.clone();
    *backend_config.secret_mut() = macworp_backend::configuration::generate_secret();
    *backend_config.interface_mut() = "127.0.0.1".to_string();
    *backend_config.worker_mut().password_mut() = password_auth::generate_hash(password.as_bytes());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base_url = format!("http://{}", listener.local_addr().unwrap());
    *backend_config.port_mut() = listener.local_addr().unwrap().port();
    let (shutdown, shutdown_receiver) = tokio::sync::oneshot::channel::<()>();
    let backend = tokio::spawn(async move {
        Backend::<LocalFilesystem>::start(
            listener,
            Box::pin(async move {
                shutdown_receiver.await.ok();
            }),
            backend_config,
            USER_NAME.to_string(),
        )
        .await
    });

    let client = Client::new(base_url, 20);
    let started = Instant::now();
    while client.utilities().healthcheck().await.is_err() {
        assert!(!backend.is_finished(), "backend stopped during startup");
        assert!(
            started.elapsed() < Duration::from_secs(30),
            "backend did not come up"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let jwt = client
        .login()
        .local_login(USER_NAME.into(), password.into())
        .await
        .unwrap();

    // Workflow integration and project
    let workflow = client
        .workflow_integrations()
        .new_workflow_integration(&jwt, "End to end".into(), "All parameter types".into())
        .await
        .unwrap();
    client
        .workflow_integrations()
        .update_workflow_integration(
            &jwt,
            workflow.id,
            "End to end".into(),
            "All parameter types".into(),
            false,
            workflow_configuration(&workflow_dir),
        )
        .await
        .unwrap();
    let project = client
        .projects()
        .new_project(&jwt, "End to end".into(), project_dir.clone())
        .await
        .unwrap();

    // Run with the embedded worker, wired as in the desktop app
    let (stop, worker) = crate::start_worker(&database_url, max_log_bytes, runtime)
        .await
        .unwrap();
    let run = client
        .runs()
        .start(
            &jwt,
            project.id,
            workflow.id,
            "end to end".into(),
            overrides(&token),
        )
        .await
        .unwrap();
    let run = wait_for_run(&client, &jwt, run.id).await;
    let logs = client.runs().logs(&jwt, run.id, None).await.unwrap();
    let log_text = logs
        .chunks
        .iter()
        .map(|c| c.content.as_str())
        .collect::<Vec<_>>()
        .join("\n");

    stop.send(true).unwrap();
    worker.await.unwrap();
    shutdown.send(()).unwrap();
    backend.await.unwrap().unwrap();

    assert_eq!(
        run.status,
        RunStatus::Succeeded,
        "{run:?}\n--- log ---\n{log_text}"
    );
    assert_eq!(run.exit_code, Some(0));
    assert_eq!(
        (run.tasks_total, run.tasks_done),
        (1, 1),
        "progress not reported"
    );
    assert!(!log_text.is_empty(), "no log lines stored");

    assert_eq!(
        read(&project_dir, "results/params.txt"),
        expected_params(&token)
    );
    assert_files(&project_dir, &token);
    assert_folders(&project_dir, &token);
}
