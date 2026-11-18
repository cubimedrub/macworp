use std::path::PathBuf;

use macworp_core::workflow_configuration::{engine::Engine, render::render_args, source::Source};
use tokio::sync::{mpsc, watch};

use super::containerized_execution::path_arg;
use super::{
    Executor, ExecutorError, Job, LaunchSpec, LogLine, Outcome, ParameterFile, containerized_spec, worker_user_args,
    run_spec,
};
use crate::container_runtime::BoxFuture;

/// Engine image used unless configured otherwise, see the engine-in-container ADR.
pub const DEFAULT_IMAGE: &str = "macworp-cwl:latest";

/// Environment variable that replaces [`DEFAULT_IMAGE`], for example to run another Toil version
/// (an image built from `docker/engine.dockerfile` with another version argument).
pub const IMAGE_VARIABLE: &str = "MACWORP_CWL_IMAGE";

/// Directory inside the project directory where Toil keeps its job store and working files.
/// It has to be below the project directory: Toil mounts files from its working directory into
/// the sibling tool containers, so the path must exist on the host as well.
pub const WORK_DIRECTORY: &str = ".toil";

/// Engine parameters that already say where the job store or working files go, so no default is
/// added
const LOCATION_PARAMETERS: &[&str] = &["jobStore", "workDir"];

/// The engine image to run: the value of [`IMAGE_VARIABLE`] if set and not empty, else
/// [`DEFAULT_IMAGE`].
pub fn image() -> String {
    std::env::var(IMAGE_VARIABLE)
        .ok()
        .filter(|image| !image.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_IMAGE.to_string())
}

/// Runs CWL workflows with Toil (`toil-cwl-runner`) inside an engine image through the container
/// runtime. `DockerRequirement` images are started as sibling containers through the runtime
/// socket, like Nextflow does.
#[derive(Debug, Clone)]
pub struct CwlExecutor {
    runtime_binary: PathBuf,
    image: String,
    engine_access: Vec<String>,
}

impl CwlExecutor {
    /// # Arguments
    /// * `runtime_binary` - the container runtime binary (`docker`, `podman`)
    /// * `image` - the engine image with Toil, see [`image`]
    pub fn new(runtime_binary: impl Into<PathBuf>, image: impl Into<String>) -> Self {
        Self {
            runtime_binary: runtime_binary.into(),
            image: image.into(),
            engine_access: Vec::new(),
        }
    }

    /// Extra `run` arguments so the engine can start sibling containers, see
    /// [`crate::container_runtime::RuntimeInfo::engine_access_args`].
    ///
    /// # Arguments
    /// * `args` - the `run` arguments
    pub fn with_engine_access(mut self, args: Vec<String>) -> Self {
        self.engine_access = args;
        self
    }

    /// The `<runtime> run ...` command for the job, see [`containerized_spec`].
    ///
    /// Toil writes the outputs into the project directory (`--outdir`), its job store and working
    /// files go to [`WORK_DIRECTORY`] below it unless an engine parameter says otherwise. The
    /// workflow parameters are the CWL input object, a JSON file in a scratch directory (mounted
    /// read-only, removed by [`run_spec`] after the run), see [`ParameterFile`]. Only local
    /// sources are supported.
    ///
    /// # Arguments
    /// * `job` - the job to run
    pub fn launch_spec(&self, job: &Job) -> Result<LaunchSpec, ExecutorError> {
        let configuration = &job.configuration;
        let mut read_only: Vec<&std::path::Path> = Vec::new();
        read_only.extend(job.reference_dir.as_deref());
        let Source::Local(local) = configuration.source() else {
            return Err(ExecutorError::UnsupportedSource(
                match configuration.source() {
                    Source::Remote(_) => "remote (not cloned by the worker yet)",
                    Source::NfCore(_) => "nf-core",
                    _ => "image",
                }
                .into(),
            ));
        };
        read_only.push(local.location().as_path());

        let mut command: Vec<String> = vec![
            "toil-cwl-runner".into(),
            "--outdir".into(),
            path_arg(&job.project_dir),
            "--disableProgress".into(),
        ];
        if !configuration
            .engine_parameters()
            .iter()
            .any(|p| LOCATION_PARAMETERS.contains(&p.name()))
        {
            let work = job.project_dir.join(WORK_DIRECTORY);
            command.extend([
                "--workDir".into(),
                path_arg(&work),
                "--jobStore".into(),
                path_arg(&work.join("jobstore")),
            ]);
        }
        command.extend(render_args(configuration, job.reference_dir.as_deref())?);
        command.push(path_arg(&local.location().join(local.script())));
        // Last step that can fail, so no scratch directory is left behind by an error
        let parameters = ParameterFile::write(job)?;
        if let Some(parameters) = &parameters {
            command.push(path_arg(parameters.file()));
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
}

impl Executor for CwlExecutor {
    fn engine(&self) -> Engine {
        Engine::Cwl
    }

    fn run<'a>(
        &'a self,
        job: &'a Job,
        logs: mpsc::Sender<LogLine>,
        cancel: watch::Receiver<bool>,
    ) -> BoxFuture<'a, Outcome> {
        // Toil does not create its working directory itself
        let spec = self.launch_spec(job).and_then(|spec| {
            std::fs::create_dir_all(job.project_dir.join(WORK_DIRECTORY))
                .map_err(|error| ExecutorError::Io("the Toil working directory", error))?;
            Ok(spec)
        });
        Box::pin(async move {
            let outcome = run_spec(spec, logs, cancel).await;
            if matches!(outcome, Outcome::Succeeded) && job.configuration.cleanup_on_success() {
                Self::cleanup(job);
            }
            outcome
        })
    }
}

