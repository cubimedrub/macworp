//! Which engine parameters a workflow may set, and how their values must look.
//!
//! Engine parameters end up on the command line of the engine, which runs in a container next to
//! the runtime socket. Only the parameters listed here are accepted (an allowlist: Snakemake and
//! Toil use `argparse`, which also accepts abbreviations of option names, so a denylist could be
//! bypassed with `--dir` for `--directory`). Everything that locates files or configures how
//! MAcWorP itself talks to the engine (config files, work directories, progress reporting,
//! revisions, the parameter file, and everything that selects how software is deployed:
//! Snakemake's deployment method and conda/apptainer options, which only the worker sets) is left out on purpose; the worker adds those itself.
//!
//! Names are rendered as `-name` (Nextflow) or `--name` (Snakemake, Toil), so single letter
//! short options are not listed: `--c` would be an abbreviation, not `-c`.
//!
//! `-resume` is not offered either: the backend adds it when a failed run is retried
//! ([`Engine::resume_parameter`]), so it must not be set by hand.

use thiserror::Error;

use super::engine::Engine;

/// How the value of an allowed engine parameter must look.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ValueRule {
    /// A switch without a value
    Flag,
    /// A whole number, `min` or more
    Integer {
        /// Smallest accepted number
        min: i64,
    },
    /// A whole number or the word `all` (Snakemake `--cores`)
    IntegerOrAll,
    /// A memory or disk size such as `4G` or `512M`: digits with an optional unit
    Size,
    /// Words of letters, digits and `_ . , - + =` joined by commas, for example
    /// `docker,test` (Nextflow profiles) or `mem_mb=4000` (Snakemake resources)
    Words,
    /// One of the listed values
    OneOf(&'static [&'static str]),
}

impl ValueRule {
    /// Checks `value` against the rule, with a message saying what is expected.
    ///
    /// # Arguments
    /// * `value` - the value as entered, empty for a switch
    pub fn check(&self, value: &str) -> Result<(), String> {
        let word = |c: char| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-' | '+' | '=');
        match self {
            ValueRule::Flag if value.is_empty() => Ok(()),
            ValueRule::Flag => Err("takes no value".into()),
            ValueRule::Integer { min } => match value.parse::<i64>() {
                Ok(number) if number >= *min => Ok(()),
                _ => Err(format!("must be a whole number of at least {min}")),
            },
            ValueRule::IntegerOrAll => {
                if value == "all" || value.parse::<u32>().is_ok_and(|number| number >= 1) {
                    Ok(())
                } else {
                    Err("must be a positive whole number or `all`".into())
                }
            }
            ValueRule::Size => {
                let digits = value.trim_end_matches(|c: char| c.is_ascii_alphabetic());
                let unit = &value[digits.len()..];
                let known_unit = matches!(
                    unit.to_ascii_uppercase().as_str(),
                    "" | "K"
                        | "M"
                        | "G"
                        | "T"
                        | "KB"
                        | "MB"
                        | "GB"
                        | "TB"
                        | "KIB"
                        | "MIB"
                        | "GIB"
                        | "TIB"
                );
                if !digits.is_empty()
                    && digits.chars().all(|c| c.is_ascii_digit() || c == '.')
                    && known_unit
                {
                    Ok(())
                } else {
                    Err("must be a size like `4G` or `512M`".into())
                }
            }
            ValueRule::Words => {
                if !value.is_empty()
                    && value.split(',').all(|part| {
                        !part.is_empty() && !part.starts_with('-') && part.chars().all(word)
                    })
                {
                    Ok(())
                } else {
                    Err(
                        "must be words of letters, digits and `_ . - + =`, separated by commas, not starting with `-`"
                            .into(),
                    )
                }
            }
            ValueRule::OneOf(options) => {
                if options.contains(&value) {
                    Ok(())
                } else {
                    Err(format!("must be one of: {}", options.join(", ")))
                }
            }
        }
    }
}

/// One engine parameter a workflow may set.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ParameterSpec {
    /// Name without leading dashes, as stored in `EngineParameter::name`
    pub name: &'static str,
    /// What the value looks like
    pub rule: ValueRule,
    /// One line for the editor: what the parameter does
    pub description: &'static str,
    /// Whether users may switch it on or off when starting a run, not only the people who edit
    /// the workflow. Only for switches that change what is recorded, not how the workflow
    /// behaves (trace, report, timeline, task graph, printed commands). Everything that changes
    /// behaviour or results (profiles, error handling, retries, resources) stays with the
    /// workflow's editors.
    pub user_switchable: bool,
}

