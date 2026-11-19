use std::path::{Path, PathBuf};

use macworp_core::workflow_configuration::{engine::Engine, render::render_args, source::Source};
use tokio::sync::{mpsc, watch};

use super::containerized_execution::path_arg;
use super::{
    Executor, ExecutorError, Job, LaunchSpec, LogLine, Outcome, ParameterFile, containerized_spec,
    run_spec, worker_user_args,
};
use crate::container_runtime::{BoxFuture, RuntimeKind};

/// Engine image used unless configured otherwise, see the engine-in-container ADR.
pub const DEFAULT_IMAGE: &str = "macworp-snakemake:latest";

/// Environment variable that replaces [`DEFAULT_IMAGE`], for example to run an older Snakemake
/// version (an image built from `docker/engine.dockerfile` with another version argument).
pub const IMAGE_VARIABLE: &str = "MACWORP_SNAKEMAKE_IMAGE";

/// The engine image to run: the value of [`IMAGE_VARIABLE`] if set and not empty, else
/// [`DEFAULT_IMAGE`].
pub fn image() -> String {
    std::env::var(IMAGE_VARIABLE)
        .ok()
        .filter(|image| !image.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_IMAGE.to_string())
}

/// Environment variable naming an AppArmor profile for the engine container (Docker on Linux),
/// for example `macworp-snakemake` from `docker/apparmor-macworp-snakemake`. Hosts that restrict
/// unprivileged user namespaces (Ubuntu 24.04+) need it, see `docs/container-runtimes.md`. Not
/// detected automatically: the runtime refuses to start a container with a profile that is not
/// loaded, and loaded profiles are not readable without root.
pub const APPARMOR_PROFILE_VARIABLE: &str = "MACWORP_SNAKEMAKE_APPARMOR_PROFILE";

/// The AppArmor profile to apply: the value of [`APPARMOR_PROFILE_VARIABLE`] if set and not empty.
pub fn apparmor_profile() -> Option<String> {
    std::env::var(APPARMOR_PROFILE_VARIABLE)
        .ok()
        .map(|profile| profile.trim().to_string())
        .filter(|profile| !profile.is_empty())
}

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

/// Engine parameters that already choose where Apptainer keeps pulled images, so no shared
/// cache is set
const CACHE_PARAMETERS: &[&str] = &["apptainer-prefix", "singularity-prefix"];

/// `run` options Apptainer needs inside the engine container on Docker, see
/// `docs/container-runtimes.md`: it creates user namespaces and mounts `/proc` in them, which the
/// default seccomp profile and the masked `/proc` paths of the runtime forbid. No extra
/// capability, no device and no socket.
const DOCKER_APPTAINER_RUN_ARGS: [&str; 4] = [
    "--security-opt",
    "seccomp=unconfined",
    "--security-opt",
    "systempaths=unconfined",
];

/// The same for rootless Podman: `unmask=ALL` is its spelling of `systempaths=unconfined`, and
/// `--userns=keep-id` maps the host user to itself. Without it the engine would run as a mapped
/// subordinate id (or as root in the user namespace, where `unsquashfs` fails changing owners of
/// the unpacked image), and Apptainer rejects both.
const PODMAN_APPTAINER_RUN_ARGS: [&str; 5] = [
    "--userns=keep-id",
    "--security-opt",
    "seccomp=unconfined",
    "--security-opt",
    "unmask=ALL",
];

/// Runs Snakemake inside an engine image through the container runtime.
#[derive(Debug, Clone)]
pub struct SnakemakeExecutor {
    runtime_binary: PathBuf,
    image: String,
    /// The runtime behind `runtime_binary`, decides the `run` options
    runtime_kind: RuntimeKind,
    /// AppArmor profile passed to Docker, see [`APPARMOR_PROFILE_VARIABLE`]
    apparmor_profile: Option<String>,
}

impl SnakemakeExecutor {
    pub fn new(runtime_binary: impl Into<PathBuf>, image: impl Into<String>) -> Self {
        Self {
            runtime_binary: runtime_binary.into(),
            image: image.into(),
            runtime_kind: RuntimeKind::Docker,
            apparmor_profile: None,
        }
    }

    /// Runs the engine container under an AppArmor profile (Docker only, Podman ignores it).
    ///
    /// # Arguments
    /// * `profile` - name of a loaded profile, `None` keeps the runtime's default
    pub fn with_apparmor_profile(mut self, profile: Option<String>) -> Self {
        self.apparmor_profile = profile;
        self
    }

    /// Tells the executor which runtime its binary belongs to (default Docker), because the
    /// options Apptainer needs differ.
    ///
    /// # Arguments
    /// * `kind` - the runtime kind of the binary given to [`Self::new`]
    pub fn with_runtime_kind(mut self, kind: RuntimeKind) -> Self {
        self.runtime_kind = kind;
        self
    }