impl CwlExecutor {
    /// Removes [`WORK_DIRECTORY`] from the project directory after a successful run. Toil
    /// deletes its job store itself on success, what is left is the empty directory. Does
    /// nothing when an engine parameter moved the job store or working files elsewhere
    /// ([`LOCATION_PARAMETERS`]), because then the directory is not ours. A failure is only
    /// logged, the run stays successful.
    ///
    /// # Arguments
    /// * `job` - the finished job
    fn cleanup(job: &Job) {
        if job
            .configuration
            .engine_parameters()
            .iter()
            .any(|parameter| LOCATION_PARAMETERS.contains(&parameter.name()))
        {
            return;
        }
        let directory = job.project_dir.join(WORK_DIRECTORY);
        if let Err(error) = std::fs::remove_dir_all(&directory)
            && error.kind() != std::io::ErrorKind::NotFound
        {
            tracing::warn!("Cannot remove {}: {error}", directory.display());
        }
    }
}

#[cfg(test)]
mod tests {
    use macworp_core::workflow_configuration::{
        WorkflowConfiguration,
        engine::EngineParameter,
        parameters::{Parameter, parameter::IsParameterWithValue, text::Text},
        source::{LocalSource, RemoteSource},
    };

    use super::*;

    /// A CWL job in project `/data/p1` with the given source.
    ///
    /// # Arguments
    /// * `source` - the workflow source
    fn job(source: Source) -> Job {
        let mut configuration =
            serde_json::from_value::<WorkflowConfiguration>(serde_json::json!({
                "engine": "Cwl",
                "engine_parameters": [],
                "source": { "Local": { "location": "/w", "script": "tool.cwl" } },
                "dynamic_parameters": [],
                "fixed_parameters": []
            }))
            .unwrap();
        *configuration.source_mut() = source;
        Job {
            run_id: 5,
            project_dir: "/data/p1".into(),
            reference_dir: None,
            configuration,
            progress: None,
        }
    }

    /// The local source `/w/tool.cwl`.
    fn local() -> Source {
        Source::Local(LocalSource::new("/w".into(), "tool.cwl".into()))
    }

    /// Executor on podman with image `img:1`.
    fn executor() -> CwlExecutor {
        CwlExecutor::new("podman", "img:1")
    }

    /// The engine runs as the worker's user, so what it writes belongs to the storage user
    #[cfg(unix)]
    #[test]
    fn engine_runs_as_worker_user() {
        let args = executor().launch_spec(&job(local())).unwrap().args;
        let user = args.iter().position(|a| a == "--user").unwrap();
        assert_eq!(Some(args[user + 1].clone()), crate::executors::worker_user());
        assert!(args.windows(2).any(|pair| pair == ["-e", "HOME=/tmp"]));
    }