impl ParameterSpec {
    /// Whether the parameter is a switch (no value).
    pub fn is_flag(&self) -> bool {
        self.rule == ValueRule::Flag
    }
}

/// Short constructor so the tables below stay readable.
const fn spec(name: &'static str, rule: ValueRule, description: &'static str) -> ParameterSpec {
    ParameterSpec {
        name,
        rule,
        description,
        user_switchable: false,
    }
}

/// A switch that users may turn on or off when starting a run, see
/// [`ParameterSpec::user_switchable`].
const fn user_switch(name: &'static str, description: &'static str) -> ParameterSpec {
    ParameterSpec {
        name,
        rule: ValueRule::Flag,
        description,
        user_switchable: true,
    }
}

/// Nextflow parameters. `profile` selects named blocks of the workflow's own configuration, but
/// MAcWorP passes a generated configuration with `-c` (see the worker's Nextflow executor) that
/// takes precedence over profiles and per-process settings: the `local` executor and `docker` as container engine (the
/// engine image reaches Docker and Podman through the same Docker-compatible socket), all other
/// software deployments off. A profile therefore cannot move a run to another executor or
/// deployment method, whatever it sets.
const NEXTFLOW: &[ParameterSpec] = &[
    spec(
        "profile",
        ValueRule::Words,
        "Configuration profiles of the workflow, comma separated (`test,small`). MAcWorP overrides the executor and the software deployment of every profile: tasks run locally in containers of the available runtime, so profiles such as `conda`, `singularity` or `slurm` have no effect on that",
    ),
    spec(
        "ansi-log",
        ValueRule::OneOf(&["true", "false"]),
        "Colourful, updating console output (`false` gives plain log lines)",
    ),
    user_switch("with-trace", "Write a trace file of all tasks"),
    user_switch("with-report", "Write an HTML execution report"),
    user_switch("with-timeline", "Write an HTML timeline of the tasks"),
    user_switch("with-dag", "Write the task graph"),
    spec(
        "queue-size",
        ValueRule::Integer { min: 1 },
        "Most tasks that run at the same time",
    ),
    spec(
        "process.cpus",
        ValueRule::Integer { min: 1 },
        "CPUs of every task",
    ),
    spec("process.memory", ValueRule::Size, "Memory of every task"),
    spec(
        "process.errorStrategy",
        ValueRule::OneOf(&["terminate", "finish", "ignore", "retry"]),
        "What to do when a task fails",
    ),
];

const SNAKEMAKE: &[ParameterSpec] = &[
    spec("cores", ValueRule::IntegerOrAll, "CPU cores to use"),
    spec(
        "jobs",
        ValueRule::IntegerOrAll,
        "Most jobs that run at the same time",
    ),
    spec(
        "resources",
        ValueRule::Words,
        "Resource limits, `mem_mb=4000`",
    ),
    spec(
        "keep-going",
        ValueRule::Flag,
        "Continue with independent jobs after a failure",
    ),
    spec(
        "rerun-incomplete",
        ValueRule::Flag,
        "Rerun jobs whose output is incomplete",
    ),
    spec(
        "latency-wait",
        ValueRule::Integer { min: 0 },
        "Seconds to wait for output files to appear",
    ),
    spec(
        "retries",
        ValueRule::Integer { min: 0 },
        "How often a failed job is tried again",
    ),
    user_switch("printshellcmds", "Print the shell commands of the jobs"),
];

const CWL: &[ParameterSpec] = &[
    spec(
        "maxCores",
        ValueRule::Integer { min: 1 },
        "Most CPU cores used at the same time",
    ),
    spec(
        "maxMemory",
        ValueRule::Size,
        "Most memory used at the same time",
    ),
    spec(
        "maxDisk",
        ValueRule::Size,
        "Most disk space used at the same time",
    ),
    spec(
        "defaultCores",
        ValueRule::Integer { min: 1 },
        "Cores a job reserves when it names none",
    ),
    spec(
        "defaultMemory",
        ValueRule::Size,
        "Memory a job reserves when it names none",
    ),
    spec(
        "defaultDisk",
        ValueRule::Size,
        "Disk space a job reserves when it names none",
    ),
    spec(
        "retryCount",
        ValueRule::Integer { min: 0 },
        "How often a failed job is tried again",
    ),
];

