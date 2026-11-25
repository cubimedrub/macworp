//! Raising the memory of the VM that runs the containers (podman machine, Docker Desktop, WSL2)
//! for users who should not have to open a console or edit a configuration file. The desktop app
//! asks first and then calls [`MemoryAdjustment::apply`]; nothing here runs without that call.

use std::{
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};

use serde_json::Value;
use thiserror::Error;
use tokio::process::Command;

use super::runtime::BoxFuture;

/// Mebibytes in a gibibyte.
const MEBIBYTES_PER_GIBIBYTE: u64 = 1024;
/// The slider starts here and its start value is never lower: below it large tasks are killed.
const RECOMMENDED_MINIMUM_MEBIBYTES: u64 = 4 * MEBIBYTES_PER_GIBIBYTE;
/// Longest a stop, start or restart of a VM may take.
const COMMAND_TIMEOUT: Duration = Duration::from_secs(180);
/// Name of the backup written next to the `.wslconfig` before it changes.
const WSL_CONFIG_BACKUP: &str = ".wslconfig.bak";

/// What a user can choose with the memory slider.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemoryRange {
    /// Lowest value of the slider
    pub minimum_mebibytes: u64,
    /// Highest value: the host's memory minus what the system keeps
    pub maximum_mebibytes: u64,
    /// Value the slider starts at: half the host's memory, between 4 and 16 GiB
    pub start_mebibytes: u64,
    /// Distance between two positions of the slider
    pub step_mebibytes: u64,
    /// Above this the host gets slow, the dialog shows a warning
    pub warning_mebibytes: u64,
}

impl MemoryRange {
    /// The range for a host with `host_bytes` of memory. The system keeps the larger of 4 GiB
    /// and a quarter of the memory, so a 32 GiB machine can give 24 GiB. Hosts too small for the
    /// recommended minimum get a range that starts at its maximum.
    ///
    /// # Arguments
    /// * `host_bytes` - memory of the computer
    pub fn for_host(host_bytes: u64) -> Self {
        let host = host_bytes / 1024 / 1024;
        let kept = (host / 4).max(RECOMMENDED_MINIMUM_MEBIBYTES);
        let step = MEBIBYTES_PER_GIBIBYTE / 2;
        let round = |mebibytes: u64| mebibytes / step * step;
        let maximum = round(host.saturating_sub(kept)).max(2 * MEBIBYTES_PER_GIBIBYTE);
        let minimum = (2 * MEBIBYTES_PER_GIBIBYTE).min(maximum);
        let start = round(host / 2)
            .clamp(RECOMMENDED_MINIMUM_MEBIBYTES, 16 * MEBIBYTES_PER_GIBIBYTE)
            .clamp(minimum, maximum);
        Self {
            minimum_mebibytes: minimum,
            maximum_mebibytes: maximum,
            start_mebibytes: start,
            step_mebibytes: step,
            warning_mebibytes: round(host / 4 * 3).max(minimum),
        }
    }

    /// Memory of this computer in mebibytes, `None` if the system does not say.
    pub fn host_mebibytes() -> Option<u64> {
        let mut system = sysinfo::System::new().ok()?;
        system.refresh_memory();
        (system.total_memory() > 0).then(|| system.total_memory() / 1024 / 1024)
    }

    /// The range for this computer, `None` if the system does not say how much memory it has.
    pub fn of_this_host() -> Option<Self> {
        Self::host_mebibytes().map(|mebibytes| Self::for_host(mebibytes * 1024 * 1024))
    }
}

/// Which system the adjustment is planned for, a parameter so the planning is testable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostSystem {
    /// Windows host
    Windows,
    /// macOS host
    MacOs,
    /// Linux host: containers use the host's memory, there is nothing to adjust
    Linux,
}

impl HostSystem {
    /// The system this program runs on.
    pub fn current() -> Self {
        if cfg!(windows) {
            Self::Windows
        } else if cfg!(target_os = "macos") {
            Self::MacOs
        } else {
            Self::Linux
        }
    }
}

