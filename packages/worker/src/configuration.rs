use clap::Args;

use crate::container_runtime::RuntimePreference;

/// Worker configuration, usable as clap arguments (worker binary) or built directly (embedded worker)
#[derive(Debug, Clone, Default, Args)]
pub struct WorkerConfiguration {
    /// Container runtime to use for running workflow engines
    #[arg(
        long,
        env = "MACWORP_CONTAINER_RUNTIME",
        value_enum,
        default_value_t = RuntimePreference::Auto
    )]
    pub container_runtime: RuntimePreference,
}

/// How a remote worker reaches and identifies itself to the backend, usable as clap arguments.
/// Not needed by the embedded desktop worker, which reads the database directly.
#[derive(Debug, Clone, Default, Args)]
pub struct RemoteWorkerConfiguration {
    /// Backend base URL, e.g. https://macworp.example.org
    #[arg(long, env = "MACWORP_BACKEND_URL")]
    pub backend_url: Option<String>,
    /// Username of the worker credential configured in the backend
    #[arg(long, env = "MACWORP_WORKER_USERNAME", default_value = "worker")]
    pub username: String,
    /// Password of the worker credential, prefer `--password-file` (arguments are visible to
    /// other local users)
    #[arg(long, env = "MACWORP_WORKER_PASSWORD", hide_env_values = true)]
    pub password: Option<String>,
    /// File holding the password, one trailing line break is ignored
    #[arg(
        long,
        env = "MACWORP_WORKER_PASSWORD_FILE",
        conflicts_with = "password"
    )]
    pub password_file: Option<std::path::PathBuf>,
    /// Name of this worker process in the backend, defaults to `$HOSTNAME` or `worker`
    #[arg(long, env = "MACWORP_WORKER_NAME")]
    pub name: Option<String>,
    /// Runs this worker executes at the same time
    #[arg(long, env = "MACWORP_WORKER_CAPACITY", default_value_t = 1, value_parser = clap::value_parser!(u16).range(1..))]
    pub capacity: u16,
}

impl RemoteWorkerConfiguration {
    /// The backend URL, required to run a remote worker
    pub fn backend_url(&self) -> Result<&str, String> {
        self.backend_url
            .as_deref()
            .ok_or_else(|| "--backend-url (MACWORP_BACKEND_URL) is required".to_string())
    }

    /// The password from `--password` or the password file; the file's trailing line break is
    /// dropped. Empty passwords are rejected.
    pub fn resolve_password(&self) -> Result<String, String> {
        let password = match (&self.password, &self.password_file) {
            (Some(password), _) => password.clone(),
            (None, Some(path)) => {
                let content = std::fs::read_to_string(path)
                    .map_err(|e| format!("Cannot read password file {}: {e}", path.display()))?;
                content
                    .strip_suffix("\r\n")
                    .or_else(|| content.strip_suffix('\n'))
                    .unwrap_or(&content)
                    .to_string()
            }
            (None, None) => {
                return Err(
                    "--password-file (MACWORP_WORKER_PASSWORD_FILE) or --password is required"
                        .to_string(),
                );
            }
        };
        if password.is_empty() {
            return Err("The worker password is empty".to_string());
        }
        Ok(password)
    }

    /// Name reported to the backend: `--name`, else `$HOSTNAME`, else `worker`
    pub fn worker_name(&self) -> String {
        self.name
            .clone()
            .or_else(|| {
                std::env::var("HOSTNAME")
                    .ok()
                    .filter(|name| !name.is_empty())
            })
            .unwrap_or_else(|| "worker".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Password file: one trailing line break is stripped, an empty file is rejected
    #[test]
    fn password_file_is_trimmed_and_must_not_be_empty() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("password");
        std::fs::write(&path, "secret\n").unwrap();
        let configuration = RemoteWorkerConfiguration {
            password_file: Some(path.clone()),
            ..Default::default()
        };
        assert_eq!(configuration.resolve_password().unwrap(), "secret");
        std::fs::write(&path, "\n").unwrap();
        assert!(configuration.resolve_password().is_err());
        assert!(
            RemoteWorkerConfiguration::default()
                .resolve_password()
                .is_err()
        );
    }
}
