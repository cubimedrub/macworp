use std::path::PathBuf;

use macworp_core::workflow_configuration::{engine::Engine, render::render_args, source::Source};
use tokio::sync::{mpsc, watch};

use super::{
    Executor, ExecutorError, Job, LaunchSpec, LogLine, Outcome, containerized_spec, run_spec,
};
use crate::container_runtime::BoxFuture;

/// Runs the single image of a `Container` workflow through the container runtime.
///
/// The image is the workflow itself, so unlike the other executors there is no engine image.
/// Workflow parameters are passed as arguments after the image (`--name value`, `-n value` for
/// single character names), engine parameters are rejected.
#[derive(Debug, Clone)]
pub struct ContainerExecutor {
    runtime_binary: PathBuf,
}

impl ContainerExecutor {
    pub fn new(runtime_binary: impl Into<PathBuf>) -> Self {
        Self {
            runtime_binary: runtime_binary.into(),
        }
    }

    /// The `<runtime> run ...` command for the job, see [`containerized_spec`].
    pub fn launch_spec(&self, job: &Job) -> Result<LaunchSpec, ExecutorError> {
        match job.configuration.source() {
            Source::Image(image) => {
                let read_only: Vec<&std::path::Path> =
                    job.reference_dir.as_deref().into_iter().collect();
                Ok(containerized_spec(
                    &self.runtime_binary,
                    image.image(),
                    job,
                    &read_only,
                    &[],
                    render_args(&job.configuration, job.reference_dir.as_deref())?,
                ))
            }
            other => Err(ExecutorError::UnsupportedSource(format!("{other:?}"))),
        }
    }
}

impl Executor for ContainerExecutor {
    fn engine(&self) -> Engine {
        Engine::Container
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
        source::{ImageSource, LocalSource},
    };

    use super::*;

    fn job(source: Source) -> Job {
        let mut configuration =
            serde_json::from_value::<WorkflowConfiguration>(serde_json::json!({
                "engine": "Container",
                "engine_parameters": [],
                "source": { "Image": { "image": "x" } },
                "dynamic_parameters": [],
                "fixed_parameters": []
            }))
            .unwrap();
        *configuration.source_mut() = source;
        Job {
            run_id: 5,
            project_dir: "/data/p1".into(),
            reference_dir: None,
            apptainer_cache: None,
            cwl_cache: None,
            configuration,
            progress: None,
        }
    }

    #[test]
    fn runs_the_image() {
        let spec = ContainerExecutor::new("docker")
            .launch_spec(&job(Source::Image(ImageSource::new("hello:1".into()))))
            .unwrap();
        let name = spec.container_name.clone().unwrap();
        assert!(name.starts_with("macworp-run-5-"), "{name}");
        assert_eq!(
            super::super::containerized_execution::without_worker_user(spec.args.clone()),
            [
                "run",
                "--rm",
                "--name",
                name.as_str(),
                "-v",
                "/data/p1:/data/p1",
                "-w",
                "/data/p1",
                "hello:1"
            ]
        );
    }

    #[test]
    fn other_sources_are_rejected() {
        let source = Source::Local(LocalSource::new("/w".into(), "main.nf".into()));
        let result = ContainerExecutor::new("docker").launch_spec(&job(source));
        assert!(matches!(result, Err(ExecutorError::UnsupportedSource(_))));
    }
}
