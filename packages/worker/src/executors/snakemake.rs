use std::path::PathBuf;

use macworp_core::workflow_configuration::{engine::Engine, render::render_args, source::Source};
use tokio::sync::{mpsc, watch};

use super::containerized_execution::path_arg;
use super::{
    Executor, ExecutorError, Job, LaunchSpec, LogLine, Outcome, containerized_spec, run_spec,
};
use crate::container_runtime::BoxFuture;

/// Engine image used unless configured otherwise, see the engine-in-container ADR.
pub const DEFAULT_IMAGE: &str = "macworp-snakemake:latest";

/// Engine parameters that already say how many cores to use
const CORE_PARAMETERS: &[&str] = &["cores", "c", "jobs", "j"];

/// Engine parameters that already choose how software is deployed, so no default is added
const DEPLOYMENT_PARAMETERS: &[&str] = &[
    "software-deployment-method",
    "sdm",
    "use-apptainer",
    "use-singularity",
    "use-conda",
    "use-envmodules",
];

/// `run` options Apptainer needs inside the engine container, see `docs/container-runtimes.md`:
/// it creates user namespaces and mounts `/proc` in them, which the default seccomp profile and
/// the masked `/proc` paths of the runtime forbid. No extra capability, no device and no socket.
const APPTAINER_RUN_ARGS: [&str; 4] = [
    "--security-opt",
    "seccomp=unconfined",
    "--security-opt",
    "systempaths=unconfined",
];

/// Runs Snakemake inside an engine image through the container runtime.
#[derive(Debug, Clone)]
pub struct SnakemakeExecutor {
    runtime_binary: PathBuf,
    image: String,
}

impl SnakemakeExecutor {
    pub fn new(runtime_binary: impl Into<PathBuf>, image: impl Into<String>) -> Self {
        Self {
            runtime_binary: runtime_binary.into(),
            image: image.into(),
        }
    }

    /// The `<runtime> run ...` command for the job, see [`containerized_spec`].
    ///
    /// Snakemake works in the project directory (`--directory`). It insists on a core count, so
    /// `--cores all` is added unless an engine parameter sets one.
    pub fn launch_spec(&self, job: &Job) -> Result<LaunchSpec, ExecutorError> {
        let config = &job.configuration;

        let mut read_only: Vec<&std::path::Path> = Vec::new();
        read_only.extend(job.reference_dir.as_deref());
        let mut command: Vec<String> = vec![
            "snakemake".into(),
            "--directory".into(),
            path_arg(&job.project_dir),
        ];
        match config.source() {
            Source::Local(local) => {
                read_only.push(local.location().as_path());
                command.extend([
                    "--snakefile".into(),
                    path_arg(&local.location().join(local.script())),
                ]);
            }
            // Snakemake cannot run a repository by URL, the worker would have to clone it first
            Source::Remote(_) => {
                return Err(ExecutorError::UnsupportedSource(
                    "remote (not cloned by the worker yet)".into(),
                ));
            }
            Source::NfCore(_) => return Err(ExecutorError::UnsupportedSource("nf-core".into())),
            Source::Image(_) => return Err(ExecutorError::UnsupportedSource("image".into())),
        }
        if !config
            .engine_parameters()
            .iter()
            .any(|p| CORE_PARAMETERS.contains(&p.name()))
        {
            command.extend(["--cores".into(), "all".into()]);
        }
        if !config
            .engine_parameters()
            .iter()
            .any(|p| DEPLOYMENT_PARAMETERS.contains(&p.name()))
        {
            command.extend(["--software-deployment-method".into(), "apptainer".into()]);
            // Apptainer binds the working directory, home and /tmp on its own, the rest of the
            // read-only inputs (workflow files, reference data) it only sees when asked
            if !read_only.is_empty() {
                let binds: Vec<String> = read_only
                    .iter()
                    .map(|dir| format!("-B {0}:{0}:ro", path_arg(dir)))
                    .collect();
                command.extend(["--apptainer-args".into(), binds.join(" ")]);
            }
        }
        if let Some(progress) = &job.progress {
            command.extend([
                "--logger".into(),
                "panoptes".into(),
                "--logger-panoptes-address".into(),
                format!("{}/snakemake", progress.base_url),
            ]);
        }
        command.extend(render_args(config, job.reference_dir.as_deref())?);

        Ok(containerized_spec(
            &self.runtime_binary,
            &self.image,
            job,
            &read_only,
            &self.run_args(job),
            command,
        ))
    }

    /// Extra `run` arguments of the engine container: what Apptainer needs
    /// ([`APPTAINER_RUN_ARGS`]), a writable home for its image cache and, where known, the user
    /// that owns the project directory. Apptainer cannot unpack images as root, so the engine
    /// runs as that user, which also keeps the files it writes owned by them.
    ///
    /// # Arguments
    /// * `job` - the job whose project directory decides the user
    fn run_args(&self, job: &Job) -> Vec<String> {
        let mut args: Vec<String> = APPTAINER_RUN_ARGS.map(String::from).into();
        args.extend(["-e".into(), "HOME=/tmp".into()]);
        if let Some(user) = owner_of(&job.project_dir) {
            args.extend(["--user".into(), user]);
        }
        args
    }
}