/// Something went wrong while changing the memory.
#[derive(Debug, Error)]
pub enum Error {
    /// A command of the VM tooling failed
    #[error("`{command}` failed: {message}")]
    Command {
        /// The command line
        command: String,
        /// Its error output or the reason it could not run
        message: String,
    },
    /// A settings file could not be read, understood or written
    #[error("{path}: {message}")]
    File {
        /// The file
        path: PathBuf,
        /// What went wrong
        message: String,
    },
}

/// Starts programs, so the steps can be tested without a VM.
pub trait CommandRunner: Send + Sync {
    /// Runs `program` with `arguments` and fails with its error output if it does not succeed.
    ///
    /// # Arguments
    /// * `program` - executable name
    /// * `arguments` - its arguments
    fn run<'a>(
        &'a self,
        program: &'a str,
        arguments: &'a [String],
    ) -> BoxFuture<'a, Result<String, Error>>;
}

/// Runs the real programs.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemCommands;

impl CommandRunner for SystemCommands {
    fn run<'a>(
        &'a self,
        program: &'a str,
        arguments: &'a [String],
    ) -> BoxFuture<'a, Result<String, Error>> {
        Box::pin(async move {
            let command = format!("{program} {}", arguments.join(" "));
            tracing::info!("Changing the container engine memory: {command}");
            let failed = |message: String| Error::Command {
                command: command.clone(),
                message,
            };
            // Output goes to files, not pipes: `podman machine start` leaves helper processes
            // (vfkit, gvproxy) that keep an inherited pipe open for as long as the machine runs,
            // so waiting for the end of a pipe never returns. Waiting for the process does.
            let output_file = tempfile::tempfile().map_err(|e| failed(e.to_string()))?;
            let error_file = tempfile::tempfile().map_err(|e| failed(e.to_string()))?;
            let mut child = Command::new(program)
                .args(arguments)
                .stdin(Stdio::null())
                .stdout(Stdio::from(
                    output_file.try_clone().map_err(|e| failed(e.to_string()))?,
                ))
                .stderr(Stdio::from(
                    error_file.try_clone().map_err(|e| failed(e.to_string()))?,
                ))
                .kill_on_drop(true)
                .spawn()
                .map_err(|e| failed(e.to_string()))?;
            let status = tokio::time::timeout(COMMAND_TIMEOUT, child.wait())
                .await
                .map_err(|_| failed("timed out".into()))?
                .map_err(|e| failed(e.to_string()))?;
            let read = |mut file: std::fs::File| {
                use std::io::{Read, Seek, SeekFrom};
                let mut bytes = Vec::new();
                let _ = file.seek(SeekFrom::Start(0));
                let _ = file.read_to_end(&mut bytes);
                // `wsl.exe` writes UTF-16, which shows up as NUL bytes
                String::from_utf8_lossy(&bytes).replace('\0', "")
            };
            if status.success() {
                Ok(read(output_file))
            } else {
                Err(failed(format!("{status}: {}", read(error_file).trim())))
            }
        })
    }
}

/// A podman machine as `podman machine list --format json` reports it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Machine {
    /// Name to give to `podman machine start` and the like
    pub name: String,
    /// `wsl`, `hyperv`, `applehv`, `libkrun`, `qemu`
    pub provider: String,
    /// Whether it runs now
    pub running: bool,
}

impl Machine {
    /// The podman machines of this computer (running first), empty if podman cannot list them.
    pub async fn list() -> Vec<Machine> {
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
            Ok(listed) => Self::parse_list(&listed),
            Err(_) => Vec::new(),
        }
    }

    /// Parses the output of `podman machine list --format json`: the running machine first,
    /// otherwise the default one.
    ///
    /// # Arguments
    /// * `json` - the command's output
    pub fn parse_list(json: &str) -> Vec<Machine> {
        let Ok(Value::Array(entries)) = serde_json::from_str::<Value>(json) else {
            return Vec::new();
        };
        let mut machines: Vec<(bool, Machine)> = entries
            .iter()
            .filter_map(|entry| {
                let text = |key: &str| entry.get(key).and_then(Value::as_str);
                let flag = |key: &str| entry.get(key).and_then(Value::as_bool).unwrap_or(false);
                Some((
                    flag("Default"),
                    Machine {
                        name: text("Name")?.trim_end_matches('*').to_string(),
                        provider: text("VMType")?.to_string(),
                        running: flag("Running"),
                    },
                ))
            })
            .collect();
        machines.sort_by_key(|(default, machine)| (!machine.running, !default));
        machines.into_iter().map(|(_, machine)| machine).collect()
    }
}

