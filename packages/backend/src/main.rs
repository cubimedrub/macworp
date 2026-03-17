use std::path::PathBuf;

use clap::{Parser, Subcommand};
use macworp_backend::{Backend, configuration::Configuration, standalone_error::StandaloneError};
use macworp_core::logging::init_tracing_subscription;
use macworp_database_migration::{Migrator, MigratorTrait};
use sea_orm::Database;

static DEFAULT_LOGGING_DIRECTIVES: &[&str] = &["axum=error"];

#[derive(Debug, Subcommand)]
enum SubCommand {
    /// Prints a new config
    Config {},
    /// Database migration
    DbMigrate {
        /// Configuration path
        config: Option<PathBuf>,
    },
    /// Start the backend server
    Serve {
        /// Configuration path
        config: Option<PathBuf>,
    },
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
    subcommand: SubCommand,
}

#[tokio::main]
async fn main() -> Result<(), StandaloneError> {
    let cli_args = CommandLineInterface::parse();

    match cli_args.subcommand {
        SubCommand::Config {} => {
            let config = Configuration::default();

            println!("{}", toml::to_string_pretty(&config).unwrap());
        }
        SubCommand::DbMigrate { config } => {
            init_tracing_subscription(cli_args.verbose, true, DEFAULT_LOGGING_DIRECTIVES)?;
            let config = match config {
                Some(config_path) => {
                    let config_raw = std::fs::read_to_string(config_path)
                        .map_err(StandaloneError::ConfigurationReadError)?;

                    toml::from_str::<Configuration>(&config_raw)
                        .map_err(StandaloneError::ConfigurationParseError)?
                }
                None => Configuration::default(),
            };

            let db = Database::connect(config.database_url())
                .await
                .map_err(StandaloneError::DatabaseConnection)?;

            Migrator::up(&db, None)
                .await
                .map_err(macworp_backend::error::Error::DatabaseMigration)?
        }
        SubCommand::Serve { config } => {
            init_tracing_subscription(cli_args.verbose, true, DEFAULT_LOGGING_DIRECTIVES)?;

            let config = match config {
                Some(config_path) => {
                    let config_raw = std::fs::read_to_string(config_path)
                        .map_err(StandaloneError::ConfigurationReadError)?;

                    toml::from_str::<Configuration>(&config_raw)
                        .map_err(StandaloneError::ConfigurationParseError)?
                }
                None => Configuration::default(),
            };

            let bind_address = format!("{}:{}", config.interface(), config.port());
            let listener = tokio::net::TcpListener::bind(bind_address).await.unwrap();
            if config.port() == 0 {
                tracing::info!("Starting on port {}", listener.local_addr().unwrap().port());
            }
            Backend::start_standalone(listener, Box::pin(shutdown_signal()), config)
                .await
                .map_err(StandaloneError::Backend)?;
        }
        SubCommand::Version {} => {
            println!("{}", env!("CARGO_PKG_VERSION"))
        }
    }
    Ok(())
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
