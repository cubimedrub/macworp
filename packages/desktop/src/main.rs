mod client;
mod components;
mod db_job_source;
/// Full stack test with a real Nextflow engine container, test only.
///
/// Not run by a plain `cargo test` (it is `#[ignore]`d, needs podman/docker and the
/// Nextflow engine image). Run it with
/// `cargo test -p macworp_desktop end_to_end_test -- --ignored --nocapture`.
#[cfg(test)]
mod end_to_end_test;
mod errors;
mod file;
mod pages;
mod router;
mod runtime_status;
mod settings;
mod storage;

use std::{env, sync::Arc, time::Duration};

use clap::{Parser, Subcommand};
use dioxus::prelude::*;
use directories::ProjectDirs;
use rand::prelude::*;

use macworp_backend::models::RunTable;
use macworp_core::logging::init_tracing_subscription;
use macworp_core::runtime_status::SleepStatus;
use macworp_ui_core::{
    client::Client, components::Bootstrap, configuration::Configuration,
    runtime_status::RuntimeStatusHandle, session::Session, storage::IsStorage, FAVICON, MAIN_CSS,
};

use macworp_worker::{
    apptainer_cache_store::ApptainerCacheStore,
    configuration::WorkerConfiguration,
    container_runtime::ContainerRuntime,
    executors::standard_executors,
    image_builder::ImageBuilder,
    image_store::{ImagePolicy, ImageStore},
    log_proxy::{HostAccess, ProgressReceiver},
    worker::{Worker, WorkerOptions},
};
use tokio::sync::watch;

use crate::{
    db_job_source::DbJobSource, errors::Error, router::Router,
    runtime_status::InitialRuntimeStatus, storage::Storage,
};

const DESKTOP_USER_NAME: &str = "admin";

static DEFAULT_LOGGING_DIRECTIVES: &[&str] = &[
    "axum=error",
    "dioxus_core=error",
    "dioxus_signals=error",
    "hyper_util=error",
    "reqwest=error",
    "sqlx=error",
    "tao=error",
    "tungstenite=error",
    "warnings=error",
];

#[derive(Debug, Subcommand)]
enum SubCommand {
    /// Show version information
    Version {},
}

#[derive(Debug, Parser)]
#[command(name = env!("CARGO_PKG_NAME"))]
struct CommandLineInterface {
    /// Optional suffix for app folder. Useful for development or troubleshooting
    /// to avoid collision with production. Adds `-{app_folder_suffix}` to the the directory
    #[arg(short, long)]
    app_folder_suffix: Option<String>,
    /// Verbosity level
    /// 0 - Error,
    /// 1 - Warn,
    /// 2 - Info,
    /// 3 - Debug,
    /// 4 - Trace
    /// > 4 Set to tracing and remove all filter directives for underlaying crates
    #[arg(short, long, action = clap::ArgAction::Count)]
    verbose: u8,
    #[command(flatten)]
    worker: WorkerConfiguration,
    #[command(subcommand)]
    subcommand: Option<SubCommand>,
}

/// Starts the desktop version of MAcWorP.
/// Application data is stored in the OS-specific application directory, see [Directories](https://docs.rs/directories/latest/directories/struct.ProjectDirs.html) for more information
/// * qualifier: `de.ruhr-uni-bochum`
/// * organization: `CUBiMed.RUB`
/// * application: `env!("CARGO_PKG_NAME")`s
///
/// Database is SQL
fn main() -> Result<(), Error> {
    let cli_args = CommandLineInterface::parse();

    if let Some(subcommand) = &cli_args.subcommand {
        match subcommand {
            SubCommand::Version {} => {
                println!("{}", env!("CARGO_PKG_VERSION"));
            }
        }
        return Ok(());
    }
    init_tracing_subscription(cli_args.verbose, true, DEFAULT_LOGGING_DIRECTIVES)?;
    runtime_status::set_preference(cli_args.worker.container_runtime);

    // The runtime is not entered with `#[tokio::main]`: that would run the whole UI event loop
    // inside `block_on`, which sets tokio's cooperative budget once for the lifetime of the
    // loop and never resets it. After a few dozen HTTP calls every tokio future in the UI
    // would answer "pending" and wake itself at once, spinning the UI at 100% CPU. Entering
    // the runtime only (`enter`) leaves the UI threads without a budget.
    let runtime = tokio::runtime::Runtime::new().map_err(Error::AsyncRuntime)?;
    let result = match runtime.block_on(start(cli_args.app_folder_suffix)) {
        Ok(mut running) => {
            {
                let _entered = runtime.enter();
                running.launch();
            }
            runtime.block_on(running.stop())
        }
        Err(error) => Err(error),
    };
    match result {
        Ok(()) => tracing::info!("Application stopped successfully"),
        Err(e) => tracing::error!("Application stopped with error: {e}"),
    }
    Ok(())
}

