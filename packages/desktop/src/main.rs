mod client;
mod components;
mod errors;
mod file;
mod pages;
mod router;
mod storage;

use std::{env, time::Duration};

use clap::{Parser, Subcommand};
use dioxus::prelude::*;
use directories::ProjectDirs;
use rand::prelude::*;

use macworp_core::logging::init_tracing_subscription;
use macworp_ui_core::{
    client::Client, components::Bootstrap, configuration::Configuration, storage::IsStorage,
    FAVICON, MAIN_CSS,
};

use crate::{errors::Error, router::Router, storage::Storage};

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
    /// Verbosity level
    /// 0 - Error,
    /// 1 - Warn,
    /// 2 - Info,
    /// 3 - Debug,
    /// 4 - Trace
    /// > 4 Set to tracing and remove all filter directives for underlaying crates
    #[arg(short, long, action = clap::ArgAction::Count)]
    verbose: u8,
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
        match start().await {
            Ok(_) => tracing::info!("Application stopped successfully"),
            Err(e) => tracing::error!("Application stopped with error: {e}"),
        }
    }

    Ok(())
}

/// Starts the desktop application with the backend running in the background
///
async fn start() -> Result<(), Error> {
    let app_directory =
        ProjectDirs::from("de.ruhr-uni-bochum", "CUBiMed.RUB", env!("CARGO_PKG_NAME"))
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

    let mut backend_config = macworp_backend::configuration::Configuration::default();
    *backend_config.database_url_mut() = database_url;
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

    let backend = tokio::spawn(async {
        match macworp_backend::Backend::<
            macworp_backend::filesystems::local_filesystem::LocalFilesystem,
        >::start(
            backend_listener,
            Box::pin(backend_shutdown_signal),
            backend_config,
            DESKTOP_USER_NAME.to_string(),
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
        .launch(App);

    backend_shutdown_signal_sender
        .send(())
        .map_err(|_| Error::BackendStopSignal)?;

    backend
        .await
        .map_err(Error::BackendStopping)?
        .map_err(Error::Backend)
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

    rsx! {
        document::Link { rel: "icon", href: FAVICON }
        document::Link { rel: "stylesheet", href: MAIN_CSS }
        Bootstrap {}

        Router::<Router> {}
    }
}
