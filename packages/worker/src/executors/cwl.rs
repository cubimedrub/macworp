use std::path::PathBuf;

use macworp_core::workflow_configuration::{engine::Engine, render::render_args, source::Source};
use tokio::sync::{mpsc, watch};

use macworp_core::paths::PathMapping;

use super::containerized_execution::{mount_arg, path_arg};
use super::{
    Executor, ExecutorError, Job, LaunchSpec, LogLine, Outcome, ParameterFile, containerized_spec,
    default_engine_image, engine_image, run_spec, worker_user_args,
};
use crate::container_runtime::{BoxFuture, RuntimeKind};

/// Environment variable that replaces the default engine image as a whole, for example to run another Toil version
/// (an image built from `docker/engine.dockerfile` with another version argument).
pub const IMAGE_VARIABLE: &str = "MACWORP_CWL_IMAGE";

/// Directory inside the project directory where Toil keeps its job store and working files.
/// It has to be below the project directory: Toil mounts files from its working directory into
/// the sibling tool containers, so the path must exist on the host as well.
pub const WORK_DIRECTORY: &str = ".toil";

/// Engine parameters that already say where the job store or working files go, so no default is
/// added
const LOCATION_PARAMETERS: &[&str] = &["jobStore", "workDir"];

/// Directory below [`WORK_DIRECTORY`] where cwltool puts the temporary folders of the steps when
/// a step cache is used. Without `--cachedir` Toil's file store handles them; with it cwltool
/// falls back to the `/tmp` of the engine container, which the sibling tool containers cannot
/// mount from the host.
const TEMPORARY_DIRECTORY: &str = "tmp";

/// Engine parameters that choose the step cache themselves, so the shared one is not added
const CACHE_PARAMETERS: &[&str] = &["cachedir"];

/// Memory Toil reserves for a job that declares none (`--defaultMemory`). Toil's own default is
/// 2 GiB, which it refuses to schedule on a container runtime VM with less memory ("requesting
/// ... more than the maximum"), also on a restart of a failed run. The value is a scheduling
/// reservation, not a limit, and tools that need more declare `ramMin` themselves.
const DEFAULT_MEMORY: &str = "512M";

/// The engine image to run unless the workflow pins a version, see [`default_engine_image`]:
/// the value of [`IMAGE_VARIABLE`] if set and not empty, else the default of the engine catalog
/// on the configured registry.
pub fn image() -> String {
    default_engine_image(&Engine::Cwl, IMAGE_VARIABLE)
}

/// Runs CWL workflows with Toil (`toil-cwl-runner`) inside an engine image through the container
/// runtime. `DockerRequirement` images are started as sibling containers through the runtime
/// socket, like Nextflow does.
#[derive(Debug, Clone)]
pub struct CwlExecutor {
    /// How paths on this machine look inside the runtime's containers
    paths: PathMapping,
    runtime_binary: PathBuf,
    image: String,
    engine_access: Vec<String>,
    /// Runtime of `runtime_binary`: tool containers started through a Podman socket need Toil's
    /// `--podman` mode
    runtime_kind: RuntimeKind,
}

impl CwlExecutor {
    /// # Arguments
    /// * `runtime_binary` - the container runtime binary (`docker`, `podman`)
    /// * `image` - the engine image with Toil, see [`image`]
    pub fn new(runtime_binary: impl Into<PathBuf>, image: impl Into<String>) -> Self {
        Self {
            runtime_binary: runtime_binary.into(),
            paths: PathMapping::Native,
            image: image.into(),
            engine_access: Vec::new(),
            runtime_kind: RuntimeKind::Docker,
        }
    }

    /// How paths on this machine look inside the runtime's containers, see [`PathMapping`]
    /// (default: unchanged).
    ///
    /// # Arguments
    /// * `paths` - The mapping of the runtime behind this executor
    pub fn with_path_mapping(mut self, paths: PathMapping) -> Self {
        self.paths = paths;
        self
    }