    /// The `<runtime> run ...` command for the job, see [`containerized_spec`].
    ///
    /// Snakemake works in the project directory (`--directory`). It insists on a core count, so
    /// `--cores all` is added unless an engine parameter sets one. The workflow parameters go
    /// into a file in a scratch directory (mounted read-only, removed by [`run_spec`] after the
    /// run), see [`ParameterFile`].
    pub fn launch_spec(&self, job: &Job) -> Result<LaunchSpec, ExecutorError> {
        let config = &job.configuration;
        // Checked first so a failing job leaves no scratch directory behind
        match config.source() {
            Source::Local(_) => {}
            Source::Remote(_) => {
                return Err(ExecutorError::UnsupportedSource(
                    "remote (not cloned by the worker yet)".into(),
                ));
            }
            Source::NfCore(_) => return Err(ExecutorError::UnsupportedSource("nf-core".into())),
            Source::Image(_) => return Err(ExecutorError::UnsupportedSource("image".into())),
        }
        let parameters = ParameterFile::write(job)?;
        match self.launch_spec_with(job, parameters.as_ref()) {
            Ok(mut spec) => {
                spec.scratch_directory = parameters.map(|file| file.directory().to_path_buf());
                Ok(spec)
            }
            Err(error) => {
                if let Some(file) = parameters {
                    file.discard();
                }
                Err(error)
            }
        }
    }

    /// [`Self::launch_spec`] once the parameter file is written.
    ///
    /// # Arguments
    /// * `job` - the job to run
    /// * `parameters` - the written parameter file, `None` without parameters
    fn launch_spec_with(
        &self,
        job: &Job,
        parameters: Option<&ParameterFile>,
    ) -> Result<LaunchSpec, ExecutorError> {
        let config = &job.configuration;

        let mut read_only: Vec<&std::path::Path> = Vec::new();
        read_only.extend(job.reference_dir.as_deref());
        read_only.extend(parameters.map(ParameterFile::directory));
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
        if let Some(parameters) = parameters {
            command.extend(["--configfile".into(), path_arg(parameters.file())]);
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
            if let Some(cache) = self.cache_directory(job) {
                command.extend(["--apptainer-prefix".into(), path_arg(cache)]);
            }
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

    /// The shared image cache to use for `job`: the one of the job, unless the author's engine
    /// parameters or deployment method make Snakemake manage images itself, `None` then.
    ///
    /// # Arguments
    /// * `job` - The run about to start
    fn cache_directory<'a>(&self, job: &'a Job) -> Option<&'a Path> {
        let parameters = job.configuration.engine_parameters();
        let overridden = parameters
            .iter()
            .any(|p| CACHE_PARAMETERS.contains(&p.name()));
        let own_deployment = parameters
            .iter()
            .any(|p| DEPLOYMENT_PARAMETERS.contains(&p.name()));
        if overridden || own_deployment {
            None
        } else {
            job.apptainer_cache.as_deref()
        }
    }