/// The desktop app after startup, before and after its window: the UI to launch and what to
/// stop afterwards.
struct Running {
    /// The UI with everything it needs from the setup, `None` once launched
    ui: Option<dioxus::LaunchBuilder>,
    /// Waits for the container runtime and starts the embedded worker
    worker_supervisor: tokio::task::JoinHandle<()>,
    /// The embedded worker once it started
    worker_slot: Arc<std::sync::Mutex<Option<WorkerHandle>>>,
    /// Stops the backend
    backend_shutdown_signal_sender: tokio::sync::oneshot::Sender<()>,
    /// The backend task
    backend: tokio::task::JoinHandle<Result<(), macworp_backend::error::Error>>,
}

impl Running {
    /// Shows the window and returns when it is closed. Must be called with the tokio runtime
    /// entered, not inside `block_on`, see `main`.
    fn launch(&mut self) {
        if let Some(ui) = self.ui.take() {
            ui.launch(App);
        }
    }

    /// Stops the embedded worker (a running job is finished first) and the backend.
    async fn stop(self) -> Result<(), Error> {
        self.worker_supervisor.abort();
        let worker = self
            .worker_slot
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take();
        if let Some((stop, handle)) = worker {
            // A running job is finished (or cancelled by process exit) before the backend goes away
            stop.send(true).ok();
            handle.await.map_err(Error::WorkerStopping)?;
        }

        self.backend_shutdown_signal_sender
            .send(())
            .map_err(|_| Error::BackendStopSignal)?;

        self.backend
            .await
            .map_err(Error::BackendStopping)?
            .map_err(Error::Backend)
    }
}

