mod client;
mod components;
mod db_job_source;
/// Full stack test with a real Nextflow engine container, test only.
///
/// Not run by a plain `cargo test` (it is `#[ignore]`d, needs podman/docker and the
/// `macworp-nextflow:latest` image). Run it with
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
use macworp_ui_core::{
    client::Client, components::Bootstrap, configuration::Configuration,
    runtime_status::RuntimeStatusHandle, storage::IsStorage, FAVICON, MAIN_CSS,
};

use macworp_worker::{
    configuration::WorkerConfiguration,
    container_runtime::ContainerRuntime,
    executors::standard_executors,
    log_proxy::{HostAccess, ProgressReceiver},
    worker::{Worker, WorkerOptions},
};
use tokio::sync::watch;

use crate::{
    db_job_source::DbJobSource,
    errors::Error,
    router::Router,
    runtime_status::{check_runtime, to_status, InitialRuntimeStatus},
    storage::Storage,
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
#[tokio::main]
async fn main() -> Result<(), Error> {
    let cli_args = CommandLineInterface::parse();

    if let Some(subcommand) = &cli_args.subcommand {
        match subcommand {
            SubCommand::Version {} => {
                println!("{}", env!("CARGO_PKG_VERSION"));
            }
        }
    } else {
        init_tracing_subscription(cli_args.verbose, true, DEFAULT_LOGGING_DIRECTIVES)?;
        runtime_status::set_preference(cli_args.worker.container_runtime);
        match start(cli_args.app_folder_suffix).await {
            Ok(_) => tracing::info!("Application stopped successfully"),
            Err(e) => tracing::error!("Application stopped with error: {e}"),
        }
    }

    Ok(())
}

/// Starts the desktop application with the backend running in the background
///
/// Pa
async fn start(app_folder_suffix: Option<String>) -> Result<(), Error> {
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

    let container_runtime_report =
        macworp_worker::container_runtime::doctor(runtime_status::preference()).await;
    let unavailable_engines = container_runtime_report.unavailable_engines();
    let runtime_status = InitialRuntimeStatus(to_status(&container_runtime_report));
    if container_runtime_report.is_ok() {
        tracing::info!("Container runtime:\n{container_runtime_report}");
    } else {
        tracing::warn!(
            "No usable container runtime, workflows cannot run:\n{container_runtime_report}"
        );
    }
    let container_runtime = container_runtime_report.into_runtime().ok();

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

    let settings = settings::Settings::load(app_data_directory)?;

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

    let database_path = app_data_directory.join("db.sqlite");

    let database_url = format!("sqlite://{}", database_path.to_str().unwrap());

    tracing::info!("Using database at: {}", database_url);

    let max_log_bytes =
        macworp_backend::configuration::Configuration::default().max_log_bytes_per_run();
    let mut backend_config = macworp_backend::configuration::Configuration::default();
    *backend_config.database_url_mut() = database_url.clone();
    *backend_config.secret_mut() = jwt_secret;
    *backend_config.fixed_unavailable_engines_mut() = Some(unavailable_engines);
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
    let backend = tokio::spawn(async move {
        match macworp_backend::Backend::<
            macworp_backend::filesystems::local_filesystem::LocalFilesystem,
        >::start(
            backend_listener,
            Box::pin(backend_shutdown_signal),
            backend_config,
            DESKTOP_USER_NAME.to_string(),
            reference_directory,
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

    // Backend ran the migrations, the embedded worker can use the same database now
    let worker = match container_runtime {
        Some(runtime) => Some(
            start_worker(
                &database_url,
                max_log_bytes,
                runtime,
                settings.reference_directory(),
            )
            .await?,
        ),
        None => None,
    };

    let mut storage = Storage::new();

    let jwt = client
        .login()
        .local_login(DESKTOP_USER_NAME.to_string(), application_password.clone())
        .await
        .map_err(Error::LoginFailed)?;

    storage.set_jwt(Some(jwt))?;

    dioxus::LaunchBuilder::new()
        .with_context(config)
        .with_context(storage)
        .with_context(runtime_status)
        .with_context(settings)
        .launch(App);

    if let Some((stop, handle)) = worker {
        // A running job is finished (or cancelled by process exit) before the backend goes away
        stop.send(true).ok();
        handle.await.map_err(Error::WorkerStopping)?;
    }

    backend_shutdown_signal_sender
        .send(())
        .map_err(|_| Error::BackendStopSignal)?;

    backend
        .await
        .map_err(Error::BackendStopping)?
        .map_err(Error::Backend)
}

/// Starts the embedded worker on the backend database.
///
/// Runs left `Running` by an earlier crash are failed first, nothing can be executing them now.
async fn start_worker(
    database_url: &str,
    max_log_bytes: u64,
    runtime: Arc<dyn ContainerRuntime>,
    reference_directory: macworp_backend::reference_directory::ReferenceDirectory,
) -> Result<(watch::Sender<bool>, tokio::task::JoinHandle<()>), Error> {
    let db = sea_orm::Database::connect(database_url)
        .await
        .map_err(Error::WorkerDatabase)?;
    match RunTable::fail_orphaned_running(&db).await {
        Ok(0) => {}
        Ok(n) => tracing::warn!("Marked {n} interrupted run(s) as failed"),
        Err(e) => tracing::warn!("Unable to clean up interrupted runs: {e}"),
    }

    let executors = standard_executors(&runtime).await;
    let mut worker = Worker::new(
        Arc::new(DbJobSource::new(db, max_log_bytes, reference_directory)),
        executors,
        WorkerOptions::default(),
    );
    if let Some(access) = HostAccess::for_runtime(runtime.kind()) {
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
    tracing::info!("Embedded worker started using {}", runtime.kind());
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
    use_context_provider(|| Signal::new(config));
    // Storage need to be wrapped in Result, like on web version (depending on Browser storage initialization may fail)
    use_context_provider(|| Signal::new(storage));

    // Container runtime status bar. A refresh only updates the UI: the backend keeps the
    // engine availability it was started with.
    let initial_status = consume_context::<InitialRuntimeStatus>();
    let mut status = use_signal(|| initial_status.0);
    let mut refreshing = use_signal(|| false);
    let refresh = use_callback(move |()| {
        spawn(async move {
            refreshing.set(true);
            let new_status = check_runtime().await;
            status.set(new_status);
            refreshing.set(false);
        });
    });
    // One local user owns everything: no sharing, groups or owner column
    use_context_provider(|| macworp_ui_core::sharing::SharingEnabled(false));
    use_context_provider(|| macworp_ui_core::logout::LogoutEnabled(false));
    use_context_provider(|| RuntimeStatusHandle {
        status,
        refreshing,
        refresh,
    });

    rsx! {
        document::Link { rel: "icon", href: FAVICON }
        document::Link { rel: "stylesheet", href: MAIN_CSS }
        Bootstrap {}

        Router::<Router> {}
    }
}
