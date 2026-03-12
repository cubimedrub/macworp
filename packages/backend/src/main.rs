use clap::{Parser, Subcommand};
use macworp_backend::start;
use macworp_core::logging::init_tracing_subscription;

#[derive(Debug, Subcommand)]
enum SubCommand {
    /// Start the backend server
    Serve {
        /// Interface to bind to
        interface: String,
        /// Port to bind to, 0 will ask the OS for a free port.
        port: u16,
    },
    /// Show version information
    Version {},
}

#[derive(Debug, Parser)]
#[command(name = "macworp")]
struct CommandLineInterface {
    /// Verbosity level
    /// 0 - Error,
    /// 1 - Warn,
    /// 2 - Info,
    /// 3 - Debug,
    /// > 3 - Trace
    #[arg(short, long, action = clap::ArgAction::Count)]
    verbose: u8,
    #[command(subcommand)]
    subcommand: SubCommand,
}

#[tokio::main]
async fn main() {
    let cli_args = CommandLineInterface::parse();

    match cli_args.subcommand {
        SubCommand::Serve { interface, port } => {
            init_tracing_subscription(cli_args.verbose, true);

            let bind_address = format!("{}:{}", interface, port);
            let listener = tokio::net::TcpListener::bind(bind_address).await.unwrap();
            if port == 0 {
                tracing::info!("Starting on port {}", listener.local_addr().unwrap().port());
            }
            start(listener, Box::pin(shutdown_signal())).await;
        }
        SubCommand::Version {} => {
            println!("{}", env!("CARGO_PKG_VERSION"))
        }
    }
}

/// Shutdown signal that listens for Ctrl+C or SIGTERM and completes when either of them is received.
async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}