/// Starts the desktop application with the backend running in the background
///
/// Pa
async fn start(app_folder_suffix: Option<String>) -> Result<Running, Error> {
    let application_name = if let Some(app_folder_suffix) = app_folder_suffix {
        format!("{}-{}", env!("CARGO_PKG_NAME"), app_folder_suffix)
    } else {
        env!("CARGO_PKG_NAME").to_string()
    };
    let app_directory = ProjectDirs::from("de.ruhr-uni-bochum", "CUBiMed.RUB", &application_name)
        .ok_or(Error::ProjectDirectoryNotFound)?;

    let app_data_directory = app_directory.data_dir();

    tracing::info!(
        "Application directory is: {}",
        app_data_directory.to_str().unwrap()
    );

    if app_data_directory.is_file() {
        return Err(Error::AppDirectoryCreation(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            "App data directory path point to existing file",
        )));
    }

    match app_data_directory.try_exists() {
        Ok(true) => tracing::info!("App directory already exists"),
        Ok(false) => {
            tracing::info!("Creating app directory");
            create_private_directory(app_data_directory).map_err(Error::AppDirectoryCreation)?;
        }
        Err(err) => return Err(Error::AppDirectoryCreation(err)),
    }

    let settings = settings::Settings::load(
        app_data_directory,
        macworp_backend::configuration::Configuration::default().branch_cache_seconds(),
    )?;

    // Secrets live in memory only: the backend re-syncs the desktop user's password on
    // every start and the login token is never persisted, so nothing needs to survive a restart.
    for legacy_file in ["pwd", "jwt_secret"] {
        match std::fs::remove_file(app_data_directory.join(legacy_file)) {
            Ok(_) => tracing::info!("Removed legacy secret file {legacy_file}"),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            Err(err) => tracing::warn!("Unable to remove legacy secret file {legacy_file}: {err}"),
        }
    }

    let application_password = generate_password();
    let jwt_secret = macworp_backend::configuration::generate_secret();

    let folders: runtime_status::Folders = {
        let settings = settings.clone();
        let data_directory = app_data_directory.to_path_buf();
        Arc::new(move || {
            vec![
                ("application data".to_string(), data_directory.clone()),
                (
                    "reference data".to_string(),
                    settings.path(settings::Folder::Reference),
                ),
                (
                    "workflow cache".to_string(),
                    settings.path(settings::Folder::WorkflowCache),
                ),
                (
                    "container image cache".to_string(),
                    settings.path(settings::Folder::ApptainerCache),
                ),
                ("CWL cache".to_string(), settings.cwl_cache()),
            ]
        })
    };
    let mut container_runtime_report =
        macworp_worker::container_runtime::doctor(runtime_status::preference()).await;
    container_runtime_report
        .check_shared_folders(&folders())
        .await;
    let startup_status = runtime_status::status_of(&container_runtime_report).await;
    if container_runtime_report.is_ok() {
        tracing::info!("Container runtime:\n{container_runtime_report}");
    } else {
        tracing::warn!(
            "No usable container runtime, workflows cannot run:\n{container_runtime_report}"
        );
    }
    // The usable runtimes reach the embedded worker through this channel: when the user starts
    // a stopped engine from the app, the worker starts then
    let (runtimes_sender, runtimes_receiver) =
        tokio::sync::watch::channel(container_runtime_report.into_runtimes().ok());
    // The backend serves the status, starting the engine and the memory change to the UI
    // under `/runtime`, and rejects runs of the engines the last check found unavailable
    let runtime_control = Arc::new(runtime_status::DesktopRuntime::new(
        startup_status,
        runtimes_sender,
        folders,
    ));
    let sleep_prevention = runtime_control.sleep_prevention();

    let database_path = app_data_directory.join("db.sqlite");

    let database_url = format!("sqlite://{}", database_path.to_str().unwrap());

    tracing::info!("Using database at: {}", database_url);

    let max_log_bytes =
        macworp_backend::configuration::Configuration::default().max_log_bytes_per_run();
    let mut backend_config = macworp_backend::configuration::Configuration::default();
    // Single local user: workflows may live in any folder, picked in the native dialog
    *backend_config.allow_local_paths_mut() = true;
    *backend_config.database_url_mut() = database_url.clone();
    *backend_config.secret_mut() = jwt_secret;
    *backend_config.interface_mut() = "127.0.0.1".to_string();
    *backend_config.worker_mut().password_mut() =
        password_auth::generate_hash(application_password.as_bytes());

    let backend_listener =
        tokio::net::TcpListener::bind(format!("{}:0", backend_config.interface()))
            .await
            .unwrap();

    *backend_config.port_mut() = backend_listener
        .local_addr()
        .map_err(Error::BackendAddressNotFound)?
        .port();

    let base_url = format!(
        "http://{}:{}",
        backend_config.interface(),
        backend_config.port()
    );

    let (backend_shutdown_signal_sender, backend_shutdown_signal_receiver) =
        tokio::sync::oneshot::channel::<()>();
    let backend_shutdown_signal = async move {
        backend_shutdown_signal_receiver.await.ok();
    };

    let reference_directory = settings.reference_directory();
    let workflow_repository_cache = settings.workflow_repository_cache();
    let backend = tokio::spawn(async move {
        match macworp_backend::Backend::<
            macworp_backend::filesystems::local_filesystem::LocalFilesystem,
        >::start(
            backend_listener,
            Box::pin(backend_shutdown_signal),
            backend_config,
            DESKTOP_USER_NAME.to_string(),
            reference_directory,
            workflow_repository_cache,
            runtime_control,
        )
        .await
        {
            Ok(_) => Ok(()),
            Err(e) => {
                tracing::error!("Backend stopped with error: {e}");
                Err(e)
            }
        }
    });

    let config = Configuration::new(base_url, 20, 10 * 1024 * 1024);

    let client = Client::new(config.base_url().to_string(), config.timeout());

    // Wait for backend to come up.
    loop {
        if backend.is_finished() {
            return Err(Error::BackendStoppedUnexpectedly);
        }
        match client.utilities().healthcheck().await {
            Ok(_) => break,
            Err(_) => {
                tracing::warn!("Backend is not available yet, retrying in 100ms..");
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        }
    }

    // Backend ran the migrations, the embedded worker can use the same database now. It starts
    // as soon as a container runtime answers, which may be later than the app.
    let worker_slot: Arc<std::sync::Mutex<Option<WorkerHandle>>> = Arc::default();
    let worker_supervisor = tokio::spawn({
        let database_url = database_url.clone();
        let reference_directory = settings.reference_directory();
        let apptainer_cache = settings.apptainer_cache();
        let cwl_cache = settings.cwl_cache();
        let settings = settings.clone();
        let worker_slot = worker_slot.clone();
        let sleep_prevention = sleep_prevention.clone();
        let mut runtimes_receiver = runtimes_receiver;
        async move {
            let runtimes = loop {
                if let Some(runtimes) = runtimes_receiver.borrow_and_update().clone() {
                    break runtimes;
                }
                if runtimes_receiver.changed().await.is_err() {
                    return;
                }
            };
            match start_worker(
                &database_url,
                max_log_bytes,
                runtimes,
                reference_directory,
                apptainer_cache,
                cwl_cache,
                &settings,
                sleep_prevention,
            )
            .await
            {
                Ok(worker) => *worker_slot.lock().unwrap_or_else(|e| e.into_inner()) = Some(worker),
                Err(error) => tracing::error!("The embedded worker could not start: {error}"),
            }
        }
    });

    let mut storage = Storage::new();

    let jwt = client
        .login()
        .local_login(DESKTOP_USER_NAME.to_string(), application_password.clone())
        .await
        .map_err(Error::LoginFailed)?;

    // The UI gets the runtime status from the backend like any other data
    let runtime_status = InitialRuntimeStatus(
        client
            .runtime()
            .status(&jwt)
            .await
            .map_err(Error::RuntimeStatus)?,
    );

    storage.set_jwt(Some(jwt))?;

    let ui = dioxus::LaunchBuilder::new()
        .with_context(config)
        .with_context(storage)
        .with_context(runtime_status)
        .with_context(settings)
        .with_context(macworp_ui_core::local_workflow::LocalFolderPicker::new(
            |start| {
                Box::pin(async move {
                    rfd::AsyncFileDialog::new()
                        .set_title("Folder containing the workflow")
                        .set_directory(start)
                        .pick_folder()
                        .await
                        .map(|folder| folder.path().to_path_buf())
                })
            },
        ))
        ;

    Ok(Running {
        ui: Some(ui),
        worker_supervisor,
        worker_slot,
        backend_shutdown_signal_sender,
        backend,
    })
}