/// How the memory of the container VM is changed on this computer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MemoryAdjustment {
    /// A podman machine with its own memory setting (macOS, Windows with Hyper-V): the machine
    /// is stopped, changed and started.
    PodmanMachine {
        /// Name of the machine
        name: String,
    },
    /// WSL2 on Windows: every distro (podman machine, Docker Desktop) shares one limit that
    /// lives in the user's `.wslconfig`. The file is backed up and changed, then WSL is shut
    /// down and what ran on it is started again.
    Wsl2 {
        /// `%UserProfile%\.wslconfig`
        configuration: PathBuf,
        /// Name of the podman machine to start again, if it is the runtime
        podman_machine: Option<String>,
        /// Whether Docker Desktop is the runtime and has to be restarted
        docker_desktop: bool,
    },
    /// Docker Desktop on macOS: its settings file has the memory, Docker Desktop is restarted.
    DockerDesktop {
        /// `settings-store.json` of Docker Desktop
        settings: PathBuf,
    },
}

impl MemoryAdjustment {
    /// Plans the change for the runtime in use, `None` where there is nothing to adjust (Linux,
    /// wslc) or the tooling is not found.
    ///
    /// # Arguments
    /// * `kind` - the runtime
    /// * `system` - the host
    /// * `machines` - podman machines, see [`Machine::parse_list`]
    /// * `home` - the user's home directory
    pub fn plan(
        kind: super::RuntimeKind,
        system: HostSystem,
        machines: &[Machine],
        home: &Path,
    ) -> Option<Self> {
        use super::RuntimeKind;
        match (kind, system) {
            (RuntimeKind::Podman, HostSystem::Linux) | (RuntimeKind::Wslc, _) => None,
            (RuntimeKind::Docker, HostSystem::Linux) => None,
            (RuntimeKind::Podman, _) => {
                let machine = machines.first()?;
                Some(if machine.provider == "wsl" {
                    Self::Wsl2 {
                        configuration: home.join(".wslconfig"),
                        podman_machine: Some(machine.name.clone()),
                        docker_desktop: false,
                    }
                } else {
                    Self::PodmanMachine {
                        name: machine.name.clone(),
                    }
                })
            }
            (RuntimeKind::Docker, HostSystem::Windows) => Some(Self::Wsl2 {
                configuration: home.join(".wslconfig"),
                podman_machine: None,
                docker_desktop: true,
            }),
            (RuntimeKind::Docker, HostSystem::MacOs) => {
                let settings =
                    home.join("Library/Group Containers/group.com.docker/settings-store.json");
                settings
                    .is_file()
                    .then_some(Self::DockerDesktop { settings })
            }
        }
    }

    /// Plans for this computer, querying `podman machine list` when podman is the runtime.
    ///
    /// # Arguments
    /// * `kind` - the runtime
    pub async fn detect(kind: super::RuntimeKind) -> Option<Self> {
        let machines = if kind == super::RuntimeKind::Podman {
            Machine::list().await
        } else {
            Vec::new()
        };
        let home = directories::UserDirs::new()?.home_dir().to_path_buf();
        Self::plan(kind, HostSystem::current(), &machines, &home)
    }