    /// Extra `run` arguments of the engine container: what Apptainer needs
    /// ([`DOCKER_APPTAINER_RUN_ARGS`] / [`PODMAN_APPTAINER_RUN_ARGS`] by runtime) and the worker's
    /// user with a writable home for the image cache ([`worker_user_args`]). Apptainer cannot
    /// unpack images as root, and running as the worker keeps the files the engine writes owned
    /// by the user of the shared storage. The shared image cache of `job` is mounted read-write
    /// at its host path.
    ///
    /// # Arguments
    /// * `job` - The run about to start
    fn run_args(&self, job: &Job) -> Vec<String> {
        let mut args: Vec<String> = match self.runtime_kind {
            RuntimeKind::Podman => PODMAN_APPTAINER_RUN_ARGS.map(String::from).into(),
            _ => DOCKER_APPTAINER_RUN_ARGS.map(String::from).into(),
        };
        if let (RuntimeKind::Docker, Some(profile)) = (self.runtime_kind, &self.apparmor_profile) {
            args.extend(["--security-opt".into(), format!("apparmor={profile}")]);
        }
        args.extend(worker_user_args());
        if let Some(cache) = self.cache_directory(job) {
            let cache = path_arg(cache);
            args.extend(["-v".into(), format!("{cache}:{cache}")]);
        }
        args
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
            apptainer_cache: None,
            cwl_cache: None,
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

    /// Podman gets its own spelling of the options Apptainer needs, Docker keeps the old ones.
    #[test]
    fn podman_uses_keep_id_and_unmask() {
        let job = job(local(), vec![]);
        let podman = executor()
            .with_runtime_kind(RuntimeKind::Podman)
            .launch_spec(&job)
            .unwrap()
            .args;
        assert!(podman.contains(&"--userns=keep-id".to_string()));
        assert!(podman.contains(&"unmask=ALL".to_string()));
        assert!(!podman.contains(&"systempaths=unconfined".to_string()));
        let docker = executor().launch_spec(&job).unwrap().args;
        assert!(docker.contains(&"systempaths=unconfined".to_string()));
        assert!(!docker.contains(&"--userns=keep-id".to_string()));
    }

    /// The AppArmor profile is passed to Docker only, and only when configured.
    #[test]
    fn apparmor_profile_is_a_docker_option() {
        let job = job(local(), vec![]);
        let profiled = || executor().with_apparmor_profile(Some("macworp-snakemake".into()));
        let docker = profiled().launch_spec(&job).unwrap().args;
        assert!(docker.contains(&"apparmor=macworp-snakemake".to_string()));
        let podman = profiled()
            .with_runtime_kind(RuntimeKind::Podman)
            .launch_spec(&job)
            .unwrap()
            .args;
        assert!(!podman.iter().any(|a| a.starts_with("apparmor=")));
        let plain = executor().launch_spec(&job).unwrap().args;
        assert!(!plain.iter().any(|a| a.starts_with("apparmor=")));
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
                "--security-opt",
                "seccomp=unconfined",
                "--security-opt",
                "systempaths=unconfined",
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

    /// The shared image cache is passed to Snakemake and mounted read-write at its host path
    #[test]
    fn image_cache_is_shared() {
        let mut job = job(local(), vec![]);
        job.apptainer_cache = Some("/cache/apptainer".into());
        let args = executor().launch_spec(&job).unwrap().args;
        let prefix = args.iter().position(|a| a == "--apptainer-prefix").unwrap();
        assert_eq!(args[prefix + 1], "/cache/apptainer");
        let mount = args
            .iter()
            .position(|a| a == "/cache/apptainer:/cache/apptainer")
            .unwrap();
        assert_eq!(args[mount - 1], "-v");
    }

    /// An own cache or deployment method of the author wins, nothing is mounted then
    #[test]
    fn image_cache_yields_to_engine_parameters() {
        for name in ["apptainer-prefix", "use-conda"] {
            let params = vec![EngineParameter::new(name.into(), "x".into())];
            let mut job = job(local(), params);
            job.apptainer_cache = Some("/cache/apptainer".into());
            let args = executor().launch_spec(&job).unwrap().args;
            assert!(
                !args.iter().any(|a| a.contains("/cache/apptainer")),
                "{name}: {args:?}"
            );
        }
    }

    /// A deployment method chosen by the workflow author is not overridden
    #[test]
    fn engine_parameter_sets_deployment_method() {
        let params = vec![EngineParameter::new("use-conda".into(), String::new())];
        let args = executor().launch_spec(&job(local(), params)).unwrap().args;
        assert!(!args.iter().any(|a| a == "--software-deployment-method"));
        assert!(!args.iter().any(|a| a == "--apptainer-args"));
    }

    /// The engine runs as the worker's user, Apptainer cannot unpack as root
    #[cfg(unix)]
    #[test]
    fn engine_runs_as_worker_user() {
        let args = executor().launch_spec(&job(local(), vec![])).unwrap().args;
        let user = args.iter().position(|a| a == "--user").unwrap();
        assert_eq!(
            Some(args[user + 1].clone()),
            crate::executors::worker_user()
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
        let mut job = job(local(), vec![]);
        add_text(&mut job, "a", "line one\nline two");
        add_text(&mut job, "b", "7");
        add_text(&mut job, "ref.species", "say \"hi\"");
        let spec = executor().launch_spec(&job).unwrap();
        let directory = spec.scratch_directory.clone().unwrap();
        let position = spec.args.iter().position(|a| a == "--configfile").unwrap();
        let file = std::path::PathBuf::from(&spec.args[position + 1]);
        assert_eq!(file, directory.join("parameters.json"));
        assert!(!spec.args.iter().any(|a| a == "--config"));
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

    /// No parameter value, no file and no scratch directory
    #[test]
    fn no_values_write_no_file() {
        let spec = executor().launch_spec(&job(local(), vec![])).unwrap();
        assert_eq!(spec.scratch_directory, None);
        assert!(!spec.args.iter().any(|a| a == "--configfile"));
    }

    /// The scratch directory is gone after the run
    #[tokio::test]
    async fn scratch_directory_is_removed_after_the_run() {
        let mut job = job(local(), vec![]);
        add_text(&mut job, "a", "x");
        let mut spec = executor().launch_spec(&job).unwrap();
        let directory = spec.scratch_directory.clone().unwrap();
        assert!(directory.exists());
        // Not a real container run: replace the runtime by a command that succeeds
        spec.program = "true".into();
        spec.container_name = None;
        let (logs, _receiver) = mpsc::channel(8);
        let (_cancel, cancelled) = watch::channel(false);
        run_spec(Ok(spec), logs, cancelled).await;
        assert!(!directory.exists());
    }

    #[test]
    fn remote_source_is_rejected() {
        let source = Source::Remote(RemoteSource::new("https://x/y.git".into(), None, None));
        let result = executor().launch_spec(&job(source, vec![]));
        assert!(matches!(result, Err(ExecutorError::UnsupportedSource(_))));
    }
}
