use std::path::PathBuf;

use clap::{Parser, Subcommand};
use macworp_backend::{
    Backend, configuration::Configuration, models::ActiveUser, standalone_error::StandaloneError,
};
use macworp_core::logging::init_tracing_subscription;
use macworp_database_migration::{Migrator, MigratorTrait};
use sea_orm::Database;

static DEFAULT_LOGGING_DIRECTIVES: &[&str] = &["axum=error"];

#[derive(Debug, Subcommand)]
enum UserSubCommand {
    /// Create a new user
    Create {
        #[clap(short, long)]
        /// Configuration path
        config: Option<PathBuf>,
        /// Username of the new user
        username: String,
        /// firstname of the new user
        firstname: String,
        /// lastname of the new user
        lastname: String,
        /// email of the new user
        email: String,
    },
    /// Hashs the given password, e.g. for the
    Password {},
}

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
    User {
        #[command(subcommand)]
        subcommand: UserSubCommand,
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
        SubCommand::User { subcommand } => match subcommand {
            UserSubCommand::Create {
                config,
                username,
                firstname,
                lastname,
                email,
            } => {
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

                let password = rpassword::prompt_password("Password: ")
                    .map_err(StandaloneError::PasswordRead)?;
                let password_confirmation = rpassword::prompt_password("Repeat password: ")
                    .map_err(StandaloneError::PasswordRead)?;

                if password != password_confirmation {
                    return Err(StandaloneError::PasswordConfirmation);
                }

                let password_hash = password_auth::generate_hash(password.as_bytes());

                let new_user = ActiveUser::builder()
                    .set_username(username)
                    .set_firstname(firstname)
                    .set_lastname(lastname)
                    .set_email(email)
                    .set_password(password_hash);

                new_user.insert(&db).await.map_err(|err| {
                    macworp_backend::error::Error::Database(
                        "creating a new user via CLI".to_string(),
                        err,
                    )
                })?;
            }
            UserSubCommand::Password {} => {
                let password = rpassword::prompt_password("Password: ")
                    .map_err(StandaloneError::PasswordRead)?;

                let password_confirmation = rpassword::prompt_password("Repeat password: ")
                    .map_err(StandaloneError::PasswordRead)?;

                if password != password_confirmation {
                    return Err(StandaloneError::PasswordConfirmation);
                }

                let password_hash = password_auth::generate_hash(password.as_bytes());

                println!("{}", password_hash);
            }
        },
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