    /// Tells the executor which runtime its binary belongs to (default Docker). With Podman Toil
    /// runs in its `--podman` mode: cwltool then starts the tool containers with
    /// `--userns=keep-id`, so the mounted project files stay writable for the tool (a rootless
    /// Podman socket otherwise maps the tool's user to a subordinate id that does not own them).
    ///
    /// # Arguments
    /// * `kind` - the runtime kind of the binary given to [`Self::new`]
    pub fn with_runtime_kind(mut self, kind: RuntimeKind) -> Self {
        self.runtime_kind = kind;
        self
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
    /// A run never restarts the job store of an earlier run (Toil would reuse the stored inputs
    /// and ignore corrected values). Resuming instead starts afresh with the shared step cache
    /// of the job (`--cachedir`, see [`Job::cwl_cache`]): steps whose inputs did not change are
    /// served from it. cwltool then needs its temporary folders on a host path the tool
    /// containers can mount, they go below [`WORK_DIRECTORY`].
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
            path_arg(self.paths, &job.project_dir),
            "--disableProgress".into(),
        ];
        if self.runtime_kind == RuntimeKind::Podman {
            command.push("--podman".into());
        }
        if !Self::has_own_location(job) {
            let work = job.project_dir.join(WORK_DIRECTORY);
            command.extend([
                "--workDir".into(),
                path_arg(self.paths, &work),
                "--jobStore".into(),
                path_arg(self.paths, &work.join("jobstore")),
            ]);
        }
        if !configuration
            .engine_parameters()
            .iter()
            .any(|p| p.name() == "defaultMemory")
        {
            command.extend(["--defaultMemory".into(), DEFAULT_MEMORY.into()]);
        }
        if let Some(cache) = Self::cache_directory(job) {
            let temporary = path_arg(
                self.paths,
                &job.project_dir
                    .join(WORK_DIRECTORY)
                    .join(TEMPORARY_DIRECTORY),
            );
            command.extend([
                "--cachedir".into(),
                path_arg(self.paths, cache),
                "--tmp-outdir-prefix".into(),
                format!("{temporary}/"),
                "--tmpdir-prefix".into(),
                format!("{temporary}/"),
            ]);
        }
        // Images the worker built are only in the runtime's local store: Toil's pre-pull
        // (`docker pull` of every image of the document) would fail on them and abort the run
        if !configuration.images().is_empty()
            && !configuration
                .engine_parameters()
                .iter()
                .any(|p| p.name() == "no-prepull")
        {
            command.push("--no-prepull".into());
        }
        command.extend(render_args(
            configuration,
            job.reference_dir.as_deref(),
            self.paths,
        )?);
        command.push(path_arg(self.paths, &local.location().join(local.script())));
        // Last step that can fail, so no scratch directory is left behind by an error
        let parameters = ParameterFile::write(job, self.paths)?;
        if let Some(parameters) = &parameters {
            command.push(path_arg(self.paths, parameters.file()));
            read_only.push(parameters.directory());
        }
        let mut spec = containerized_spec(
            &self.runtime_binary,
            &engine_image(&self.image, job.configuration.engine_version())?,
            job,
            self.paths,
            &read_only,
            &self.run_args(job),
            command,
        );
        spec.scratch_directory = parameters.map(|file| file.directory().to_path_buf());
        Ok(spec)
    }

    /// Makes the working directories of a run: [`WORK_DIRECTORY`] is emptied first (the job store
    /// of a failed run is still there and a fresh run needs an empty one, unless the author's
    /// engine parameters put it elsewhere), Toil does not create its working directory itself,
    /// and with a step cache cwltool's temporary folder ([`TEMPORARY_DIRECTORY`]) must exist.
    ///
    /// # Arguments
    /// * `job` - the run about to start
    fn prepare_directories(job: &Job) -> Result<(), ExecutorError> {
        let work = job.project_dir.join(WORK_DIRECTORY);
        if !Self::has_own_location(job) {
            match std::fs::remove_dir_all(&work) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => {
                    return Err(ExecutorError::Io("the old Toil working directory", error));
                }
            }
        }
        std::fs::create_dir_all(&work)
            .map_err(|error| ExecutorError::Io("the Toil working directory", error))?;
        if Self::cache_directory(job).is_some() {
            std::fs::create_dir_all(work.join(TEMPORARY_DIRECTORY))
                .map_err(|error| ExecutorError::Io("the Toil temporary directory", error))?;
        }
        Ok(())
    }

    /// Whether the author's engine parameters say where the job store or working files go, so
    /// [`WORK_DIRECTORY`] is not used (and not cleared before a run).
    ///
    /// # Arguments
    /// * `job` - the run about to start
    fn has_own_location(job: &Job) -> bool {
        job.configuration
            .engine_parameters()
            .iter()
            .any(|p| LOCATION_PARAMETERS.contains(&p.name()))
    }

    /// Whether the Toil version a job runs on has the step cache option (`--cachedir`): from
    /// Toil 8 on. Toil 7 rejects it (the value would be read as the workflow), so a resumed run
    /// there recomputes every step. The version is the one pinned in the workflow, else the
    /// catalog default (also when `MACWORP_CWL_IMAGE` replaces the image).
    ///
    /// # Arguments
    /// * `pinned` - the engine version pinned in the workflow configuration, if any
    pub fn supports_step_cache(pinned: Option<&str>) -> bool {
        let default = Engine::Cwl
            .catalog()
            .map(|catalog| catalog.default.as_str());
        pinned
            .or(default)
            .and_then(|version| version.split('.').next())
            .and_then(|major| major.parse::<u32>().ok())
            .is_none_or(|major| major >= 8)
    }

    /// The shared step cache to use for `job`: the one of the job unless the author's engine
    /// parameters choose a cache themselves or the Toil version has none
    /// ([`Self::supports_step_cache`]), `None` then (and where the job has none).
    ///
    /// # Arguments
    /// * `job` - the run about to start
    fn cache_directory(job: &Job) -> Option<&std::path::Path> {
        if !Self::supports_step_cache(job.configuration.engine_version()) {
            return None;
        }
        let own = job
            .configuration
            .engine_parameters()
            .iter()
            .any(|p| CACHE_PARAMETERS.contains(&p.name()));
        if own { None } else { job.cwl_cache.as_deref() }
    }

    /// Extra `run` arguments of the engine container: the worker's user
    /// ([`worker_user_args`]), the runtime socket access and the shared step cache of `job`,
    /// mounted read-write at its host path.
    ///
    /// # Arguments
    /// * `job` - the run about to start
    fn run_args(&self, job: &Job) -> Vec<String> {
        let mut args = worker_user_args();
        args.extend(self.engine_access.iter().cloned());
        if let Some(cache) = Self::cache_directory(job) {
            args.extend(["-v".into(), mount_arg(self.paths, cache, false)]);
        }
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
        let spec = self
            .launch_spec(job)
            .and_then(|spec| Self::prepare_directories(job).map(|()| spec));
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
    #[cfg(unix)]
    use macworp_core::workflow_configuration::parameters::{
        Parameter, parameter::IsParameterWithValue, text::Text,
    };
    use macworp_core::workflow_configuration::{
        WorkflowConfiguration,
        engine::EngineParameter,
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
            apptainer_cache: None,
            cwl_cache: None,
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

    /// Toil runs the tool containers in its Podman mode on a Podman runtime only.
    #[test]
    fn podman_runtime_selects_podman_mode() {
        let docker = executor().launch_spec(&job(local())).unwrap().args;
        assert!(!docker.contains(&"--podman".to_string()));
        let podman = executor()
            .with_runtime_kind(RuntimeKind::Podman)
            .launch_spec(&job(local()))
            .unwrap()
            .args;
        assert!(podman.contains(&"--podman".to_string()));
    }

    /// A workflow with built images runs Toil without its pre-pull, others keep it.
    #[test]
    fn built_images_disable_the_prepull() {
        use macworp_core::workflow_configuration::images::ImageBuild;
        let plain = executor().launch_spec(&job(local())).unwrap().args;
        assert!(!plain.contains(&"--no-prepull".to_string()));
        let mut with_images = job(local());
        with_images
            .configuration
            .images_mut()
            .push(ImageBuild::new("Dockerfile".into(), "team/tool:1".into()));
        let args = executor().launch_spec(&with_images).unwrap().args;
        assert_eq!(args.iter().filter(|a| *a == "--no-prepull").count(), 1);
    }

    /// The step cache is passed from Toil 8 on, Toil 7 does not know `--cachedir`.
    #[test]
    fn step_cache_needs_toil_8() {
        for (version, cached) in [("9.6.0", true), ("8.2.0", true), ("7.0.0", false)] {
            let mut job = job(local());
            job.cwl_cache = Some("/cache/cwl".into());
            *job.configuration.engine_version_mut() = Some(version.into());
            let args = executor().launch_spec(&job).unwrap().args;
            assert_eq!(
                args.contains(&"--cachedir".to_string()),
                cached,
                "{version}"
            );
            assert_eq!(
                args.contains(&"/cache/cwl:/cache/cwl".to_string()),
                cached,
                "{version}"
            );
            assert_eq!(
                args.contains(&"--tmp-outdir-prefix".to_string()),
                cached,
                "{version}"
            );
        }
    }

    /// The engine runs as the worker's user, so what it writes belongs to the storage user
    #[cfg(unix)]
    #[test]
    fn engine_runs_as_worker_user() {
        let args = executor().launch_spec(&job(local())).unwrap().args;
        let user = args.iter().position(|a| a == "--user").unwrap();
        assert_eq!(
            Some(args[user + 1].clone()),
            crate::executors::worker_user()
        );
        assert!(args.windows(2).any(|pair| pair == ["-e", "HOME=/tmp"]));
    }

    /// Outputs go to the project, work files below it, the tool comes last.
    // Native mapping with POSIX host paths: a Windows host uses a mapped style (see the `PathMapping` tests).
    #[cfg(unix)]
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
                "--defaultMemory",
                "512M",
                "/w/tool.cwl"
            ]
        );
    }

    /// The shared step cache is passed to Toil with temporary folders below the work directory
    /// and mounted read-write at its host path.
    // Native mapping with POSIX host paths: a Windows host uses a mapped style (see the `PathMapping` tests).
    #[cfg(unix)]
    #[test]
    fn step_cache_is_shared() {
        let mut job = job(local());
        job.cwl_cache = Some("/cache/cwl".into());
        let args = executor().launch_spec(&job).unwrap().args;
        let at = args.iter().position(|a| a == "--cachedir").unwrap();
        assert_eq!(
            args[at..at + 6],
            [
                "--cachedir",
                "/cache/cwl",
                "--tmp-outdir-prefix",
                "/data/p1/.toil/tmp/",
                "--tmpdir-prefix",
                "/data/p1/.toil/tmp/"
            ]
        );
        assert!(args.contains(&"/cache/cwl:/cache/cwl".to_string()));
    }

    /// Without a cache in the job, or with an own `cachedir` of the author, nothing is added and
    /// nothing is mounted.
    #[test]
    fn step_cache_is_optional_and_yields_to_engine_parameters() {
        let args = executor().launch_spec(&job(local())).unwrap().args;
        assert!(!args.iter().any(|a| a == "--cachedir"));

        let mut job = job(local());
        job.cwl_cache = Some("/cache/cwl".into());
        *job.configuration.engine_parameters_mut() =
            vec![EngineParameter::new("cachedir".into(), "/own".into())];
        let args = executor().launch_spec(&job).unwrap().args;
        assert_eq!(args.iter().filter(|a| *a == "--cachedir").count(), 1);
        assert!(!args.iter().any(|a| a.contains("/cache/cwl")));
    }

    /// The job store of an earlier run is removed so a fresh run can start, the temporary folder
    /// of the step cache is made, and an author's own job store location leaves `.toil` alone.
    #[test]
    fn old_job_store_is_cleared_before_a_run() {
        let project = tempfile::tempdir().unwrap();
        let old = project.path().join(WORK_DIRECTORY).join("jobstore");
        std::fs::create_dir_all(&old).unwrap();
        let mut job = cleanup_job(project.path(), &[]);
        job.cwl_cache = Some("/cache/cwl".into());
        CwlExecutor::prepare_directories(&job).unwrap();
        assert!(!old.exists());
        assert!(project.path().join(WORK_DIRECTORY).join("tmp").is_dir());

        std::fs::create_dir_all(&old).unwrap();
        CwlExecutor::prepare_directories(&cleanup_job(project.path(), &["jobStore"])).unwrap();
        assert!(old.exists());
    }

    /// Engine parameters go before the tool, and a given job store replaces the default.
    // Native mapping with POSIX host paths: a Windows host uses a mapped style (see the `PathMapping` tests).
    #[cfg(unix)]
    #[test]
    fn engine_parameters_replace_locations() {
        let mut job = job(local());
        *job.configuration.engine_parameters_mut() = vec![
            EngineParameter::new("jobStore".into(), "/data/js".into()),
            EngineParameter::new("maxCores".into(), "2".into()),
        ];
        let args = executor().launch_spec(&job).unwrap().args;
        assert!(!args.contains(&"/data/p1/.toil/jobstore".to_string()));
        let tail = &args[args.len() - 7..];
        assert_eq!(
            tail,
            [
                "--defaultMemory",
                "512M",
                "--jobStore",
                "/data/js",
                "--maxCores",
                "2",
                "/w/tool.cwl"
            ]
        );
    }

    /// A workflow that sets `defaultMemory` itself is not given the default twice.
    #[test]
    fn default_memory_yields_to_an_engine_parameter() {
        let mut job = job(local());
        *job.configuration.engine_parameters_mut() =
            vec![EngineParameter::new("defaultMemory".into(), "4G".into())];
        let args = executor().launch_spec(&job).unwrap().args;
        assert_eq!(args.iter().filter(|a| *a == "--defaultMemory").count(), 1);
        assert!(args.contains(&"4G".to_string()));
        assert!(!args.contains(&"512M".to_string()));
    }

    /// Values go into a read-only mounted JSON input object after the tool.
    // Native mapping with POSIX host paths: a Windows host uses a mapped style (see the `PathMapping` tests).
    #[cfg(unix)]
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
