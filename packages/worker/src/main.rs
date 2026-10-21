use clap::{Parser, Subcommand};
use macworp_worker::{configuration::WorkerConfiguration, container_runtime};
use tracing::Level;

#[derive(Debug, Subcommand)]
enum Command {
    /// Start the worker (default)
    Run,
    /// Check that a usable container runtime is available and print the result
    Doctor,
}

#[derive(Debug, Parser)]
#[command(name = env!("CARGO_PKG_NAME"), version)]
struct CommandLineInterface {
    /// Verbosity level: 0 error, 1 warn, 2 info, 3 debug, 4+ trace
    #[arg(short, long, action = clap::ArgAction::Count)]
    verbose: u8,
    #[command(flatten)]
    configuration: WorkerConfiguration,
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
            Ok(runtime) => {
                tracing::info!("Using container runtime {}", runtime.kind());
                tracing::warn!("Worker has no executors yet, exiting");
                std::process::ExitCode::SUCCESS
            }
            Err(error) => {
                eprintln!("No usable container runtime:\n{error}");
                std::process::ExitCode::FAILURE
            }
        },
    }
}
