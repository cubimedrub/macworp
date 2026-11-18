use std::path::PathBuf;

use macworp_core::workflow_configuration::{engine::Engine, render::render_args, source::Source};
use tokio::sync::{mpsc, watch};

use super::containerized_execution::path_arg;
use super::{
    Executor, ExecutorError, Job, LaunchSpec, LogLine, Outcome, ParameterFile, containerized_spec, worker_user_args,
    run_process, run_spec,
};
use crate::container_runtime::BoxFuture;

/// Engine image used unless configured otherwise, see the engine-in-container ADR.
pub const DEFAULT_IMAGE: &str = "macworp-nextflow:latest";

/// Environment variable that replaces [`DEFAULT_IMAGE`], for example to run an older Nextflow
/// version (an image built from `docker/engine.dockerfile` with another version argument).
pub const IMAGE_VARIABLE: &str = "MACWORP_NEXTFLOW_IMAGE";

/// The engine image to run: the value of [`IMAGE_VARIABLE`] if set and not empty, else
/// [`DEFAULT_IMAGE`].
pub fn image() -> String {
    std::env::var(IMAGE_VARIABLE)
        .ok()
        .filter(|image| !image.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_IMAGE.to_string())
}

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

    /// The `<runtime> run ...` command for the job, see [`containerized_spec`]. The workflow
    /// parameters go into a file (`-params-file`, mounted read-only, removed by [`run_spec`]
    /// after the run), see [`ParameterFile`].
    pub fn launch_spec(&self, job: &Job) -> Result<LaunchSpec, ExecutorError> {
        let config = &job.configuration;

        let mut read_only: Vec<&std::path::Path> = Vec::new();
        read_only.extend(job.reference_dir.as_deref());
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
        command.extend(render_args(config, job.reference_dir.as_deref())?);
        // Last step that can fail, so no scratch directory is left behind by an error
        let parameters = ParameterFile::write(job)?;
        if let Some(parameters) = &parameters {
            command.extend(["-params-file".into(), path_arg(parameters.file())]);
            read_only.push(parameters.directory());
        }

        let mut spec = containerized_spec(
            &self.runtime_binary,
            &self.image,
            job,
            &read_only,
            &self.run_args(),
            command,
        );
        spec.scratch_directory = parameters.map(|file| file.directory().to_path_buf());
        Ok(spec)
    }

    /// Extra `run` arguments of the engine container: the worker's user
        /// ([`worker_user_args`]) and the runtime socket access.
        fn run_args(&self) -> Vec<String> {
            let mut args = worker_user_args();
            args.extend(self.engine_access.iter().cloned());
            args
        }

    /// The `<runtime> run ... nextflow clean -f` command that deletes the work directories of
    /// the last run. It runs in the engine image like the run itself (same user, same project
    /// mount), because task files may not belong to the worker's user on the host.
    ///
    /// # Arguments
    /// * `job` - the job whose project directory is cleaned
    pub fn cleanup_spec(&self, job: &Job) -> LaunchSpec {
        let read_only: Vec<&std::path::Path> = Vec::new();
        containerized_spec(
            &self.runtime_binary,
            &self.image,
            job,
            &read_only,
            &self.run_args(),
            vec!["nextflow".into(), "clean".into(), "-f".into()],
        )
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
        Box::pin(async move {
            let outcome = run_spec(self.launch_spec(job), logs.clone(), cancel.clone()).await;
            // Only a successful run is cleaned: a failed or cancelled one keeps its work
            // directories so it can be resumed
            if matches!(outcome, Outcome::Succeeded) && job.configuration.cleanup_on_success() {
                match run_process(&self.cleanup_spec(job), logs, cancel).await {
                    Outcome::Failed { error, .. } => {
                        tracing::warn!("Nextflow cleanup failed: {error:?}");
                    }
                    Outcome::Succeeded | Outcome::Cancelled => {}
                }
            }
            outcome
        })
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
            reference_dir: None,
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

    /// The reference directory is mounted read-only at its own path, nothing is mounted without it.
    #[test]
    fn reference_directory_is_mounted_read_only() {
        let source = Source::Local(LocalSource::new("/w".into(), "main.nf".into()));
        let mut with_reference = job(source.clone());
        with_reference.reference_dir = Some("/data/reference".into());
        let args = executor().launch_spec(&with_reference).unwrap().args;
        assert!(args.contains(&"/data/reference:/data/reference:ro".to_string()));
        let args = executor().launch_spec(&job(source)).unwrap().args;
        assert!(!args.iter().any(|a| a.contains("reference")));
    }

    /// The engine runs as the worker's user, so what it writes belongs to the storage user
    #[cfg(unix)]
    #[test]
    fn engine_runs_as_worker_user() {
        let args = executor().launch_spec(&job(Source::Local(LocalSource::new("/w".into(), "main.nf".into())))).unwrap().args;
        let user = args.iter().position(|a| a == "--user").unwrap();
        assert_eq!(Some(args[user + 1].clone()), crate::executors::worker_user());
        assert!(args.windows(2).any(|pair| pair == ["-e", "HOME=/tmp"]));
    }

    /// The cleanup runs `nextflow clean -f` in the engine image, in the project directory.
    #[test]
    fn cleanup_runs_nextflow_clean_in_the_engine_image() {
        let spec = executor().cleanup_spec(&job(Source::Local(LocalSource::new(
            "/w".into(),
            "main.nf".into(),
        ))));
        let args = spec.args;
        let image = args.iter().position(|a| a == "img:1").unwrap();
        assert_eq!(args[image + 1..], ["nextflow", "clean", "-f"]);
        assert!(args.contains(&"/data/p1:/data/p1".to_string()));
        assert!(!args.iter().any(|a| a.ends_with(":ro")));
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
        let name = spec.container_name.clone().unwrap();
        assert!(name.starts_with("macworp-run-7-"), "{name}");
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

    /// Applies one text parameter value to the job's configuration.
    ///
    /// # Arguments
    /// * `job` - job to change
    /// * `name` - name of the new dynamic parameter
    /// * `value` - its value
    fn add_text(job: &mut Job, name: &str, value: &str) {
        use macworp_core::workflow_configuration::parameters::{
            Parameter, parameter::IsParameterWithValue, text::Text,
        };
        let mut text = Text::new(name.into(), "".into(), "".into());
        *text.value_mut() = Some(value.into());
        job.configuration
            .dynamic_parameters_mut()
            .push(Parameter::Text(text));
    }

    /// Values go into a read-only mounted JSON file, exactly as typed, never onto the command line
    #[test]
    fn parameters_are_passed_through_a_file() {
        let mut job = job(Source::Local(LocalSource::new(
            "/w".into(),
            "main.nf".into(),
        )));
        add_text(&mut job, "a", "line one\nline two");
        add_text(&mut job, "b", "7");
        add_text(&mut job, "ref.species", "say \"hi\"");
        let spec = executor().launch_spec(&job).unwrap();
        let directory = spec.scratch_directory.clone().unwrap();
        let position = spec.args.iter().position(|a| a == "-params-file").unwrap();
        let file = PathBuf::from(&spec.args[position + 1]);
        assert_eq!(file, directory.join("parameters.json"));
        let image = spec.args.iter().position(|a| a == "img:1").unwrap();
        assert!(!spec.args[image..].iter().any(|a| a.starts_with("--")));
        assert!(
            spec.args
                .contains(&format!("{0}:{0}:ro", directory.display()))
        );
        let written: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
        assert_eq!(
            written,
            serde_json::json!({
                "a": "line one\nline two", "b": "7", "ref": {"species": "say \"hi\""}
            })
        );
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn image_source_is_rejected() {
        let result = executor().launch_spec(&job(Source::Image(Default::default())));
        assert!(matches!(result, Err(ExecutorError::UnsupportedSource(_))));
    }
}
