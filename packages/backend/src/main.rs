use std::path::{Path, PathBuf};

use clap::{Parser, Subcommand, ValueEnum};
use macworp_backend::{
    Backend,
    configuration::Configuration,
    filesystems::remote_filesystem::RemoteFilesystem,
    models::{ActiveUser, User},
    standalone_error::StandaloneError,
};
use macworp_core::logging::init_tracing_subscription;
use macworp_database_migration::{Migrator, MigratorTrait};
use sea_orm::Database;

static DEFAULT_LOGGING_DIRECTIVES: &[&str] = &["axum=error", "sqlx=error"];

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
pub enum MigrationDirection {
    /// Migration up
    Up,
    /// Migration down
    Down,
}

#[derive(Debug, Subcommand)]
enum UserSubCommand {
    /// Create a new user
    Create {
        /// Configuration path
        #[arg(short, long)]
        config: Option<PathBuf>,
        /// Flag for making new user admin
        #[arg(short, long, default_value_t = false, action = clap::ArgAction::SetTrue)]
        is_admin: bool,
        /// Username of the new user
        username: String,
        /// firstname of the new user
        firstname: String,
        /// lastname of the new user
        lastname: String,
        /// email of the new user
        email: String,
        /// Read the password from this file instead of prompting
        #[arg(long)]
        password_file: Option<PathBuf>,
    },
    /// Hashs the given password, e.g. for the
    Password {
        /// Read the password from this file instead of prompting
        #[arg(long)]
        password_file: Option<PathBuf>,
    },
    /// Make an existing user an administrator
    Promote {
        /// Configuration path
        #[arg(short, long)]
        config: Option<PathBuf>,
        /// Username of the user to promote
        username: String,
    },
    /// Remove the administrator flag of a user
    Demote {
        /// Configuration path
        #[arg(short, long)]
        config: Option<PathBuf>,
        /// Username of the user to demote
        username: String,
    },
}

#[derive(Debug, Subcommand)]
enum SubCommand {
    /// Prints a new config
    Config {},
    /// Database migration, default is updated to latest migration
    DbMigrate {
        /// Direction ot migrate
        #[arg(short, long)]
        direction: Option<MigrationDirection>,
        /// Steps to migratre up or down
        #[arg(short, long)]
        steps: Option<u32>,
        /// Configuration path
        config: Option<PathBuf>,
    },
    /// Start the backend server
    Serve {
        /// Configuration path
        config: Option<PathBuf>,
    },
    /// Manage users
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
        SubCommand::DbMigrate {
            config,
            direction,
            steps,
        } => {
            init_tracing_subscription(cli_args.verbose, true, DEFAULT_LOGGING_DIRECTIVES)?;
            let config = match config {
                Some(config_path) => {
                    let config_raw = std::fs::read_to_string(config_path)
                        .map_err(StandaloneError::ConfigurationReadError)?;

                    toml::from_str::<Configuration>(&config_raw)
                        .map_err(StandaloneError::ConfigurationParseError)?
                }
                None => return Err(StandaloneError::ConfigurationMissing),
            };

            let db = Database::connect(config.database_url())
                .await
                .map_err(StandaloneError::DatabaseConnection)?;

            match direction {
                Some(MigrationDirection::Down) => Migrator::down(&db, steps)
                    .await
                    .map_err(macworp_backend::error::Error::DatabaseMigration)?,
                _ => Migrator::up(&db, steps)
                    .await
                    .map_err(macworp_backend::error::Error::DatabaseMigration)?,
            }
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
                None => return Err(StandaloneError::ConfigurationMissing),
            };