/// `<uid>:<gid>` of the owner of `path`, `None` where unknown (missing path, not a Unix host).
///
/// # Arguments
/// * `path` - file or directory to look at
fn owner_of(path: &std::path::Path) -> Option<String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let metadata = std::fs::metadata(path).ok()?;
        Some(format!("{}:{}", metadata.uid(), metadata.gid()))
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        None
    }
}

impl Executor for SnakemakeExecutor {
    fn engine(&self) -> Engine {
        Engine::Snakemake
    }

    fn run<'a>(
        &'a self,
        job: &'a Job,
        logs: mpsc::Sender<LogLine>,
        cancel: watch::Receiver<bool>,
    ) -> BoxFuture<'a, Outcome> {
        Box::pin(run_spec(self.launch_spec(job), logs, cancel))
    }
}

#[cfg(test)]
mod tests {
    use macworp_core::workflow_configuration::{
        WorkflowConfiguration,
        engine::EngineParameter,
        source::{LocalSource, RemoteSource},
    };

    use super::*;
    use crate::log_proxy::ProgressEndpoint;

    fn job(source: Source, engine_parameters: Vec<EngineParameter>) -> Job {
        let mut configuration =
            serde_json::from_value::<WorkflowConfiguration>(serde_json::json!({
                "engine": "Snakemake",
                "engine_parameters": [],
                "source": { "Local": { "location": "/w", "script": "Snakefile" } },
                "dynamic_parameters": [],
                "fixed_parameters": []
            }))
            .unwrap();
        *configuration.source_mut() = source;
        *configuration.engine_parameters_mut() = engine_parameters;
        Job {
            run_id: 3,
            project_dir: "/data/p1".into(),
            reference_dir: None,
            configuration,
            progress: None,
        }
    }

    fn local() -> Source {
        Source::Local(LocalSource::new("/w".into(), "Snakefile".into()))
    }

    fn executor() -> SnakemakeExecutor {
        SnakemakeExecutor::new("podman", "img:1")
    }

    #[test]
    fn progress_endpoint_adds_panoptes_logger() {
        let mut job = job(local(), vec![]);
        job.progress = Some(ProgressEndpoint {
            base_url: "http://host.docker.internal:4000/tok".into(),
            run_args: vec![
                "--add-host".into(),
                "host.docker.internal:host-gateway".into(),
            ],
        });
        let args = executor().launch_spec(&job).unwrap().args;
        let logger = args.iter().position(|a| a == "--logger").unwrap();
        assert_eq!(args[logger + 1], "panoptes");
        assert_eq!(args[logger + 2], "--logger-panoptes-address");
        assert_eq!(
            args[logger + 3],
            "http://host.docker.internal:4000/tok/snakemake"
        );
        assert!(args.contains(&"--add-host".to_string()));
    }

    #[test]
    fn local_source_defaults_to_all_cores() {
        let spec = executor().launch_spec(&job(local(), vec![])).unwrap();
        let name = spec.container_name.clone().unwrap();
        assert!(name.starts_with("macworp-run-3-"), "{name}");
        assert_eq!(
            spec.args,
            [
                "run",
                "--rm",
                "--name",
                name.as_str(),
                "-v",
                "/data/p1:/data/p1",
                "-w",
                "/data/p1",
                "-v",
                "/w:/w:ro",
                "--security-opt",
                "seccomp=unconfined",
                "--security-opt",
                "systempaths=unconfined",
                "-e",
                "HOME=/tmp",
                "img:1",
                "snakemake",
                "--directory",
                "/data/p1",
                "--snakefile",
                "/w/Snakefile",
                "--cores",
                "all",
                "--software-deployment-method",
                "apptainer",
                "--apptainer-args",
                "-B /w:/w:ro"
            ]
        );
    }

    /// A deployment method chosen by the workflow author is not overridden
    #[test]
    fn engine_parameter_sets_deployment_method() {
        let params = vec![EngineParameter::new("use-conda".into(), String::new())];
        let args = executor().launch_spec(&job(local(), params)).unwrap().args;
        assert!(!args.iter().any(|a| a == "--software-deployment-method"));
        assert!(!args.iter().any(|a| a == "--apptainer-args"));
    }

    /// The engine runs as the owner of the project directory, Apptainer cannot unpack as root
    #[cfg(unix)]
    #[test]
    fn engine_runs_as_project_owner() {
        let directory = tempfile::tempdir().unwrap();
        let mut job = job(local(), vec![]);
        job.project_dir = directory.path().to_path_buf();
        let args = executor().launch_spec(&job).unwrap().args;
        let user = args.iter().position(|a| a == "--user").unwrap();
        assert_eq!(Some(args[user + 1].clone()), owner_of(directory.path()));
    }

    #[test]
    fn engine_parameter_sets_cores() {
        let params = vec![EngineParameter::new("cores".into(), "4".into())];
        let spec = executor().launch_spec(&job(local(), params)).unwrap();
        let tail: Vec<_> = spec.args.iter().skip_while(|a| *a != "snakemake").collect();
        assert_eq!(tail.iter().filter(|a| a.as_str() == "--cores").count(), 1);
        assert!(spec.args.ends_with(&["--cores".into(), "4".into()]));
    }

    #[test]
    fn remote_source_is_rejected() {
        let source = Source::Remote(RemoteSource::new("https://x/y.git".into(), None, None));
        let result = executor().launch_spec(&job(source, vec![]));
        assert!(matches!(result, Err(ExecutorError::UnsupportedSource(_))));
    }
}
