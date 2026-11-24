//! Full stack tests: backend + SQLite + embedded worker + a real container runtime.
//!
//! One scenario runs once per engine (Nextflow, Container): every parameter type as fixed and as
//! dynamic parameter, the same project, the same expectations (`expected_params.txt`, touched and
//! untouched files). The Nextflow workflow is `tests/fixtures/end_to_end/workflow`, the Container
//! workflow is the image built from `tests/fixtures/end_to_end/container`, which does the same.
//! A further test checks that Snakemake is rejected when the runtime cannot run it (podman).
//!
//! The `reference_change` submodule runs a workflow that reads the global reference data twice
//! while the test changes it in between, to see what a run observes without and with a snapshot.
//!
//! Needs podman or docker, so the tests are ignored by default. Nextflow also needs the
//! engine image of the default version (pulled from the registry or built from `docker/engine.dockerfile` (target `nextflow`)), the Container test
//! builds its image itself (pulls `alpine:3`). A test skips with a message when the selected
//! runtime cannot run its engine.
//!
//! ```text
//! cargo test -p macworp_desktop end_to_end_test -- --ignored --nocapture
//! ```
//!
//! `MACWORP_CONTAINER_RUNTIME=podman|docker|wslc` enforces the container runtime (as for the app
//! and the worker binary), by default podman is preferred over docker.
//!
//! The project lives in a temporary directory below `$MACWORP_E2E_DIR` (default: the home
//! directory), because VM based runtimes on macOS only share the home directory with containers.