    /// Outputs go to the project, work files below it, the tool comes last.
    #[test]
    fn local_source() {
        let spec = executor().launch_spec(&job(local())).unwrap();
        let name = spec.container_name.clone().unwrap();
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
                "toil-cwl-runner",
                "--outdir",
                "/data/p1",
                "--disableProgress",
                "--workDir",
                "/data/p1/.toil",
                "--jobStore",
                "/data/p1/.toil/jobstore",
                "/w/tool.cwl"
            ]
        );
    }

    /// Engine parameters go before the tool, and a given job store replaces the default.
    #[test]
    fn engine_parameters_replace_locations() {
        let mut job = job(local());
        *job.configuration.engine_parameters_mut() = vec![
            EngineParameter::new("jobStore".into(), "/data/js".into()),
            EngineParameter::new("maxCores".into(), "2".into()),
        ];
        let args = executor().launch_spec(&job).unwrap().args;
        assert!(!args.contains(&"/data/p1/.toil/jobstore".to_string()));
        let tail = &args[args.len() - 5..];
        assert_eq!(
            tail,
            ["--jobStore", "/data/js", "--maxCores", "2", "/w/tool.cwl"]
        );
    }

    /// Values go into a read-only mounted JSON input object after the tool.
    #[test]
    fn parameters_are_the_input_object() {
        let mut job = job(local());
        let mut text = Text::new("message".into(), "".into(), "".into());
        *text.value_mut() = Some("hello".into());
        job.configuration
            .dynamic_parameters_mut()
            .push(Parameter::Text(text));
        let spec = executor().launch_spec(&job).unwrap();
        let directory = spec.scratch_directory.clone().unwrap();
        let file = spec.args.last().unwrap();
        assert_eq!(PathBuf::from(file), directory.join("parameters.json"));
        assert_eq!(spec.args[spec.args.len() - 2], "/w/tool.cwl");
        assert!(
            spec.args
                .contains(&format!("{0}:{0}:ro", directory.display()))
        );
        let written: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(file).unwrap()).unwrap();
        assert_eq!(written, serde_json::json!({"message": "hello"}));
        std::fs::remove_dir_all(directory).unwrap();
    }

    /// Sources other than local are rejected.
    #[test]
    fn other_sources_are_rejected() {
        for source in [
            Source::Remote(RemoteSource::new(
                "https://example.org/x.git".into(),
                None,
                None,
            )),
            Source::Image(Default::default()),
        ] {
            let result = executor().launch_spec(&job(source));
            assert!(matches!(result, Err(ExecutorError::UnsupportedSource(_))));
        }
    }

    /// A job whose project directory is `directory`, with the engine parameters given.
    ///
    /// # Arguments
    /// * `directory` - the project directory
    /// * `parameters` - names of engine parameters to set (with an empty value)
    fn cleanup_job(directory: &std::path::Path, parameters: &[&str]) -> Job {
        let mut job = job(local());
        job.project_dir = directory.to_path_buf();
        *job.configuration.engine_parameters_mut() = parameters
            .iter()
            .map(|name| EngineParameter::new((*name).into(), String::new()))
            .collect();
        job
    }

    /// The leftover Toil directory is removed, a missing one is not an error.
    #[test]
    fn cleanup_removes_the_work_directory() {
        let project = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(project.path().join(WORK_DIRECTORY).join("jobstore")).unwrap();
        std::fs::write(project.path().join("result.txt"), "kept").unwrap();
        let job = cleanup_job(project.path(), &[]);
        CwlExecutor::cleanup(&job);
        assert!(!project.path().join(WORK_DIRECTORY).exists());
        assert!(project.path().join("result.txt").exists());
        CwlExecutor::cleanup(&job);
    }

    /// With a job store or working directory chosen by the workflow, `.toil` is not touched.
    #[test]
    fn cleanup_skips_workflows_with_their_own_locations() {
        for parameter in LOCATION_PARAMETERS {
            let project = tempfile::tempdir().unwrap();
            std::fs::create_dir_all(project.path().join(WORK_DIRECTORY)).unwrap();
            CwlExecutor::cleanup(&cleanup_job(project.path(), &[parameter]));
            assert!(project.path().join(WORK_DIRECTORY).exists());
        }
    }
}