    /// What the user should know before agreeing, in plain language.
    pub fn impact(&self) -> &'static str {
        match self {
            Self::PodmanMachine { .. } => {
                "The container engine (podman machine) is stopped and started again. Running workflows are stopped."
            }
            Self::Wsl2 { .. } => {
                "Windows Subsystem for Linux is shut down and started again. This also stops other Linux distributions and Docker Desktop's engine for a moment, and running workflows are stopped. Your .wslconfig file is backed up first."
            }
            Self::DockerDesktop { .. } => {
                "Docker Desktop is restarted. Running workflows are stopped."
            }
        }
    }

    /// Sets the memory and restarts what has to restart. A changed `.wslconfig` is restored when
    /// a step fails.
    ///
    /// # Arguments
    /// * `mebibytes` - new memory of the VM, see [`MemoryRange`]
    pub async fn apply(&self, mebibytes: u64) -> Result<(), Error> {
        self.apply_with(&SystemCommands, mebibytes).await
    }

    /// [`Self::apply`] with explicit command execution.
    ///
    /// # Arguments
    /// * `commands` - runs the programs
    /// * `mebibytes` - new memory of the VM
    pub async fn apply_with(
        &self,
        commands: &dyn CommandRunner,
        mebibytes: u64,
    ) -> Result<(), Error> {
        let run = async |program: &str, arguments: &[&str]| {
            let arguments: Vec<String> = arguments.iter().map(|a| a.to_string()).collect();
            commands.run(program, &arguments).await.map(drop)
        };
        match self {
            Self::PodmanMachine { name } => {
                run("podman", &["machine", "stop", name]).await?;
                let memory = mebibytes.to_string();
                let set = run("podman", &["machine", "set", "--memory", &memory, name]).await;
                // Start again even when the change failed, the user had a running engine
                let start = run("podman", &["machine", "start", name]).await;
                set.and(start)
            }
            Self::Wsl2 {
                configuration,
                podman_machine,
                docker_desktop,
            } => {
                if let Some(name) = podman_machine {
                    let _ = run("podman", &["machine", "stop", name]).await;
                }
                let backup = Self::change_wsl_configuration(configuration, mebibytes)?;
                let result = run("wsl", &["--shutdown"]).await;
                if let Err(error) = result {
                    Self::restore(configuration, backup.as_deref());
                    return Err(error);
                }
                if let Some(name) = podman_machine {
                    run("podman", &["machine", "start", name]).await?;
                }
                if *docker_desktop {
                    run("docker", &["desktop", "restart"]).await?;
                }
                Ok(())
            }
            Self::DockerDesktop { settings } => {
                let original = std::fs::read_to_string(settings).map_err(|e| Error::File {
                    path: settings.clone(),
                    message: e.to_string(),
                })?;
                let changed =
                    Self::docker_settings_with_memory(&original, mebibytes).map_err(|message| {
                        Error::File {
                            path: settings.clone(),
                            message,
                        }
                    })?;
                std::fs::write(settings, changed).map_err(|e| Error::File {
                    path: settings.clone(),
                    message: e.to_string(),
                })?;
                if let Err(error) = run("docker", &["desktop", "restart"]).await {
                    let _ = std::fs::write(settings, original);
                    return Err(error);
                }
                Ok(())
            }
        }
    }

    /// Writes the new `.wslconfig`, after saving the old one next to it.
    ///
    /// # Arguments
    /// * `configuration` - the `.wslconfig`
    /// * `mebibytes` - new memory limit
    ///
    /// # Returns
    /// The previous content, `None` if the file did not exist.
    fn change_wsl_configuration(
        configuration: &Path,
        mebibytes: u64,
    ) -> Result<Option<String>, Error> {
        let file_error = |e: std::io::Error| Error::File {
            path: configuration.to_path_buf(),
            message: e.to_string(),
        };
        let previous = match std::fs::read_to_string(configuration) {
            Ok(text) => Some(text),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => return Err(file_error(e)),
        };
        if let Some(previous) = &previous {
            let backup = configuration.with_file_name(WSL_CONFIG_BACKUP);
            std::fs::write(&backup, previous).map_err(|e| Error::File {
                path: backup,
                message: e.to_string(),
            })?;
        }
        let changed =
            Self::wsl_configuration_with_memory(previous.as_deref().unwrap_or(""), mebibytes);
        std::fs::write(configuration, changed).map_err(file_error)?;
        Ok(previous)
    }

    /// Puts the file back as it was.
    ///
    /// # Arguments
    /// * `configuration` - the `.wslconfig`
    /// * `previous` - its content before the change, `None` if it did not exist (then it is removed)
    fn restore(configuration: &Path, previous: Option<&str>) {
        let _ = match previous {
            Some(text) => std::fs::write(configuration, text),
            None => std::fs::remove_file(configuration),
        };
    }

    /// The `.wslconfig` text with `memory` set in the `[wsl2]` section; every other line, key and
    /// section stays as it is.
    ///
    /// # Arguments
    /// * `text` - current content, may be empty
    /// * `mebibytes` - new memory limit
    pub fn wsl_configuration_with_memory(text: &str, mebibytes: u64) -> String {
        let value = format!("memory={mebibytes}MB");
        let newline = if text.contains("\r\n") || text.is_empty() {
            "\r\n"
        } else {
            "\n"
        };
        let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
        let is_section = |line: &str| line.trim().starts_with('[');
        let in_wsl2 = |line: &str| line.trim().eq_ignore_ascii_case("[wsl2]");
        match lines.iter().position(|line| in_wsl2(line)) {
            Some(start) => {
                let end = lines[start + 1..]
                    .iter()
                    .position(|line| is_section(line))
                    .map_or(lines.len(), |offset| start + 1 + offset);
                let existing = (start + 1..end).find(|&index| {
                    lines[index]
                        .split('=')
                        .next()
                        .is_some_and(|key| key.trim().eq_ignore_ascii_case("memory"))
                });
                match existing {
                    Some(index) => lines[index] = value,
                    None => lines.insert(start + 1, value),
                }
            }
            None => {
                if lines.last().is_some_and(|line| !line.trim().is_empty()) {
                    lines.push(String::new());
                }
                lines.push("[wsl2]".to_string());
                lines.push(value);
            }
        }
        lines.join(newline) + newline
    }

    /// The Docker Desktop settings with `MemoryMiB` changed, everything else kept.
    ///
    /// # Arguments
    /// * `json` - content of `settings-store.json`
    /// * `mebibytes` - new memory limit
    ///
    /// # Returns
    /// The new content, or why it cannot be changed (not JSON, not an object).
    pub fn docker_settings_with_memory(json: &str, mebibytes: u64) -> Result<String, String> {
        let mut value: Value = serde_json::from_str(json).map_err(|e| e.to_string())?;
        let object = value
            .as_object_mut()
            .ok_or_else(|| "not a JSON object".to_string())?;
        object.insert("MemoryMiB".into(), Value::from(mebibytes));
        serde_json::to_string_pretty(&value).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;
    use crate::container_runtime::RuntimeKind;

    const GIBIBYTE: u64 = 1024 * 1024 * 1024;

    /// Records the commands and fails the ones that contain `fail`.
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
                    Some(pattern) if line.contains(pattern) => Err(Error::Command {
                        command: line,
                        message: "boom".into(),
                    }),
                    _ => Ok(String::new()),
                }
            })
        }
    }

    fn temporary_directory(name: &str) -> PathBuf {
        let path =
            std::env::temp_dir().join(format!("macworp_adjust_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        path
    }

    /// 32 GiB host: the system keeps a quarter, the slider starts at 16 GiB.
    #[test]
    fn range_for_a_large_host() {
        let range = MemoryRange::for_host(32 * GIBIBYTE);
        assert_eq!(range.minimum_mebibytes, 2048);
        assert_eq!(range.maximum_mebibytes, 24 * 1024);
        assert_eq!(range.start_mebibytes, 16 * 1024);
        assert_eq!(range.step_mebibytes, 512);
        assert_eq!(range.warning_mebibytes, 24 * 1024);
    }

    /// 8 GiB host: the system keeps 4 GiB (more than a quarter), the start value stays in range.
    #[test]
    fn range_for_a_small_host() {
        let range = MemoryRange::for_host(8 * GIBIBYTE);
        assert_eq!(range.maximum_mebibytes, 4 * 1024);
        assert_eq!(range.start_mebibytes, 4 * 1024);
        assert!(range.minimum_mebibytes <= range.start_mebibytes);
        // Hosts below the minimum still get a usable range
        let tiny = MemoryRange::for_host(2 * GIBIBYTE);
        assert!(tiny.minimum_mebibytes <= tiny.start_mebibytes);
        assert!(tiny.start_mebibytes <= tiny.maximum_mebibytes);
    }

    #[test]
    fn range_for_a_medium_host_starts_at_half() {
        let range = MemoryRange::for_host(16 * GIBIBYTE);
        assert_eq!(range.maximum_mebibytes, 12 * 1024);
        assert_eq!(range.start_mebibytes, 8 * 1024);
    }

    #[test]
    fn parses_machines_running_first() {
        let machines = Machine::parse_list(
            r#"[{"Name":"b","Default":true,"Running":false,"VMType":"applehv"},
                {"Name":"a*","Default":false,"Running":true,"VMType":"wsl"}]"#,
        );
        assert_eq!(machines[0].name, "a");
        assert_eq!(machines[0].provider, "wsl");
        assert_eq!(machines.len(), 2);
        assert!(Machine::parse_list("garbage").is_empty());
    }

    #[test]
    fn plans_per_runtime_and_host() {
        let machine = |provider: &str| {
            vec![Machine {
                name: "m".into(),
                provider: provider.into(),
                running: true,
            }]
        };
        let home = temporary_directory("plan");
        let plan = |kind, system, machines: &[Machine]| {
            MemoryAdjustment::plan(kind, system, machines, &home)
        };
        assert_eq!(
            plan(RuntimeKind::Podman, HostSystem::MacOs, &machine("applehv")),
            Some(MemoryAdjustment::PodmanMachine { name: "m".into() })
        );
        assert!(matches!(
            plan(RuntimeKind::Podman, HostSystem::Windows, &machine("wsl")),
            Some(MemoryAdjustment::Wsl2 {
                podman_machine: Some(_),
                ..
            })
        ));
        assert_eq!(
            plan(RuntimeKind::Podman, HostSystem::Windows, &machine("hyperv")),
            Some(MemoryAdjustment::PodmanMachine { name: "m".into() })
        );
        assert!(matches!(
            plan(RuntimeKind::Docker, HostSystem::Windows, &[]),
            Some(MemoryAdjustment::Wsl2 {
                docker_desktop: true,
                ..
            })
        ));
        // Nothing to adjust on Linux, without machine, for wslc; Docker Desktop needs its file
        assert!(plan(RuntimeKind::Podman, HostSystem::Linux, &machine("wsl")).is_none());
        assert!(plan(RuntimeKind::Docker, HostSystem::Linux, &[]).is_none());
        assert!(plan(RuntimeKind::Podman, HostSystem::MacOs, &[]).is_none());
        assert!(plan(RuntimeKind::Wslc, HostSystem::Windows, &[]).is_none());
        assert!(plan(RuntimeKind::Docker, HostSystem::MacOs, &[]).is_none());
    }

    #[test]
    fn wsl_configuration_keeps_everything_else() {
        let changed = MemoryAdjustment::wsl_configuration_with_memory(
            "[wsl2]\r\nprocessors=4\r\nmemory=2GB\r\n\r\n[experimental]\r\nautoMemoryReclaim=gradual\r\n",
            8192,
        );
        assert_eq!(
            changed,
            "[wsl2]\r\nprocessors=4\r\nmemory=8192MB\r\n\r\n[experimental]\r\nautoMemoryReclaim=gradual\r\n"
        );
        // Key missing in the section, section missing, file empty
        assert_eq!(
            MemoryAdjustment::wsl_configuration_with_memory("[wsl2]\nswap=0\n", 4096),
            "[wsl2]\nmemory=4096MB\nswap=0\n"
        );
        assert_eq!(
            MemoryAdjustment::wsl_configuration_with_memory("[experimental]\nsparseVhd=true", 4096),
            "[experimental]\nsparseVhd=true\n\n[wsl2]\nmemory=4096MB\n"
        );
        assert_eq!(
            MemoryAdjustment::wsl_configuration_with_memory("", 4096),
            "[wsl2]\r\nmemory=4096MB\r\n"
        );
        // A `memory` key of another section is not touched
        let other = MemoryAdjustment::wsl_configuration_with_memory(
            "[other]\nmemory=1\n[wsl2]\nswap=0\n",
            4096,
        );
        assert!(other.contains("[other]\nmemory=1\n"));
    }

    #[test]
    fn docker_settings_keep_other_keys() {
        let changed =
            MemoryAdjustment::docker_settings_with_memory(r#"{"Cpus":12,"MemoryMiB":2048}"#, 8192)
                .unwrap();
        let value: Value = serde_json::from_str(&changed).unwrap();
        assert_eq!(value["MemoryMiB"], 8192);
        assert_eq!(value["Cpus"], 12);
        assert!(MemoryAdjustment::docker_settings_with_memory("[]", 1).is_err());
        assert!(MemoryAdjustment::docker_settings_with_memory("nope", 1).is_err());
    }

    #[tokio::test]
    async fn podman_machine_is_stopped_changed_and_started() {
        let recorder = Recorder::default();
        MemoryAdjustment::PodmanMachine { name: "m".into() }
            .apply_with(&recorder, 8192)
            .await
            .unwrap();
        assert_eq!(
            *recorder.calls.lock().unwrap(),
            [
                "podman machine stop m",
                "podman machine set --memory 8192 m",
                "podman machine start m"
            ]
        );
    }

    /// The machine runs again even when the change failed, and the error is the change's.
    #[tokio::test]
    async fn podman_machine_restarts_after_a_failed_change() {
        let recorder = Recorder {
            fail: Some("set --memory"),
            ..Default::default()
        };
        let error = MemoryAdjustment::PodmanMachine { name: "m".into() }
            .apply_with(&recorder, 8192)
            .await
            .unwrap_err();
        assert!(error.to_string().contains("set --memory"));
        assert_eq!(
            recorder.calls.lock().unwrap().last().unwrap(),
            "podman machine start m"
        );
    }

    #[tokio::test]
    async fn wsl_configuration_is_backed_up_and_restored_on_failure() {
        let directory = temporary_directory("wsl");
        let configuration = directory.join(".wslconfig");
        std::fs::write(&configuration, "[wsl2]\nprocessors=2\n").unwrap();
        let adjustment = MemoryAdjustment::Wsl2 {
            configuration: configuration.clone(),
            podman_machine: Some("m".into()),
            docker_desktop: true,
        };

        let recorder = Recorder::default();
        adjustment.apply_with(&recorder, 8192).await.unwrap();
        let text = std::fs::read_to_string(&configuration).unwrap();
        assert!(text.contains("memory=8192MB") && text.contains("processors=2"));
        assert_eq!(
            std::fs::read_to_string(directory.join(WSL_CONFIG_BACKUP)).unwrap(),
            "[wsl2]\nprocessors=2\n"
        );
        assert_eq!(
            *recorder.calls.lock().unwrap(),
            [
                "podman machine stop m",
                "wsl --shutdown",
                "podman machine start m",
                "docker desktop restart"
            ]
        );

        // A failed shutdown puts the previous file back
        std::fs::write(&configuration, "[wsl2]\nprocessors=2\n").unwrap();
        let failing = Recorder {
            fail: Some("--shutdown"),
            ..Default::default()
        };
        assert!(adjustment.apply_with(&failing, 8192).await.is_err());
        assert_eq!(
            std::fs::read_to_string(&configuration).unwrap(),
            "[wsl2]\nprocessors=2\n"
        );
    }

    #[tokio::test]
    async fn missing_wsl_configuration_is_created_and_removed_on_failure() {
        let directory = temporary_directory("wslnew");
        let configuration = directory.join(".wslconfig");
        let adjustment = MemoryAdjustment::Wsl2 {
            configuration: configuration.clone(),
            podman_machine: None,
            docker_desktop: false,
        };
        adjustment
            .apply_with(&Recorder::default(), 4096)
            .await
            .unwrap();
        assert!(
            std::fs::read_to_string(&configuration)
                .unwrap()
                .contains("memory=4096MB")
        );
        assert!(!directory.join(WSL_CONFIG_BACKUP).exists());

        std::fs::remove_file(&configuration).unwrap();
        let failing = Recorder {
            fail: Some("--shutdown"),
            ..Default::default()
        };
        assert!(adjustment.apply_with(&failing, 4096).await.is_err());
        assert!(!configuration.exists());
    }

    #[tokio::test]
    async fn docker_desktop_settings_are_restored_when_the_restart_fails() {
        let directory = temporary_directory("docker");
        let settings = directory.join("settings-store.json");
        std::fs::write(&settings, r#"{"MemoryMiB":2048}"#).unwrap();
        let adjustment = MemoryAdjustment::DockerDesktop {
            settings: settings.clone(),
        };
        adjustment
            .apply_with(&Recorder::default(), 8192)
            .await
            .unwrap();
        assert!(std::fs::read_to_string(&settings).unwrap().contains("8192"));

        std::fs::write(&settings, r#"{"MemoryMiB":2048}"#).unwrap();
        let failing = Recorder {
            fail: Some("desktop restart"),
            ..Default::default()
        };
        assert!(adjustment.apply_with(&failing, 8192).await.is_err());
        assert_eq!(
            std::fs::read_to_string(&settings).unwrap(),
            r#"{"MemoryMiB":2048}"#
        );
    }

    /// Changes the memory of the real runtime of this computer and reads it back. Restarts the
    /// container engine, so it only runs on request:
    /// `MACWORP_CONTAINER_RUNTIME=podman MACWORP_ADJUST_MEBIBYTES=6144 cargo test -p macworp_worker real_runtime_memory_is_adjusted -- --ignored --nocapture`
    #[tokio::test]
    #[ignore = "restarts the container engine"]
    async fn real_runtime_memory_is_adjusted() {
        use crate::container_runtime::{RuntimePreference, doctor};
        let mebibytes: u64 = std::env::var("MACWORP_ADJUST_MEBIBYTES")
            .expect("set MACWORP_ADJUST_MEBIBYTES")
            .parse()
            .unwrap();
        let preference = match std::env::var("MACWORP_CONTAINER_RUNTIME").as_deref() {
            Ok("docker") => RuntimePreference::Docker,
            _ => RuntimePreference::Podman,
        };
        let report = doctor(preference).await;
        let kind = report.runtimes.first().expect("a usable runtime").info.kind;
        let adjustment = MemoryAdjustment::detect(kind)
            .await
            .expect("nothing to adjust for this runtime");
        println!("plan: {adjustment:?}");
        adjustment.apply(mebibytes).await.unwrap();
        let after = doctor(preference).await;
        let bytes = after.runtimes.first().unwrap().info.memory_bytes.unwrap();
        println!("memory now: {} MiB", bytes / 1024 / 1024);
        // The VM reports a little less than it was given
        assert!(bytes / 1024 / 1024 >= mebibytes * 9 / 10);
    }

    /// A helper that outlives the command and keeps its output open (what `podman machine start`
    /// leaves behind) must not keep the command from returning.
    #[cfg(unix)]
    #[tokio::test]
    async fn command_returns_while_a_helper_keeps_running() {
        let started = std::time::Instant::now();
        let output = SystemCommands
            .run("sh", &["-c".into(), "sleep 20 & echo started".into()])
            .await
            .unwrap();
        assert_eq!(output.trim(), "started");
        assert!(started.elapsed() < Duration::from_secs(10));
        let failure = SystemCommands
            .run("sh", &["-c".into(), "echo oops >&2; exit 3".into()])
            .await
            .unwrap_err();
        assert!(failure.to_string().contains("oops"), "{failure}");
    }
}