            let bind_address = format!("{}:{}", config.interface(), config.port());
            let listener = tokio::net::TcpListener::bind(bind_address).await.unwrap();
            if config.port() == 0 {
                tracing::info!("Starting on port {}", listener.local_addr().unwrap().port());
            }
            Backend::<RemoteFilesystem>::start(listener, Box::pin(shutdown_signal()), config)
                .await
                .map_err(StandaloneError::Backend)?;
        }
        SubCommand::User { subcommand } => match subcommand {
            UserSubCommand::Create {
                config,
                is_admin,
                username,
                firstname,
                lastname,
                email,
                password_file,
            } => {
                let password = read_password(password_file.as_deref())?;

                let config = match config {
                    Some(config_path) => {
                        let config_raw = std::fs::read_to_string(config_path)
                            .map_err(StandaloneError::ConfigurationReadError)?;

                        toml::from_str::<Configuration>(&config_raw)
                            .map_err(StandaloneError::ConfigurationParseError)?
                    }
                    None => return Err(StandaloneError::ConfigurationMissing),
                };

                let db = Database::connect(config.database_url())
                    .await
                    .map_err(StandaloneError::DatabaseConnection)?;

                let password_hash = password_auth::generate_hash(password.as_bytes());

                let new_user = ActiveUser::builder()
                    .set_username(username)
                    .set_firstname(firstname)
                    .set_lastname(lastname)
                    .set_email(email)
                    .set_password(password_hash)
                    .set_is_admin(is_admin);

                new_user.insert(&db).await.map_err(|err| {
                    macworp_backend::error::Error::Database(
                        "creating a new user via CLI".to_string(),
                        err,
                    )
                })?;
            }
            UserSubCommand::Promote { config, username } => {
                set_admin(config, &username, true).await?;
                println!("{username} is now an administrator");
            }
            UserSubCommand::Demote { config, username } => {
                set_admin(config, &username, false).await?;
                println!("{username} is no longer an administrator");
            }
            UserSubCommand::Password { password_file } => {
                let password = read_password(password_file.as_deref())?;

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

/// Reads a new password from `password_file` (one trailing line break removed, no confirmation)
/// or, without a file, prompts for it twice.
fn read_password(password_file: Option<&Path>) -> Result<String, StandaloneError> {
    let password = match password_file {
        Some(path) => {
            let mut password =
                std::fs::read_to_string(path).map_err(StandaloneError::PasswordRead)?;
            if password.ends_with('\n') {
                password.pop();
                if password.ends_with('\r') {
                    password.pop();
                }
            }
            password
        }
        None => {
            let password =
                rpassword::prompt_password("Password: ").map_err(StandaloneError::PasswordRead)?;
            let password_confirmation = rpassword::prompt_password("Repeat password: ")
                .map_err(StandaloneError::PasswordRead)?;

            if password != password_confirmation {
                return Err(StandaloneError::PasswordConfirmation);
            }
            password
        }
    };

    if password.is_empty() {
        return Err(StandaloneError::PasswordEmpty);
    }
    Ok(password)
}

/// Connects to the database named in the configuration file and sets the administrator flag of `username`.
async fn set_admin(
    config: Option<PathBuf>,
    username: &str,
    is_admin: bool,
) -> Result<(), StandaloneError> {
    let config = match config {
        Some(config_path) => {
            let config_raw = std::fs::read_to_string(config_path)
                .map_err(StandaloneError::ConfigurationReadError)?;

            toml::from_str::<Configuration>(&config_raw)
                .map_err(StandaloneError::ConfigurationParseError)?
        }
        None => return Err(StandaloneError::ConfigurationMissing),
    };
    let db = Database::connect(config.database_url())
        .await
        .map_err(StandaloneError::DatabaseConnection)?;
    User::set_admin(&db, username, is_admin).await?;
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Writes `content` to a unique temporary file and returns its path.
    fn password_file(name: &str, content: &str) -> PathBuf {
        let path =
            std::env::temp_dir().join(format!("macworp_password_{}_{name}", std::process::id()));
        std::fs::write(&path, content).unwrap();
        path
    }

    /// One trailing line break (LF or CRLF) is removed, other whitespace is kept.
    #[test]
    fn password_file_strips_one_line_break_only() {
        let lf = password_file("lf", " se cret \n");
        let crlf = password_file("crlf", "secret\r\n");
        let double = password_file("double", "secret\n\n");

        assert_eq!(read_password(Some(&lf)).unwrap(), " se cret ");
        assert_eq!(read_password(Some(&crlf)).unwrap(), "secret");
        assert_eq!(read_password(Some(&double)).unwrap(), "secret\n");

        for path in [lf, crlf, double] {
            std::fs::remove_file(path).unwrap();
        }
    }

    /// An empty password file (or one holding only a line break) is rejected.
    #[test]
    fn empty_password_file_is_rejected() {
        let empty = password_file("empty", "\n");

        assert!(matches!(
            read_password(Some(&empty)),
            Err(StandaloneError::PasswordEmpty)
        ));

        std::fs::remove_file(empty).unwrap();
    }

    /// A missing password file is reported as a read error.
    #[test]
    fn missing_password_file_is_a_read_error() {
        let missing = std::env::temp_dir().join("macworp_password_does_not_exist");

        assert!(matches!(
            read_password(Some(&missing)),
            Err(StandaloneError::PasswordRead(_))
        ));
    }
}