/// Why an engine parameter is refused.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum Error {
    /// The engine takes no parameters (`Container`).
    #[error("Engine `{0}` does not support engine parameters")]
    Unsupported(String),
    /// The parameter is set by MAcWorP itself (resume, progress reporting, parameter file) or
    /// changes where files are read or written.
    #[error(
        "Engine parameter `{name}` is not allowed for {engine}: MAcWorP manages it or it could leave the project folder. Allowed: {allowed}"
    )]
    NotAllowed {
        /// Engine of the workflow
        engine: String,
        /// Name as entered
        name: String,
        /// Names that are allowed, comma separated
        allowed: String,
    },
    /// The value does not fit the parameter.
    #[error("Engine parameter `{name}` {reason}")]
    InvalidValue {
        /// Name of the parameter
        name: String,
        /// What is wrong with the value
        reason: String,
    },
    /// The same parameter twice.
    #[error("Engine parameter `{0}` is set twice")]
    Duplicate(String),
    /// A user tried to switch a parameter that only the workflow's editors may set.
    #[error("Engine parameter `{0}` can only be changed by those who edit the workflow")]
    NotUserSwitchable(String),
}

impl Engine {
    /// The engine parameters a workflow of this engine may set, empty for engines without any.
    /// The editor offers these as suggestions.
    pub fn parameter_specs(&self) -> &'static [ParameterSpec] {
        match self {
            Engine::Nextflow => NEXTFLOW,
            Engine::Snakemake => SNAKEMAKE,
            Engine::Cwl => CWL,
            Engine::Container => &[],
        }
    }

    /// The switches users may turn on or off when starting a run ([`ParameterSpec::user_switchable`]).
    pub fn user_switches(&self) -> impl Iterator<Item = &'static ParameterSpec> {
        self.parameter_specs()
            .iter()
            .filter(|spec| spec.user_switchable)
    }

    /// Checks one engine parameter against [`Self::parameter_specs`].
    ///
    /// # Arguments
    /// * `name` - the name without leading dashes, as stored
    /// * `value` - the value, empty for a switch
    pub fn check_parameter(&self, name: &str, value: &str) -> Result<(), Error> {
        let specs = self.parameter_specs();
        if specs.is_empty() {
            return Err(Error::Unsupported(self.to_string()));
        }
        let Some(spec) = specs.iter().find(|spec| spec.name == name) else {
            return Err(Error::NotAllowed {
                engine: self.to_string(),
                name: name.to_string(),
                allowed: specs
                    .iter()
                    .map(|spec| spec.name)
                    .collect::<Vec<_>>()
                    .join(", "),
            });
        };
        spec.rule
            .check(value)
            .map_err(|reason| Error::InvalidValue {
                name: name.to_string(),
                reason,
            })
    }
}

#[cfg(test)]
/// Tests of the engine parameter allowlist.
mod tests {
    use super::*;

    /// Every listed name is a valid flag name and every rule accepts a sample of its own kind.
    #[test]
    fn specs_are_well_formed() {
        for engine in Engine::all() {
            let mut seen = std::collections::HashSet::new();
            for spec in engine.parameter_specs() {
                assert!(seen.insert(spec.name), "{engine}: {} twice", spec.name);
                assert!(!spec.name.starts_with('-') && !spec.name.contains('='));
                assert!(!spec.description.is_empty());
            }
        }
    }

    /// Parameters MAcWorP manages or that locate files are refused, also abbreviated.
    #[test]
    fn managed_and_path_parameters_are_refused() {
        for name in [
            "resume",
            "c",
            "C",
            "w",
            "work-dir",
            "with-weblog",
            "params-file",
            "r",
            "revision",
            "e.FOO",
            "latest",
        ] {
            assert!(
                matches!(
                    Engine::Nextflow.check_parameter(name, "x"),
                    Err(Error::NotAllowed { .. })
                ),
                "nextflow {name}"
            );
        }
        for name in [
            "directory",
            "dir",
            "configfile",
            "config",
            "snakefile",
            "s",
            "c",
            "j",
            "wms-monitor",
            "logger",
            "apptainer-prefix",
            "apptainer-args",
            "singularity-args",
        ] {
            assert!(
                matches!(
                    Engine::Snakemake.check_parameter(name, "x"),
                    Err(Error::NotAllowed { .. })
                ),
                "snakemake {name}"
            );
        }
        for name in [
            "jobStore",
            "workDir",
            "restart",
            "cachedir",
            "outdir",
            "podman",
            "batchSystem",
        ] {
            assert!(
                matches!(
                    Engine::Cwl.check_parameter(name, "x"),
                    Err(Error::NotAllowed { .. })
                ),
                "cwl {name}"
            );
        }
    }