/// How to stop the embedded worker: the stop signal and the task that ends when it has stopped.
type WorkerHandle = (watch::Sender<bool>, tokio::task::JoinHandle<()>);

/// Starts the embedded worker on the backend database.
///
/// Runs left `Running` by an earlier crash are failed first, nothing can be executing them now.
///
/// # Arguments
/// * `database_url` - the backend database the worker claims runs from
/// * `max_log_bytes` - log size kept per run
/// * `runtimes` - usable container runtimes
/// * `reference_directory` - live handle of the global reference data folder
/// * `apptainer_cache` - live handle of the Snakemake image cache folder
/// * `cwl_cache` - folder of the CWL step cache
/// * `settings` - supplies the storage limits and the state directory for usage files; the
///   worker's stores are attached to it so limit changes apply live
/// * `sleep_prevention` - the worker holds it while a run executes, so the computer stays awake
///   on mains power
#[allow(clippy::too_many_arguments)]
async fn start_worker(
    database_url: &str,
    max_log_bytes: u64,
    runtimes: Vec<Arc<dyn ContainerRuntime>>,
    reference_directory: macworp_backend::reference_directory::ReferenceDirectory,
    apptainer_cache: macworp_backend::reference_directory::ReferenceDirectory,
    cwl_cache: std::path::PathBuf,
    settings: &settings::Settings,
    sleep_prevention: Arc<macworp_worker::sleep_prevention::SleepPrevention>,
) -> Result<WorkerHandle, Error> {
    let db = sea_orm::Database::connect(database_url)
        .await
        .map_err(Error::WorkerDatabase)?;
    match RunTable::fail_orphaned_running(&db).await {
        Ok(0) => {}
        Ok(n) => tracing::warn!("Marked {n} interrupted run(s) as failed"),
        Err(e) => tracing::warn!("Unable to clean up interrupted runs: {e}"),
    }

    let shared_folders = match (
        runtimes.first(),
        directories::UserDirs::new().map(|dirs| dirs.home_dir().to_path_buf()),
    ) {
        (Some(runtime), Some(home)) => match runtime.info().await {
            Ok(info) => macworp_worker::container_runtime::SharedFolders::detect(&info, &home)
                .await
                .map(Arc::new),
            Err(_) => None,
        },
        _ => None,
    };
    let executors = standard_executors(&runtimes).await;
    let worker = Worker::new(
        Arc::new(DbJobSource::new(
            db,
            max_log_bytes,
            reference_directory,
            apptainer_cache,
            cwl_cache,
        )),
        executors,
        WorkerOptions::default(),
    )
    .with_image_builders(
        runtimes
            .iter()
            .map(|runtime| ImageBuilder::for_runtime(runtime.as_ref()))
            .collect(),
    )
    // Runs whose folders the container VM cannot see fail with a clear message (macOS)
    .with_shared_folders(shared_folders)
    .with_sleep_prevention(sleep_prevention);
    // Limits come from the settings page; the stores start with the saved ones
    let image_stores: Vec<Arc<ImageStore>> = runtimes
        .iter()
        .map(|runtime| {
            Arc::new(ImageStore::new(
                runtime.clone(),
                ImagePolicy::desktop_default(),
                ImageStore::state_file_in(Some(settings.directory()), &runtime.kind().to_string()),
            ))
        })
        .collect();
    let cache_store = Arc::new(ApptainerCacheStore::new(
        None,
        ApptainerCacheStore::state_file_in(Some(settings.directory())),
    ));
    settings.attach_stores(image_stores.clone(), cache_store.clone());
    let mut worker = worker
        .with_image_stores(image_stores)
        .with_apptainer_cache_store(cache_store);
    if let Some(access) = runtimes
        .iter()
        .find_map(|runtime| HostAccess::for_runtime(runtime.kind()))
    {
        match ProgressReceiver::start(access).await {
            Ok(receiver) => {
                tracing::info!(
                    "Engine progress receiver listening on port {}",
                    receiver.port()
                );
                worker = worker.with_progress_receiver(Arc::new(receiver));
            }
            Err(e) => tracing::warn!(
                "Cannot start the engine progress receiver, runs show no progress: {e}"
            ),
        }
    }

    let (stop, stopped) = watch::channel(false);
    let handle = tokio::spawn(async move { worker.run_until(stopped).await });
    let kinds: Vec<String> = runtimes
        .iter()
        .map(|runtime| runtime.kind().to_string())
        .collect();
    tracing::info!("Embedded worker started using {}", kinds.join(", "));
    Ok((stop, handle))
}

