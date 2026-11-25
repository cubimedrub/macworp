//! Starting the container engine for users who closed it (a stopped podman machine, Docker
//! Desktop not running), so they do not need a console. The desktop app asks first and then calls
//! [`EngineStart::start`]; nothing here runs without that call.

use std::{path::Path, time::Duration};

use super::{
    RuntimeKind,
    adjustment::{CommandRunner, Error, HostSystem, Machine, SystemCommands},
};

/// How long Docker Desktop may take until it answers after it was opened.
const DOCKER_DESKTOP_READY_TIMEOUT: Duration = Duration::from_secs(180);
/// Wait between two checks whether Docker Desktop answers.
const DOCKER_DESKTOP_POLL: Duration = Duration::from_secs(3);

/// A stopped container engine that can be started from the app.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EngineStart {
    /// A podman machine that exists but is stopped (macOS, Windows).
    PodmanMachine {
        /// Name to give to `podman machine start`
        name: String,
    },
    /// Docker Desktop is installed but not running.
    DockerDesktop,
}

impl EngineStart {
    /// Chooses what to start: a stopped podman machine first (podman is the preferred runtime),
    /// then Docker Desktop. Never on Linux, where podman and Docker run as a service or socket
    /// the user manages.
    ///
    /// # Arguments
    /// * `kinds` - runtimes the user allows, see `RuntimePreference`
    /// * `system` - the host
    /// * `machines` - podman machines, see [`Machine::parse_list`]
    /// * `docker_desktop` - whether Docker Desktop is installed
    pub fn plan(
        kinds: &[RuntimeKind],
        system: HostSystem,
        machines: &[Machine],
        docker_desktop: bool,
    ) -> Option<Self> {
        if system == HostSystem::Linux {
            return None;
        }
        if kinds.contains(&RuntimeKind::Podman)
            && let Some(machine) = machines.iter().find(|machine| !machine.running)
        {
            return Some(Self::PodmanMachine {
                name: machine.name.clone(),
            });
        }
        (kinds.contains(&RuntimeKind::Docker) && docker_desktop).then_some(Self::DockerDesktop)
    }

    /// Plans for this computer by looking at the podman machines and at Docker Desktop.
    ///
    /// # Arguments
    /// * `kinds` - runtimes the user allows
    pub async fn detect(kinds: &[RuntimeKind]) -> Option<Self> {
        let system = HostSystem::current();
        if system == HostSystem::Linux {
            return None;
        }
        let machines = if kinds.contains(&RuntimeKind::Podman) {
            match SystemCommands
                .run(
                    "podman",
                    &[
                        "machine".into(),
                        "list".into(),
                        "--format".into(),
                        "json".into(),
                    ],
                )
                .await
            {
                Ok(listed) => Machine::parse_list(&listed),
                Err(_) => Vec::new(),
            }
        } else {
            Vec::new()
        };
        let docker_desktop = kinds.contains(&RuntimeKind::Docker)
            && (Path::new("/Applications/Docker.app").exists()
                || SystemCommands
                    .run("docker", &["desktop".into(), "--help".into()])
                    .await
                    .is_ok());
        Self::plan(kinds, system, &machines, docker_desktop)
    }

    /// Name of the engine for the button and the dialog.
    pub fn name(&self) -> String {
        match self {
            Self::PodmanMachine { name } => format!("podman machine {name}"),
            Self::DockerDesktop => "Docker Desktop".to_string(),
        }
    }

