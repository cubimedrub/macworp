use std::path::PathBuf;

use macworp_core::workflow_configuration::{engine::Engine, render::render_args, source::Source};
use tokio::sync::{mpsc, watch};

use super::containerized_execution::path_arg;
use super::{
    Executor, ExecutorError, Job, LaunchSpec, LogLine, Outcome, containerized_spec, run_spec,
};
use crate::container_runtime::BoxFuture;

/// Engine image used unless configured otherwise, see the engine-in-container ADR.
pub const DEFAULT_IMAGE: &str = "macworp-nextflow:latest";

/// Runs Nextflow inside an engine image through the container runtime.
#[derive(Debug, Clone)]
pub struct NextflowExecutor {
    runtime_binary: PathBuf,
    image: String,
    engine_access: Vec<String>,
}

impl NextflowExecutor {
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
    pub fn launch_spec(&self, job: &Job) -> Result<LaunchSpec, ExecutorError> {
        let config = &job.configuration;

        let mut read_only = Vec::new();
        let mut command: Vec<String> = vec!["nextflow".into(), "run".into()];
        match config.source() {
            Source::Local(local) => {
                read_only.push(local.location().as_path());
                command.push(path_arg(&local.location().join(local.script())));
            }
            Source::Remote(remote) => {
                command.push(remote.url().to_string());
                if let Some(version) = remote.version() {
                    command.extend(["-r".into(), version.clone()]);
                }
            }
            Source::NfCore(nf_core) => {
                command.push(format!(
                    "nf-core/{}",
                    nf_core.pipeline().trim_start_matches("nf-core/")
                ));
                if let Some(version) = nf_core.version() {
                    command.extend(["-r".into(), version.clone()]);
                }
            }
            Source::Image(_) => {
                return Err(ExecutorError::UnsupportedSource("image".into()));
            }
        }
        if let Some(progress) = &job.progress {
            command.extend([
                "-with-weblog".into(),
                format!("{}/nextflow", progress.base_url),
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

impl Executor for NextflowExecutor {
    fn engine(&self) -> Engine {
        Engine::Nextflow
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
        source::{LocalSource, NfCoreSource, RemoteSource},
    };

    use super::*;
    use crate::log_proxy::ProgressEndpoint;

    fn job(source: Source) -> Job {
        let mut configuration =
            serde_json::from_value::<WorkflowConfiguration>(serde_json::json!({
                "engine": "Nextflow",
                "engine_parameters": [],
                "source": { "Local": { "location": "/w", "script": "main.nf" } },
                "dynamic_parameters": [],
                "fixed_parameters": []
            }))
            .unwrap();
        *configuration.source_mut() = source;
        Job {
            run_id: 7,
            project_dir: "/data/p1".into(),
            configuration,
            progress: None,
        }
    }

    fn executor() -> NextflowExecutor {
        NextflowExecutor::new("podman", "img:1")
    }

    #[test]
    fn progress_endpoint_adds_weblog_and_host_mapping() {
        let mut job = job(Source::Local(LocalSource::new(
            "/w".into(),
            "main.nf".into(),
        )));
        job.progress = Some(ProgressEndpoint {
            base_url: "http://host.docker.internal:4000/tok".into(),
            run_args: vec![
                "--add-host".into(),
                "host.docker.internal:host-gateway".into(),
            ],
        });
        let args = executor().launch_spec(&job).unwrap().args;
        // run option before the image, engine option after the script
        let image = args.iter().position(|a| a == "img:1").unwrap();
        let add_host = args.iter().position(|a| a == "--add-host").unwrap();
        assert!(add_host < image);
        assert_eq!(args[add_host + 1], "host.docker.internal:host-gateway");
        let weblog = args.iter().position(|a| a == "-with-weblog").unwrap();
        assert!(weblog > image);
        assert_eq!(
            args[weblog + 1],
            "http://host.docker.internal:4000/tok/nextflow"
        );
    }

    #[test]
    fn local_source() {
        let spec = executor()
            .launch_spec(&job(Source::Local(LocalSource::new(
                "/w".into(),
                "main.nf".into(),
            ))))
            .unwrap();
        assert_eq!(spec.program, PathBuf::from("podman"));
        assert_eq!(spec.container_name.as_deref(), Some("macworp-run-7"));
        assert_eq!(
            spec.args,
            [
                "run",
                "--rm",
                "--name",
                "macworp-run-7",
                "-v",
                "/data/p1:/data/p1",
                "-w",
                "/data/p1",
                "-v",
                "/w:/w:ro",
                "img:1",
                "nextflow",
                "run",
                "/w/main.nf"
            ]
        );
    }

    #[test]
    fn remote_source_with_version() {
        let spec = executor()
            .launch_spec(&job(Source::Remote(RemoteSource::new(
                "https://example.org/x.git".into(),
                Some("v1".into()),
                None,
            ))))
            .unwrap();
        assert!(spec.args.ends_with(&[
            "img:1".into(),
            "nextflow".into(),
            "run".into(),
            "https://example.org/x.git".into(),
            "-r".into(),
            "v1".into()
        ]));
    }

    #[test]
    fn nf_core_prefix_not_doubled() {
        for name in ["rnaseq", "nf-core/rnaseq"] {
            let spec = executor()
                .launch_spec(&job(Source::NfCore(NfCoreSource::new(name.into(), None))))
                .unwrap();
            assert_eq!(spec.args.last().unwrap(), "nf-core/rnaseq");
        }
    }

    #[test]
    fn image_source_is_rejected() {
        let result = executor().launch_spec(&job(Source::Image(Default::default())));
        assert!(matches!(result, Err(ExecutorError::UnsupportedSource(_))));
    }
}