    /// Allowed parameters pass with a fitting value and fail with another.
    #[test]
    fn values_are_checked() {
        assert!(
            Engine::Nextflow
                .check_parameter("profile", "docker,test")
                .is_ok()
        );
        assert!(Engine::Nextflow.check_parameter("profile", "-c").is_err());
        assert!(Engine::Nextflow.check_parameter("profile", "a,,b").is_err());
        assert!(Engine::Nextflow.check_parameter("profile", "").is_err());
        assert!(Engine::Nextflow.check_parameter("with-trace", "").is_ok());
        // A path would let the report land anywhere
        assert!(
            Engine::Nextflow
                .check_parameter("with-report", "/etc/x")
                .is_err()
        );
        assert!(Engine::Nextflow.check_parameter("queue-size", "4").is_ok());
        assert!(Engine::Nextflow.check_parameter("queue-size", "0").is_err());
        assert!(Engine::Snakemake.check_parameter("cores", "all").is_ok());
        assert!(Engine::Snakemake.check_parameter("cores", "8").is_ok());
        assert!(Engine::Snakemake.check_parameter("cores", "0").is_err());
        assert!(
            Engine::Snakemake
                .check_parameter("resources", "mem_mb=4000")
                .is_ok()
        );
        assert!(Engine::Cwl.check_parameter("defaultMemory", "4G").is_ok());
        assert!(
            Engine::Cwl
                .check_parameter("defaultMemory", "4 G; rm")
                .is_err()
        );
        assert!(Engine::Cwl.check_parameter("maxCores", "-1").is_err());
    }

    /// `Container` takes no engine parameters at all.
    #[test]
    fn container_has_no_parameters() {
        assert_eq!(
            Engine::Container.check_parameter("x", ""),
            Err(Error::Unsupported("Container".into()))
        );
    }

    /// Users can only switch the observability switches; everything else is refused.
    #[test]
    fn users_switch_only_observability_parameters() {
        use crate::workflow_configuration::{WorkflowConfiguration, engine::EngineParameter};
        let mut configuration = WorkflowConfiguration::default();
        *configuration.engine_parameters_mut() = vec![
            EngineParameter::new("profile".into(), "test".into()),
            EngineParameter::new("with-trace".into(), String::new()),
        ];
        let switch = |name: &str, enabled: bool| vec![(name.to_string(), enabled)];
        let names = |configuration: &WorkflowConfiguration| {
            configuration
                .engine_parameters()
                .iter()
                .map(|parameter| parameter.name().to_string())
                .collect::<Vec<_>>()
        };

        let mut changed = configuration.clone();
        changed
            .apply_engine_switches(&[("with-dag".into(), true), ("with-trace".into(), false)])
            .unwrap();
        assert_eq!(names(&changed), ["profile", "with-dag"]);
        // Switching on what is on, and off what is off, changes nothing
        changed
            .apply_engine_switches(&[("with-dag".into(), true), ("with-report".into(), false)])
            .unwrap();
        assert_eq!(names(&changed), ["profile", "with-dag"]);

        for name in [
            "profile",
            "queue-size",
            "process.cpus",
            "resume",
            "with-weblog",
        ] {
            assert_eq!(
                configuration
                    .clone()
                    .apply_engine_switches(&switch(name, true)),
                Err(Error::NotUserSwitchable(name.into())),
                "{name}"
            );
        }
        let twice = [switch("with-dag", true), switch("with-dag", false)].concat();
        assert_eq!(
            configuration.clone().apply_engine_switches(&twice),
            Err(Error::Duplicate("with-dag".into()))
        );
        // Only the three engines' observability switches are open to users
        assert_eq!(
            Engine::Nextflow
                .user_switches()
                .map(|s| s.name)
                .collect::<Vec<_>>(),
            ["with-trace", "with-report", "with-timeline", "with-dag"]
        );
        assert_eq!(Engine::Cwl.user_switches().count(), 0);
        assert!(Engine::Snakemake.user_switches().all(|spec| spec.is_flag()));
    }
}
