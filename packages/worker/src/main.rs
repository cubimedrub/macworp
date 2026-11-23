use std::sync::Arc;

use clap::{Parser, Subcommand};
use macworp_core::workflow_configuration::engine::Engine;
use macworp_worker::{
    configuration::{RemoteWorkerConfiguration, WorkerConfiguration},
    container_runtime::{self, ContainerRuntime},
    executors::standard_executors,
    http_job_source::HttpJobSource,
    image_builder::ImageBuilder,
    log_proxy::{HostAccess, ProgressReceiver},
    worker::{Worker, WorkerOptions},
};
use tokio::sync::watch;
use tracing::Level;

#[derive(Debug, Subcommand)]
enum Command {
    /// Start the worker (default)
    Run,
    /// Check that a usable container runtime is available and print the result
    Doctor,
}

#[derive(Debug, Parser)]
#[command(name = env!("CARGO_PKG_NAME"), version, about = "MAcWorP worker: runs queued workflows from a backend in container engines")]
struct CommandLineInterface {
    /// Verbosity level: 0 error, 1 warn, 2 info, 3 debug, 4+ trace
    #[arg(short, long, action = clap::ArgAction::Count)]
    verbose: u8,
    #[command(flatten)]
    configuration: WorkerConfiguration,
    #[command(flatten)]
    remote: RemoteWorkerConfiguration,
    #[command(subcommand)]
    command: Option<Command>,
}

#[tokio::main]
async fn main() -> std::process::ExitCode {
    let cli = CommandLineInterface::parse();

    tracing_subscriber::fmt()
        .with_max_level(match cli.verbose {
            0 => Level::ERROR,
            1 => Level::WARN,
            2 => Level::INFO,
            3 => Level::DEBUG,
            _ => Level::TRACE,
        })
        .init();

    let mut report = container_runtime::doctor(cli.configuration.container_runtime).await;
    if let Ok(storage) = cli.remote.storage_path() {
        report.check_label_of(&storage);
    }

    match cli.command.unwrap_or(Command::Run) {
        Command::Doctor => {
            println!("{report}");
            if report.is_ok() {
                std::process::ExitCode::SUCCESS
            } else {
                std::process::ExitCode::FAILURE
            }
        }
        Command::Run => match report.into_runtimes() {
            Ok(runtimes) => match run_remote(&cli.configuration, &cli.remote, runtimes).await {
                Ok(()) => std::process::ExitCode::SUCCESS,
                Err(error) => {
                    eprintln!("{error}");
                    std::process::ExitCode::FAILURE
                }
            },
            Err(error) => {
                eprintln!("No usable container runtime:\n{error}");
                std::process::ExitCode::FAILURE
            }
        },
    }
}

/// Runs `capacity` job slots against the backend until Ctrl-C; running jobs finish first.
///
/// # Arguments
/// * `configuration` - worker settings (image removal limits)
/// * `remote` - backend connection and worker settings
/// * `runtimes` - usable container runtimes, most preferred first; each engine runs on the
///   first one that supports it
async fn run_remote(
    configuration: &WorkerConfiguration,
    remote: &RemoteWorkerConfiguration,
    runtimes: Vec<Arc<dyn ContainerRuntime>>,
) -> Result<(), String> {
    let mut kinds = Vec::new();
    let mut versions = Vec::new();
    for runtime in &runtimes {
        kinds.push(runtime.kind().to_string());
        if let Ok(info) = runtime.info().await {
            versions.push(format!("{} {}", info.kind, info.version));
        }
    }
    let runtime_name = kinds.join(", ");
    let runtime_version = (!versions.is_empty()).then(|| versions.join(", "));
    let executors = standard_executors(&runtimes).await;
    if executors.is_empty() {
        return Err(format!(
            "The container runtime(s) {runtime_name} support no engine"
        ));
    }
    let engines: Vec<Engine> = executors.iter().map(|executor| executor.engine()).collect();
    let source = Arc::new(
        HttpJobSource::new(
            remote.backend_url()?,
            remote.storage_path()?,
            remote.username.clone(),
            remote.resolve_password()?,
            remote.worker_name(),
            engines.clone(),
        )
        .map_err(|e| e.to_string())?,
    );
    let progress = match runtimes
        .iter()
        .find_map(|runtime| HostAccess::for_runtime(runtime.kind()))
    {
        Some(access) => match ProgressReceiver::start(access).await {
            Ok(receiver) => Some(Arc::new(receiver)),
            Err(e) => {
                tracing::warn!(
                    "Cannot start the engine progress receiver, runs show no progress: {e}"
                );
                None
            }
        },
        None => None,
    };

    let (stop, stopped) = watch::channel(false);
    let announcer = tokio::spawn(announce_loop(
        source.clone(),
        runtime_name.clone(),
        runtime_version,
        engines,
        stopped.clone(),
    ));
    // One set for all slots: they share the image store, so they share the name locks
    let builders: Vec<ImageBuilder> = runtimes
        .iter()
        .map(|runtime| ImageBuilder::for_runtime(runtime.as_ref()))
        .collect();
    let image_stores = configuration.image_stores(&runtimes);
    let apptainer_cache_store = configuration.apptainer_cache_store();
    let mut slots = Vec::new();
    for _ in 0..remote.capacity {
        let mut worker = Worker::new(source.clone(), executors.clone(), WorkerOptions::default())
            .with_image_builders(builders.clone())
            .with_image_stores(image_stores.clone())
            .with_apptainer_cache_store(apptainer_cache_store.clone());
        if let Some(progress) = &progress {
            worker = worker.with_progress_receiver(progress.clone());
        }
        let stopped = stopped.clone();
        slots.push(tokio::spawn(async move { worker.run_until(stopped).await }));
    }
    tracing::info!(
        "Worker {} started with {} slot(s) using {}",
        remote.worker_name(),
        remote.capacity,
        runtime_name
    );

    tokio::signal::ctrl_c()
        .await
        .map_err(|e| format!("Cannot listen for Ctrl-C: {e}"))?;
    tracing::info!("Stopping, waiting for running jobs to finish");
    let _ = stop.send(true);
    for slot in slots {
        let _ = slot.await;
    }
    let _ = announcer.await;
    Ok(())
}

/// Announces the worker to the backend at start and then every third of the validity the backend
/// returned (default 10 s until the first answer), until `stopped` turns true. Failures are only
/// logged: a backend that is down or restarting must not stop the worker.
///
/// # Arguments
/// * `source` - connection to the backend
/// * `runtime` - container runtime binary name
/// * `runtime_version` - its version if known
/// * `engines` - engines the worker's executors run
/// * `stopped` - turns true when the worker shuts down
async fn announce_loop(
    source: Arc<HttpJobSource>,
    runtime: String,
    runtime_version: Option<String>,
    engines: Vec<Engine>,
    mut stopped: watch::Receiver<bool>,
) {
    let mut interval = std::time::Duration::from_secs(10);
    loop {
        match source
            .announce(&runtime, runtime_version.as_deref(), &engines)
            .await
        {
            Ok(ttl_seconds) => {
                interval = std::time::Duration::from_secs(u64::from(ttl_seconds.max(3)) / 3);
            }
            Err(e) => tracing::warn!("Cannot announce to the backend: {e}"),
        }
        tokio::select! {
            _ = tokio::time::sleep(interval) => {}
            _ = stopped.changed() => return,
        }
        if *stopped.borrow() {
            return;
        }
    }
}
