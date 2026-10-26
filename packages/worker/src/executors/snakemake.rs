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

/// Runs Snakemake inside an engine image through the container runtime.
#[derive(Debug, Clone)]
pub struct SnakemakeExecutor {
    runtime_binary: PathBuf,
    image: String,
    engine_access: Vec<String>,
}

impl SnakemakeExecutor {
    pub fn new(runtime_binary: impl Into<PathBuf>, image: impl Into<String>) -> Self {
        Self {
            runtime_binary: runtime_binary.into(),
            image: image.into(),
            engine_access: Vec::new(),
        }
    }

    /// Extra `run` arguments so the engine can start sibling containers, see
    /// [`crate::container_runtime::RuntimeInfo::engine_access_args`].
    pub fn with_engine_access(mut self, args: Vec<String>) -> Self {
        self.engine_access = args;
        self
    }

    /// The `<runtime> run ...` command for the job, see [`containerized_spec`].
    ///
    /// Snakemake works in the project directory (`--directory`). It insists on a core count, so
    /// `--cores all` is added unless an engine parameter sets one.
    pub fn launch_spec(&self, job: &Job) -> Result<LaunchSpec, ExecutorError> {
        let config = &job.configuration;

        let mut read_only = Vec::new();
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
        if let Some(progress) = &job.progress {
            command.extend([
                "--wms-monitor".into(),
                format!("{}/snakemake", progress.base_url),
                "--wms-monitor-arg".into(),
                format!("project_id={}", job.run_id),
            ]);
        }
        command.extend(render_args(config)?);

        Ok(containerized_spec(
            &self.runtime_binary,
            &self.image,
            job,
            &read_only,
            &self.engine_access,
            command,
        ))
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
    fn progress_endpoint_adds_wms_monitor() {
        let mut job = job(local(), vec![]);
        job.progress = Some(ProgressEndpoint {
            base_url: "http://host.docker.internal:4000/tok".into(),
            run_args: vec![
                "--add-host".into(),
                "host.docker.internal:host-gateway".into(),
            ],
        });
        let args = executor().launch_spec(&job).unwrap().args;
        let monitor = args.iter().position(|a| a == "--wms-monitor").unwrap();
        assert_eq!(
            args[monitor + 1],
            "http://host.docker.internal:4000/tok/snakemake"
        );
        assert_eq!(args[monitor + 2], "--wms-monitor-arg");
        assert_eq!(args[monitor + 3], "project_id=3");
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
                "img:1",
                "snakemake",
                "--directory",
                "/data/p1",
                "--snakefile",
                "/w/Snakefile",
                "--cores",
                "all"
            ]
        );
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
