use std::sync::Arc;

use clap::{Parser, Subcommand};
use macworp_worker::{
    configuration::{RemoteWorkerConfiguration, WorkerConfiguration},
    container_runtime::{self, ContainerRuntime},
    executors::standard_executors,
    http_job_source::HttpJobSource,
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

    let report = container_runtime::doctor(cli.configuration.container_runtime).await;

    match cli.command.unwrap_or(Command::Run) {
        Command::Doctor => {
            println!("{report}");
            if report.is_ok() {
                std::process::ExitCode::SUCCESS
            } else {
                std::process::ExitCode::FAILURE
            }
        }
        Command::Run => match report.into_runtime() {
            Ok(runtime) => match run_remote(&cli.remote, runtime).await {
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
async fn run_remote(
    remote: &RemoteWorkerConfiguration,
    runtime: Arc<dyn ContainerRuntime>,
) -> Result<(), String> {
    let source = Arc::new(
        HttpJobSource::new(
            remote.backend_url()?,
            remote.username.clone(),
            remote.resolve_password()?,
            remote.worker_name(),
        )
        .map_err(|e| e.to_string())?,
    );
    let executors = standard_executors(&runtime).await;
    if executors.is_empty() {
        return Err(format!(
            "The container runtime {} supports no engine",
            runtime.kind()
        ));
    }
    let progress = match HostAccess::for_runtime(runtime.kind()) {
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
    let mut slots = Vec::new();
    for _ in 0..remote.capacity {
        let mut worker = Worker::new(source.clone(), executors.clone(), WorkerOptions::default());
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
        runtime.kind()
    );

    tokio::signal::ctrl_c()
        .await
        .map_err(|e| format!("Cannot listen for Ctrl-C: {e}"))?;
    tracing::info!("Stopping, waiting for running jobs to finish");
    let _ = stop.send(true);
    for slot in slots {
        let _ = slot.await;
    }
    Ok(())
}