use std::{
    fs,
    path::{Path, PathBuf},
    sync::Arc,
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
use macworp_worker::{
    container_runtime::{ContainerRuntime, RuntimePreference},
    executors::{cwl, engine_image, nextflow, snakemake},
};
use serde_json::{json, Value};

mod reference_change;

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
        json!({"Group": {"name": "", "label": format!("{prefix} parameters"), "desc": "", "value": null}}),
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

/// Image built from `tests/fixtures/end_to_end/container` for the `Container` engine
const CONTAINER_IMAGE: &str = "macworp-e2e-container:latest";

/// Workflow definition for `engine` with every parameter type as fixed and as dynamic parameter
fn workflow_configuration(engine: &Engine, workflow_dir: &Path) -> WorkflowConfiguration {
    let mut dynamic = parameter_set("dyn", &DYNAMIC_DEFAULTS);
    // No default: the test always sends it
    dynamic.push(text("token", false, ""));
    if let Some(Value::Object(token)) = dynamic.last_mut().and_then(|t| t.get_mut("Text")) {
        token.insert("value".into(), Value::Null);
    }
    let (engine_parameters, source) = match engine {
        Engine::Nextflow => (
            json!([{"name": "ansi-log", "value": "false"}]),
            json!({"Local": {"location": workflow_dir, "script": "main.nf"}}),
        ),
        Engine::Snakemake => (
            json!([]),
            json!({"Local": {"location": workflow_dir, "script": "Snakefile"}}),
        ),
        Engine::Container => (json!([]), json!({"Image": {"image": CONTAINER_IMAGE}})),
        other => panic!("no end-to-end workflow for {other}"),
    };
    serde_json::from_value(json!({
        "engine": engine,
        "engine_parameters": engine_parameters,
        "source": source,
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

/// Backend, SQLite database and embedded worker, started like in the desktop app. Everything lives
/// in one temporary directory that is removed on drop.
struct TestApp {
    client: Client,
    jwt: String,
    project_dir: PathBuf,
    workflow_dir: PathBuf,
    /// The live global reference data
    reference_directory: PathBuf,
    /// Shared image cache the worker was started with
    apptainer_cache: PathBuf,
    /// Folder of the shared CWL step cache
    cwl_cache: PathBuf,
    /// The location the worker hands to runs as reference data, starts at the live folder
    worker_reference: macworp_backend::reference_directory::ReferenceDirectory,
    shutdown: tokio::sync::oneshot::Sender<()>,
    backend: tokio::task::JoinHandle<Result<(), macworp_backend::error::Error>>,
    stop: tokio::sync::watch::Sender<bool>,
    worker: tokio::task::JoinHandle<()>,
    _temp: tempfile::TempDir,
}

impl TestApp {
    async fn start(runtime: Arc<dyn ContainerRuntime>, unavailable_engines: Vec<Engine>) -> Self {
        let temp = tempfile::Builder::new()
            .prefix("macworp-e2e-")
            .tempdir_in(base_directory())
            .unwrap();
        let root = temp.path().canonicalize().unwrap();
        // Containers the engine starts itself (CWL tools) need the test's own folder labelled on
        // an enforcing SELinux host; created below it inherits the type
        if macworp_worker::executors::selinux_enforcing() {
            macworp_worker::selinux::DirectoryLabel::of(&root)
                .apply()
                .unwrap();
        }
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
        fs::copy(
            concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/fixtures/end_to_end/snakemake/Snakefile"
            ),
            workflow_dir.join("Snakefile"),
        )
        .unwrap();

        let password = "end-to-end-password";
        let database_url = format!("sqlite://{}", root.join("db.sqlite").display());
        let mut backend_config = macworp_backend::configuration::Configuration::default();
        let max_log_bytes = backend_config.max_log_bytes_per_run();
        *backend_config.allow_local_paths_mut() = true;
        *backend_config.database_url_mut() = database_url.clone();
        *backend_config.secret_mut() = macworp_backend::configuration::generate_secret();
        *backend_config.fixed_unavailable_engines_mut() = Some(unavailable_engines);
        *backend_config.interface_mut() = "127.0.0.1".to_string();
        *backend_config.worker_mut().password_mut() =
            password_auth::generate_hash(password.as_bytes());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base_url = format!("http://{}", listener.local_addr().unwrap());
        *backend_config.port_mut() = listener.local_addr().unwrap().port();
        let (shutdown, shutdown_receiver) = tokio::sync::oneshot::channel::<()>();
        let reference_directory = root.join("reference");
        std::fs::create_dir_all(&reference_directory).unwrap();
        let apptainer_cache = root.join("apptainer");
        std::fs::create_dir_all(&apptainer_cache).unwrap();
        let cwl_cache = root.join("cwl");
        std::fs::create_dir_all(&cwl_cache).unwrap();
        let reference_directory_for_backend = reference_directory.clone();
        let worker_reference = macworp_backend::reference_directory::ReferenceDirectory::new(
            reference_directory.clone(),
        );
        let cache_directory = root.join("workflows").join("remote_cache");
        let backend = tokio::spawn(async move {
            Backend::<LocalFilesystem>::start(
                listener,
                Box::pin(async move {
                    shutdown_receiver.await.ok();
                }),
                backend_config,
                USER_NAME.to_string(),
                macworp_backend::reference_directory::ReferenceDirectory::new(
                    reference_directory_for_backend,
                ),
                std::sync::Arc::new(
                    macworp_backend::workflow_repository_cache::WorkflowRepositoryCache::new(
                        cache_directory,
                        Duration::from_secs(3600),
                    ),
                ),
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
        let settings_directory = root.join("settings");
        std::fs::create_dir_all(&settings_directory).unwrap();
        let settings = crate::settings::Settings::load(&settings_directory, 3600).unwrap();
        let (stop, worker) = crate::start_worker(
            &database_url,
            max_log_bytes,
            vec![runtime],
            worker_reference.clone(),
            macworp_backend::reference_directory::ReferenceDirectory::new(apptainer_cache.clone()),
            cwl_cache.clone(),
            &settings,
        )
        .await
        .unwrap();
        Self {
            client,
            jwt,
            project_dir,
            workflow_dir,
            reference_directory,
            apptainer_cache,
            cwl_cache,
            worker_reference,
            shutdown,
            backend,
            stop,
            worker,
            _temp: temp,
        }
    }

    /// Creates the workflow integration and a project for it, then starts a run
    async fn start_run(
        &self,
        configuration: WorkflowConfiguration,
        values: Vec<ParameterValue>,
    ) -> Result<RunResponse, macworp_ui_core::client::Error> {
        let workflow = self
            .client
            .workflow_integrations()
            .new_workflow_integration(&self.jwt, "End to end".into(), "All parameter types".into())
            .await
            .unwrap();
        self.client
            .workflow_integrations()
            .update_workflow_integration(
                &self.jwt,
                workflow.id,
                "End to end".into(),
                "All parameter types".into(),
                configuration,
            )
            .await
            .unwrap();
        let project = self
            .client
            .projects()
            .new_project(&self.jwt, "End to end".into(), self.project_dir.clone())
            .await
            .unwrap();
        self.client
            .runs()
            .start(
                &self.jwt,
                project.id,
                macworp_core::requests::WorkflowStartRequest {
                    workflow_id: workflow.id,
                    comment: "end to end".into(),
                    parameters: values,
                    engine_switches: Vec::new(),
                    resume_from_run: None,
                },
            )
            .await
    }

    /// Waits for the run to end, returns it with its log text
    async fn finished_run(&self, run_id: i64) -> (RunResponse, String) {
        let run = wait_for_run(&self.client, &self.jwt, run_id).await;
        let logs = self
            .client
            .runs()
            .logs(&self.jwt, run_id, None)
            .await
            .unwrap();
        let log_text = logs
            .chunks
            .iter()
            .map(|c| c.content.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        (run, log_text)
    }

    /// Stops worker and backend. The returned directory holds the test data, it is removed when
    /// dropped.
    async fn stop(self) -> tempfile::TempDir {
        self.stop.send(true).unwrap();
        self.worker.await.unwrap();
        self.shutdown.send(()).unwrap();
        self.backend.await.unwrap().unwrap();
        self._temp
    }
}

/// The runtime enforced by `MACWORP_CONTAINER_RUNTIME` (or the preferred one) with its doctor
/// report. `None` and a message when there is none, or it cannot run `engine`: some runtimes only
/// run some engines, those tests are skipped.
async fn require_runtime(engine: &Engine) -> Option<(Arc<dyn ContainerRuntime>, Vec<Engine>)> {
    let report = macworp_worker::container_runtime::doctor(runtime_preference()).await;
    if !report.is_ok() {
        eprintln!("SKIPPED, no container runtime:\n{report}");
        return None;
    }
    let unavailable_engines = report.unavailable_engines();
    let Some(usable) = report.runtime_for(engine) else {
        eprintln!("SKIPPED, no usable runtime can run {engine} workflows");
        return None;
    };
    Some((usable.runtime.clone(), unavailable_engines))
}

async fn image_exists(runtime: &dyn ContainerRuntime, image: &str) -> bool {
    tokio::process::Command::new(runtime.binary())
        .args(["image", "inspect", image])
        .output()
        .await
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Makes sure the image of the engine exists. `false` (with a message) means skip the test.
async fn prepare_image(runtime: &dyn ContainerRuntime, engine: &Engine) -> bool {
    prepare_image_version(runtime, engine, None).await
}

/// The engine image `version` of `image` (the default image's repository with that tag),
/// the image itself without a version.
///
/// # Arguments
/// * `image` - the default image of the engine
/// * `version` - the pinned engine version, if any
fn pinned(image: String, version: Option<&str>) -> String {
    engine_image(&image, version).expect("catalog versions are valid tags")
}

/// [`prepare_image`] for the image of a pinned engine version.
///
/// # Arguments
/// * `runtime` - runtime the test runs on
/// * `engine` - the engine under test
/// * `version` - the pinned engine version, `None` for the default image
async fn prepare_image_version(
    runtime: &dyn ContainerRuntime,
    engine: &Engine,
    version: Option<&str>,
) -> bool {
    match engine {
        Engine::Nextflow => {
            let image = pinned(nextflow::image(), version);
            if image_exists(runtime, &image).await {
                return true;
            }
            eprintln!(
                "SKIPPED, image {image} not found (build it with `docker build -f docker/engine.dockerfile --target nextflow -t {image} docker`, or pull it)"
            );
            false
        }
        Engine::Snakemake => {
            let image = pinned(snakemake::image(), version);
            if image_exists(runtime, &image).await {
                return true;
            }
            eprintln!(
                "SKIPPED, image {image} not found (build it with `docker build -f docker/engine.dockerfile --target snakemake -t {image} docker`, or pull it)"
            );
            false
        }
        Engine::Cwl => {
            let image = pinned(cwl::image(), version);
            if image_exists(runtime, &image).await {
                return true;
            }
            eprintln!(
                "SKIPPED, image {image} not found (build it with `docker build -f docker/engine.dockerfile --target cwl -t {image} docker`, or pull it)"
            );
            false
        }
        Engine::Container => {
            let build = tokio::process::Command::new(runtime.binary())
                .args(["build", "-t", CONTAINER_IMAGE])
                .arg(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/tests/fixtures/end_to_end/container"
                ))
                .output()
                .await
                .unwrap();
            if build.status.success() {
                return true;
            }
            let output = format!(
                "{}{}",
                String::from_utf8_lossy(&build.stdout),
                String::from_utf8_lossy(&build.stderr)
            );
            // wslc has no documented `build`, tolerate a prebuilt image there only
            if runtime.kind().to_string() == "wslc" {
                eprintln!("SKIPPED, wslc cannot build {CONTAINER_IMAGE}:\n{output}");
                return image_exists(runtime, CONTAINER_IMAGE).await;
            }
            panic!("cannot build {CONTAINER_IMAGE}:\n{output}");
        }
    }
}

/// Runs the workflow of `engine` with all parameter types and checks what it wrote. The same
/// parameters, project and expectations for every engine.
async fn run_scenario(engine: Engine) {
    run_scenario_version(engine, None).await;
}

/// [`run_scenario`] on a pinned engine version.
///
/// # Arguments
/// * `engine` - the engine under test
/// * `version` - the engine version the workflow pins, `None` for the default
async fn run_scenario_version(engine: Engine, version: Option<&str>) {
    let Some((runtime, unavailable_engines)) = require_runtime(&engine).await else {
        return;
    };
    if !prepare_image_version(runtime.as_ref(), &engine, version).await {
        return;
    }

    let app = TestApp::start(runtime, unavailable_engines).await;
    seed_project(&app.project_dir);
    let token = format!("e2e-{}", uuid::Uuid::now_v7());

    let mut configuration = workflow_configuration(&engine, &app.workflow_dir);
    *configuration.engine_version_mut() = version.map(String::from);
    let run = app
        .start_run(configuration, overrides(&token))
        .await
        .unwrap();
    let (run, log_text) = app.finished_run(run.id).await;
    let project_dir = app.project_dir.clone();
    let apptainer_cache = app.apptainer_cache.clone();
    let _temp = app.stop().await;

    assert_eq!(
        run.status,
        RunStatus::Succeeded,
        "{run:?}\n--- log ---\n{log_text}"
    );
    assert_eq!(run.exit_code, Some(0));
    // A container workflow has an indeterminate progress, the engines report their tasks
    match engine {
        Engine::Nextflow => assert_eq!((run.tasks_total, run.tasks_done), (1, 1)),
        Engine::Snakemake => {
            assert!(run.tasks_total > 0, "no tasks reported: {run:?}");
            assert_eq!(run.tasks_done, run.tasks_total, "tasks left: {run:?}");
        }
        _ => assert_eq!((run.tasks_total, run.tasks_done), (0, 0)),
    }
    assert!(!log_text.is_empty(), "no log lines stored");
    if engine == Engine::Snakemake {
        // Pulled images land in the shared cache, not in the project
        let cached = std::fs::read_dir(&apptainer_cache).unwrap().count();
        assert!(cached > 0, "image cache is empty: {apptainer_cache:?}");
        assert!(
            !project_dir.join(".snakemake/singularity").exists(),
            "images were cached in the project"
        );
    }

    assert_eq!(
        read(&project_dir, "results/params.txt"),
        expected_params(&token)
    );
    assert_files(&project_dir, &token);
    assert_folders(&project_dir, &token);
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs docker and the macworp-snakemake image"]
async fn runs_snakemake_workflow_with_all_parameter_types() {
    run_scenario(Engine::Snakemake).await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs a container runtime and the macworp-nextflow image"]
async fn runs_nextflow_workflow_with_all_parameter_types() {
    run_scenario(Engine::Nextflow).await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs a container runtime and network access to build the test image"]
async fn runs_container_workflow_with_all_parameter_types() {
    run_scenario(Engine::Container).await;
}

/// The scenario on every version of the engine catalog (Nextflow, Snakemake), each skipped when
/// its image is missing (`make engine-images`).
///
/// # Arguments
/// * `engine` - an engine with a catalog
async fn run_scenario_all_versions(engine: Engine) {
    for version in engine
        .catalog()
        .expect("engine has versions")
        .versions
        .clone()
    {
        eprintln!("--- {engine} {version}");
        run_scenario_version(engine.clone(), Some(&version)).await;
    }
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs docker and the macworp-snakemake images of all versions"]
async fn runs_snakemake_workflow_on_all_versions() {
    run_scenario_all_versions(Engine::Snakemake).await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs a container runtime and the macworp-nextflow images of all versions"]
async fn runs_nextflow_workflow_on_all_versions() {
    run_scenario_all_versions(Engine::Nextflow).await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs a container runtime, the macworp-cwl images of all versions and network access to pull debian"]
async fn runs_cwl_workflow_on_all_versions() {
    for version in Engine::Cwl.catalog().unwrap().versions.clone() {
        eprintln!("--- Cwl {version}");
        run_cwl(Some(&version)).await;
    }
}

/// Podman cannot run Snakemake, the backend answers 409 instead of queueing a run that fails
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs a container runtime"]
async fn snakemake_is_rejected_when_runtime_cannot_run_it() {
    let report = macworp_worker::container_runtime::doctor(runtime_preference()).await;
    if !report.is_ok() {
        eprintln!("SKIPPED, no container runtime:\n{report}");
        return;
    }
    let unavailable_engines = report.unavailable_engines();
    if report.runtime_for(&Engine::Snakemake).is_some() {
        eprintln!("SKIPPED, a usable runtime can run Snakemake workflows");
        return;
    }
    let runtime = report.into_runtimes().unwrap().remove(0);

    let app = TestApp::start(runtime, unavailable_engines).await;
    let configuration: WorkflowConfiguration = serde_json::from_value(json!({
        "engine": "Snakemake",
        "engine_parameters": [],
        "source": {"Local": {"location": app.workflow_dir, "script": "Snakefile"}},
        "fixed_parameters": [],
        "dynamic_parameters": [],
    }))
    .unwrap();
    let result = app.start_run(configuration, Vec::new()).await;
    app.stop().await;

    let error = result
        .expect_err("Snakemake run must be rejected")
        .to_string();
    assert!(
        error.contains("409") && error.contains("not available"),
        "unexpected error: {error}"
    );
}

/// Engine parameter `profile` selects a profile of the workflow's `nextflow.config`: the profile's
/// parameter value reaches the workflow, while its `process.executor = 'slurm'` is overridden by the
/// executor MAcWorP fixes (otherwise the run would never finish).
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs a container runtime and the macworp-nextflow image"]
async fn runs_nextflow_workflow_with_engine_profile() {
    let engine = Engine::Nextflow;
    let Some((runtime, unavailable_engines)) = require_runtime(&engine).await else {
        return;
    };
    if !prepare_image(runtime.as_ref(), &engine).await {
        return;
    }
    let app = TestApp::start(runtime, unavailable_engines).await;
    fs::create_dir_all(&app.project_dir).unwrap();
    for name in ["profile.nf", "nextflow.config"] {
        fs::copy(
            format!(
                "{}/tests/fixtures/end_to_end/profile/{name}",
                env!("CARGO_MANIFEST_DIR")
            ),
            app.workflow_dir.join(name),
        )
        .unwrap();
    }
    let configuration: WorkflowConfiguration = serde_json::from_value(json!({
        "engine": "Nextflow",
        "engine_parameters": [
            {"name": "ansi-log", "value": "false"},
            {"name": "profile", "value": "ci"},
        ],
        "source": {"Local": {"location": app.workflow_dir, "script": "profile.nf"}},
        "fixed_parameters": [],
        "dynamic_parameters": [],
    }))
    .unwrap();
    let run = app.start_run(configuration, Vec::new()).await.unwrap();
    let (run, log_text) = app.finished_run(run.id).await;
    let project_dir = app.project_dir.clone();
    let _temp = app.stop().await;

    assert_eq!(
        run.status,
        RunStatus::Succeeded,
        "{run:?}\n--- log ---\n{log_text}"
    );
    assert_eq!(
        read(&project_dir, "results/profile.txt").trim(),
        "from profile ci"
    );
}

/// Runs a CWL tool through Toil: text, number and a file (a project-relative path the engine gets
/// as a `File`) end up in the output the engine collects into the project directory.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs a container runtime, the macworp-cwl image and network access to pull debian"]
async fn runs_cwl_workflow() {
    run_cwl(None).await;
}

/// Runs a CWL tool in an image that exists only because the worker built it from the
/// workflow's `Dockerfile` (Toil must not try to pull it, which is why the executor passes
/// `--no-prepull`). A second run with the same content finds the image and does not build again.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs a container runtime, the macworp-cwl image and network access to pull debian"]
async fn runs_cwl_workflow_with_a_built_image() {
    let engine = Engine::Cwl;
    let Some((runtime, unavailable_engines)) = require_runtime(&engine).await else {
        return;
    };
    if !prepare_image_version(runtime.as_ref(), &engine, None).await {
        return;
    }
    let app = TestApp::start(runtime, unavailable_engines).await;
    for file in ["tool.cwl", "Dockerfile"] {
        fs::copy(
            format!(
                "{}/tests/fixtures/end_to_end/cwl_image/{file}",
                env!("CARGO_MANIFEST_DIR")
            ),
            app.workflow_dir.join(file),
        )
        .unwrap();
    }
    let configuration: WorkflowConfiguration = serde_json::from_value(json!({
        "engine": "Cwl",
        "engine_parameters": [],
        "source": {"Local": {"location": app.workflow_dir, "script": "tool.cwl"}},
        "fixed_parameters": [text("message", false, "hello image")],
        "dynamic_parameters": [],
        "images": [{"dockerfile": "Dockerfile", "name": "macworp-e2e/cwl-built:1"}],
    }))
    .unwrap();
    let mut logs = Vec::new();
    for attempt in 0..2 {
        let run = app
            .start_run(configuration.clone(), Vec::new())
            .await
            .unwrap();
        let (run, log_text) = app.finished_run(run.id).await;
        assert_eq!(
            run.status,
            RunStatus::Succeeded,
            "attempt {attempt}: {run:?}\n--- log ---\n{log_text}"
        );
        assert_eq!(
            read(&app.project_dir, "cwl_built.txt"),
            "hello image\nbuilt by macworp\n"
        );
        logs.push(log_text);
    }
    let _temp = app.stop().await;
    // The second run found the content hash of the first
    assert!(
        logs[1].contains("not building again"),
        "second run built again:\n{}",
        logs[1]
    );
}

/// Runs a Snakemake rule in an image that exists only because the worker built it from the
/// workflow's `Dockerfile` and converted it into the Apptainer image cache (Snakemake would look
/// the name up on Docker Hub otherwise). A second run with the same content converts nothing.
///
/// # Arguments
/// * `version` - the engine version the workflow pins, `None` for the default
async fn run_snakemake_with_built_image(version: Option<&str>) {
    let engine = Engine::Snakemake;
    let Some((runtime, unavailable_engines)) = require_runtime(&engine).await else {
        return;
    };
    if !prepare_image_version(runtime.as_ref(), &engine, version).await {
        return;
    }
    let app = TestApp::start(runtime, unavailable_engines).await;
    // Created by the test: a missing mount source would be created root-owned by Docker
    fs::create_dir_all(&app.project_dir).unwrap();
    for file in ["Snakefile", "Dockerfile"] {
        fs::copy(
            format!(
                "{}/tests/fixtures/end_to_end/snakemake_image/{file}",
                env!("CARGO_MANIFEST_DIR")
            ),
            app.workflow_dir.join(file),
        )
        .unwrap();
    }
    let mut configuration: WorkflowConfiguration = serde_json::from_value(json!({
        "engine": "Snakemake",
        "engine_parameters": [],
        "source": {"Local": {"location": app.workflow_dir, "script": "Snakefile"}},
        "fixed_parameters": [],
        "dynamic_parameters": [],
        "images": [{"dockerfile": "Dockerfile", "name": "macworp-e2e/snakemake-built:1"}],
    }))
    .unwrap();
    *configuration.engine_version_mut() = version.map(String::from);
    let mut logs = Vec::new();
    for attempt in 0..2 {
        let run = app
            .start_run(configuration.clone(), Vec::new())
            .await
            .unwrap();
        let (run, log_text) = app.finished_run(run.id).await;
        assert_eq!(
            run.status,
            RunStatus::Succeeded,
            "attempt {attempt}: {run:?}\n--- log ---\n{log_text}"
        );
        assert_eq!(
            read(&app.project_dir, "results/built.txt"),
            "built by macworp\n"
        );
        // Snakemake reruns nothing, the output exists: remove it so the rule runs again
        std::fs::remove_file(app.project_dir.join("results/built.txt")).unwrap();
        logs.push(log_text);
    }
    let _temp = app.stop().await;
    assert!(
        logs[0].contains("Converting image"),
        "first run did not convert:\n{}",
        logs[0]
    );
    assert!(
        logs[1].contains("not converting again"),
        "second run converted again:\n{}",
        logs[1]
    );
}

/// [`run_snakemake_with_built_image`] on the default Snakemake version.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs docker, the macworp-snakemake image and network access to pull debian"]
async fn runs_snakemake_workflow_with_a_built_image() {
    run_snakemake_with_built_image(None).await;
}

/// [`run_snakemake_with_built_image`] on every Snakemake version of the catalog, which checks
/// that all of them name their cached images the way the worker expects.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs docker, the macworp-snakemake images of all versions and network access to pull debian"]
async fn runs_snakemake_workflow_with_a_built_image_on_all_versions() {
    for version in Engine::Snakemake.catalog().unwrap().versions.clone() {
        eprintln!("--- Snakemake {version}");
        run_snakemake_with_built_image(Some(&version)).await;
    }
}

/// [`runs_cwl_workflow`] on a pinned Toil version.
///
/// # Arguments
/// * `version` - the engine version the workflow pins, `None` for the default
async fn run_cwl(version: Option<&str>) {
    let engine = Engine::Cwl;
    let Some((runtime, unavailable_engines)) = require_runtime(&engine).await else {
        return;
    };
    if !prepare_image_version(runtime.as_ref(), &engine, version).await {
        return;
    }
    let app = TestApp::start(runtime, unavailable_engines).await;
    fs::copy(
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/end_to_end/cwl/tool.cwl"
        ),
        app.workflow_dir.join("tool.cwl"),
    )
    .unwrap();
    write(&app.project_dir, "data/in.txt", "file content\n");
    let mut configuration: WorkflowConfiguration = serde_json::from_value(json!({
        "engine": "Cwl",
        "engine_parameters": [],
        "source": {"Local": {"location": app.workflow_dir, "script": "tool.cwl"}},
        "fixed_parameters": [
            text("message", false, "hello cwl"),
            {"Integer": {"name": "count", "label": "", "desc": "", "value": 3, "min": 1, "max": 10}},
            path("input", false, 1, &["data/in.txt"]),
        ],
        "dynamic_parameters": [],
    }))
    .unwrap();
    *configuration.engine_version_mut() = version.map(String::from);
    let run = app.start_run(configuration, Vec::new()).await.unwrap();
    let (run, log_text) = app.finished_run(run.id).await;
    let project_dir = app.project_dir.clone();
    let cwl_cache = app.cwl_cache.clone();
    let _temp = app.stop().await;

    assert_eq!(
        run.status,
        RunStatus::Succeeded,
        "{run:?}\n--- log ---\n{log_text}"
    );
    assert_eq!(
        read(&project_dir, "cwl_result.txt"),
        "hello cwl\n3\nfile content\n"
    );
    // The step went through the shared step cache that a later run reuses (Toil 7 has none)
    let cached = std::fs::read_dir(&cwl_cache).unwrap().count();
    if cwl::CwlExecutor::supports_step_cache(version) {
        assert!(cached > 0, "step cache is empty: {cwl_cache:?}");
    } else {
        assert_eq!(cached, 0, "Toil 7 filled the step cache: {cwl_cache:?}");
    }
}