    /// What the user should know before agreeing, in plain language.
    pub fn impact(&self) -> &'static str {
        match self {
            Self::PodmanMachine { .. } => {
                "The container engine (a virtual machine) starts. This takes about ten seconds."
            }
            Self::DockerDesktop => {
                "Docker Desktop opens and starts its engine. This can take a minute."
            }
        }
    }

    /// Starts the engine and returns once it runs.
    pub async fn start(&self) -> Result<(), Error> {
        self.start_with(&SystemCommands, HostSystem::current())
            .await
    }

    /// [`Self::start`] with explicit command execution.
    ///
    /// # Arguments
    /// * `commands` - runs the programs
    /// * `system` - the host: macOS can open the Docker Desktop app when its command line fails
    pub async fn start_with(
        &self,
        commands: &dyn CommandRunner,
        system: HostSystem,
    ) -> Result<(), Error> {
        let run = async |program: &str, arguments: &[&str]| {
            let arguments: Vec<String> = arguments.iter().map(|a| a.to_string()).collect();
            commands.run(program, &arguments).await.map(drop)
        };
        match self {
            Self::PodmanMachine { name } => run("podman", &["machine", "start", name]).await,
            Self::DockerDesktop => {
                // Waits until the engine answers
                let started = run("docker", &["desktop", "start"]).await;
                match (started, system) {
                    (Ok(()), _) => Ok(()),
                    (Err(error), HostSystem::MacOs) => {
                        tracing::info!("`docker desktop start` failed ({error}), opening the app");
                        run("open", &["-a", "Docker"]).await?;
                        Self::wait_until_docker_answers(commands).await
                    }
                    (Err(error), _) => Err(error),
                }
            }
        }
    }

    /// Polls `docker info` until Docker Desktop, which was just opened, answers.
    ///
    /// # Arguments
    /// * `commands` - runs the programs
    async fn wait_until_docker_answers(commands: &dyn CommandRunner) -> Result<(), Error> {
        let deadline = tokio::time::Instant::now() + DOCKER_DESKTOP_READY_TIMEOUT;
        let arguments = vec!["info".to_string()];
        loop {
            match commands.run("docker", &arguments).await {
                Ok(_) => return Ok(()),
                Err(error) if tokio::time::Instant::now() >= deadline => return Err(error),
                Err(_) => tokio::time::sleep(DOCKER_DESKTOP_POLL).await,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;
    use crate::container_runtime::BoxFuture;

    /// Records the commands and fails the ones that start with `fail`.
    #[derive(Default)]
    struct Recorder {
        calls: Mutex<Vec<String>>,
        fail: Option<&'static str>,
    }

    impl CommandRunner for Recorder {
        fn run<'a>(
            &'a self,
            program: &'a str,
            arguments: &'a [String],
        ) -> BoxFuture<'a, Result<String, Error>> {
            Box::pin(async move {
                let line = format!("{program} {}", arguments.join(" "));
                self.calls.lock().unwrap().push(line.clone());
                match self.fail {
                    Some(pattern) if line.starts_with(pattern) => Err(Error::Command {
                        command: line,
                        message: "boom".into(),
                    }),
                    _ => Ok(String::new()),
                }
            })
        }
    }

    fn machine(name: &str, running: bool) -> Machine {
        Machine {
            name: name.into(),
            provider: "applehv".into(),
            running,
        }
    }

    /// A stopped podman machine comes before Docker Desktop; Linux and running machines offer nothing.
    #[test]
    fn plans_what_to_start() {
        let both = [RuntimeKind::Podman, RuntimeKind::Docker];
        let stopped = [machine("m", false)];
        assert_eq!(
            EngineStart::plan(&both, HostSystem::MacOs, &stopped, true),
            Some(EngineStart::PodmanMachine { name: "m".into() })
        );
        assert_eq!(
            EngineStart::plan(&both, HostSystem::MacOs, &[], true),
            Some(EngineStart::DockerDesktop)
        );
        // Running machine: nothing to start for podman
        assert_eq!(
            EngineStart::plan(
                &[RuntimeKind::Podman],
                HostSystem::Windows,
                &[machine("m", true)],
                true
            ),
            None
        );
        // The user's choice of runtime limits the offer
        assert_eq!(
            EngineStart::plan(&[RuntimeKind::Docker], HostSystem::MacOs, &stopped, false),
            None
        );
        assert_eq!(
            EngineStart::plan(&both, HostSystem::Linux, &stopped, true),
            None
        );
        assert_eq!(
            EngineStart::plan(&[RuntimeKind::Wslc], HostSystem::Windows, &stopped, true),
            None
        );
    }

    #[tokio::test]
    async fn starts_the_podman_machine() {
        let recorder = Recorder::default();
        EngineStart::PodmanMachine { name: "m".into() }
            .start_with(&recorder, HostSystem::MacOs)
            .await
            .unwrap();
        assert_eq!(*recorder.calls.lock().unwrap(), ["podman machine start m"]);
    }

    /// Docker Desktop: its command line first; on macOS the app is opened when that fails.
    #[tokio::test]
    async fn docker_desktop_falls_back_to_opening_the_app_on_macos() {
        let recorder = Recorder::default();
        EngineStart::DockerDesktop
            .start_with(&recorder, HostSystem::MacOs)
            .await
            .unwrap();
        assert_eq!(*recorder.calls.lock().unwrap(), ["docker desktop start"]);

        let failing = Recorder {
            fail: Some("docker desktop"),
            ..Default::default()
        };
        EngineStart::DockerDesktop
            .start_with(&failing, HostSystem::MacOs)
            .await
            .unwrap();
        assert_eq!(
            *failing.calls.lock().unwrap(),
            ["docker desktop start", "open -a Docker", "docker info"]
        );

        // Windows has no app to open: the error of the command line is the answer
        let windows = EngineStart::DockerDesktop
            .start_with(&failing, HostSystem::Windows)
            .await;
        assert!(
            windows
                .unwrap_err()
                .to_string()
                .contains("docker desktop start")
        );
    }

    /// Starts the real Docker Desktop (closed beforehand) and checks that `docker info` answers.
    /// Opens an application, so it only runs on request.
    #[tokio::test]
    #[ignore = "starts Docker Desktop"]
    async fn starts_the_real_docker_desktop() {
        EngineStart::DockerDesktop.start().await.unwrap();
        SystemCommands
            .run("docker", &["info".into()])
            .await
            .expect("docker answers after the start");
    }
}