/// Creates a random 64 character password from ascii characters.
/// This method does not produce recommended passwords or enforces
/// any kind of pasword rules
/// Creates the directory (and parents), readable by the current user only on unix
fn create_private_directory(path: &std::path::Path) -> std::io::Result<()> {
    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
    builder.create(path)
}

fn generate_password() -> String {
    // Pretty much every ASCII char on a keyboard
    let alphabet = (33..=126).collect::<Vec<u8>>();

    let mut rng = rand::rng();
    let password: String = (0..64)
        .map(|_| alphabet.choose(&mut rng).cloned().unwrap() as char)
        .collect();

    password
}

#[component]
fn App() -> Element {
    // Get configuration and storage
    let config = consume_context::<Configuration>();
    let storage = consume_context::<Storage>();

    // Make both available app wide
    let config_signal = use_context_provider(|| Signal::new(config));
    // Storage need to be wrapped in Result, like on web version (depending on Browser storage initialization may fail)
    let storage_signal = use_context_provider(|| Signal::new(storage));

    // Container runtime status bar. Everything goes through the backend (`/runtime`), the UI runs
    // no checks or commands itself. A refresh only updates the UI: the backend keeps the
    // engine availability it was started with.
    let initial_status = consume_context::<InitialRuntimeStatus>();
    let mut status = use_signal(|| initial_status.0);
    let mut refreshing = use_signal(|| false);
    let mut starting = use_signal(|| false);
    let mut start_error = use_signal(|| None::<String>);
    let mut adjusting = use_signal(|| false);
    let mut adjust_error = use_signal(|| None::<String>);
    let mut sleep = use_signal(SleepStatus::default);
    // The power source can change at any time: ask the backend every few seconds
    use_future(move || async move {
        loop {
            if let Ok(session) = Session::new(storage_signal, config_signal) {
                if let Ok(current) = session.client.runtime().sleep(&session.jwt).await {
                    if *sleep.peek() != current {
                        sleep.set(current);
                    }
                }
            }
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        }
    });
    let refresh = use_callback(move |()| {
        spawn(async move {
            refreshing.set(true);
            match Session::new(storage_signal, config_signal) {
                Ok(session) => match session.client.runtime().refresh(&session.jwt).await {
                    Ok(new_status) => status.set(new_status),
                    Err(error) => tracing::warn!("Cannot check the container runtime: {error}"),
                },
                Err(error) => tracing::warn!("Cannot check the container runtime: {error}"),
            }
            refreshing.set(false);
        });
    });
    let start_engine = use_callback(move |()| {
        spawn(async move {
            starting.set(true);
            start_error.set(None);
            let result = match Session::new(storage_signal, config_signal) {
                Ok(session) => session
                    .client
                    .runtime()
                    .start(&session.jwt)
                    .await
                    .map_err(|error| error.to_string()),
                Err(error) => Err(error),
            };
            match result {
                Ok(new_status) => status.set(new_status),
                Err(message) => start_error.set(Some(message)),
            }
            starting.set(false);
        });
    });
    let adjust_memory = use_callback(move |mebibytes: u64| {
        spawn(async move {
            adjusting.set(true);
            adjust_error.set(None);
            let result = match Session::new(storage_signal, config_signal) {
                Ok(session) => session
                    .client
                    .runtime()
                    .adjust_memory(&session.jwt, mebibytes)
                    .await
                    .map_err(|error| error.to_string()),
                Err(error) => Err(error),
            };
            match result {
                Ok(new_status) => status.set(new_status),
                Err(message) => adjust_error.set(Some(message)),
            }
            adjusting.set(false);
        });
    });
    // One local user owns everything: no sharing, groups or owner column
    use_context_provider(|| macworp_ui_core::sharing::SharingEnabled(false));
    use_context_provider(|| macworp_ui_core::logout::LogoutEnabled(false));
    use_context_provider(|| RuntimeStatusHandle {
        status,
        refreshing,
        refresh,
        start_engine,
        starting,
        start_error,
        adjust_memory,
        adjusting,
        adjust_error,
        sleep,
    });

    rsx! {
        document::Link { rel: "icon", href: FAVICON }
        document::Link { rel: "stylesheet", href: MAIN_CSS }
        Bootstrap {}

        Router::<Router> {}
    }
}
