use std::path::PathBuf;

use macworp_core::workflow_configuration::{engine::Engine, render::render_args, source::Source};
use tokio::sync::{mpsc, watch};

use macworp_core::paths::PathMapping;

use super::containerized_execution::path_arg;
use super::{
    Executor, ExecutorError, Job, LaunchSpec, LogLine, Outcome, ParameterFile, containerized_spec,
    default_engine_image, engine_image, run_process, run_spec, worker_user, worker_user_args,
};
use crate::container_runtime::BoxFuture;

/// Environment variable that replaces the default engine image as a whole, for example to run an older Nextflow
/// version (an image built from `docker/engine.dockerfile` with another version argument).
pub const IMAGE_VARIABLE: &str = "MACWORP_NEXTFLOW_IMAGE";

/// The engine image to run unless the workflow pins a version, see [`default_engine_image`]:
/// the value of [`IMAGE_VARIABLE`] if set and not empty, else the default of the engine catalog
/// on the configured registry.
pub fn image() -> String {
    default_engine_image(&Engine::Nextflow, IMAGE_VARIABLE)
}

/// Name of the generated configuration file that switches the weblog plugin on (Nextflow 26+).
const WEBLOG_CONFIG: &str = "weblog.config";

/// File name of the generated configuration that fixes how tasks are run, in the scratch
/// directory next to [`WEBLOG_CONFIG`].
const DEPLOYMENT_CONFIG: &str = "deployment.config";

/// Configuration passed with `-c` to every Nextflow run, overriding the workflow's own
/// configuration and its `-profile`s (a `-c` file takes precedence over profiles, checked on
/// Nextflow 24.10, 25.10 and 26.10). Tasks run in the engine container itself (`local`
/// executor) and, if they name an image, as sibling containers through the runtime socket. The
/// engine image only has the `docker` client, which speaks to Docker and to Podman's
/// Docker-compatible socket alike, so `docker` is the container engine on every runtime and
/// all other software deployments are switched off. The catch-all `withName: '.*'` selector
/// also beats executors that the workflow sets per process (`withName:` / `withLabel:` blocks,
/// profiles, or an `executor` directive in the script): checked on Nextflow 24.10, 25.10 and
/// 26.10, where `process.executor` alone left those on `slurm`. Other options a profile sets
/// (`process.container`, `docker.runOptions`) are left alone.
const DEPLOYMENT_CONFIG_CONTENT: &str = "\
process.executor = 'local'
process {
  withName: '.*' { executor = 'local' }
}
docker.enabled = true
podman.enabled = false
singularity.enabled = false
apptainer.enabled = false
charliecloud.enabled = false
shifter.enabled = false
conda.enabled = false
mamba.enabled = false
spack.enabled = false
";

/// Whether a job's Nextflow reports progress through the `nf-weblog` plugin instead of
/// `-with-weblog`, which Nextflow 26 removed. The plugin comes with the engine image and is
/// switched on by a generated configuration file. Decided by the version pinned in the workflow,
/// else the catalog default (also when `MACWORP_NEXTFLOW_IMAGE` replaces the image, the executor
/// cannot see what is in it). A version it cannot read is treated as the newest.
///
/// # Arguments
/// * `pinned` - the engine version pinned in the workflow configuration, if any
pub fn uses_weblog_plugin(pinned: Option<&str>) -> bool {
    let default = Engine::Nextflow
        .catalog()
        .map(|catalog| catalog.default.as_str());
    pinned
        .or(default)
        .and_then(|version| version.split('.').next())
        .and_then(|major| major.parse::<u32>().ok())
        .is_none_or(|major| major >= 26)
}

/// Runs Nextflow inside an engine image through the container runtime.
#[derive(Debug, Clone)]
pub struct NextflowExecutor {
    /// How paths on this machine look inside the runtime's containers
    paths: PathMapping,
    runtime_binary: PathBuf,
    image: String,
    engine_access: Vec<String>,
}

impl NextflowExecutor {
    pub fn new(runtime_binary: impl Into<PathBuf>, image: impl Into<String>) -> Self {
        Self {
            runtime_binary: runtime_binary.into(),
            paths: PathMapping::Native,
            image: image.into(),
            engine_access: Vec::new(),
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
                command.push(path_arg(self.paths, &local.location().join(local.script())));
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
        let plugin = uses_weblog_plugin(config.engine_version());
        let mut extra_files = vec![(DEPLOYMENT_CONFIG, DEPLOYMENT_CONFIG_CONTENT.to_string())];
        if let Some(progress) = &job.progress {
            let address = format!("{}/nextflow", progress.base_url);
            if plugin {
                extra_files.push((
                    WEBLOG_CONFIG,
                    format!(
                        "plugins {{\n  id 'nf-weblog'\n}}\nweblog {{\n  enabled = true\n  url = '{address}'\n}}\n"
                    ),
                ));
            } else {
                command.extend(["-with-weblog".into(), address]);
            }
        }
        command.extend(render_args(
            config,
            job.reference_dir.as_deref(),
            self.paths,
        )?);
        // Last step that can fail, so no scratch directory is left behind by an error
        let parameters = ParameterFile::write_with(job, self.paths, &extra_files)?;
        if let Some(parameters) = &parameters {
            for name in [WEBLOG_CONFIG, DEPLOYMENT_CONFIG] {
                if let Some(file) = parameters.extra_file(name) {
                    command.extend(["-c".into(), path_arg(self.paths, file)]);
                }
            }
            if parameters.has_values() {
                command.extend([
                    "-params-file".into(),
                    path_arg(self.paths, parameters.file()),
                ]);
            }
            read_only.push(parameters.directory());
        }

        let mut spec = containerized_spec(
            &self.runtime_binary,
            &engine_image(&self.image, job.configuration.engine_version())?,
            job,
            self.paths,
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
    /// the last run. It runs in the engine image like the run itself (same project mount) but
    /// **as root**: Nextflow task containers start through the runtime socket and may run as
    /// root (some images only work that way), so their files in `work/` can be root-owned on a
    /// Linux host and the worker's user could not delete them. Root in the container is the
    /// host user with rootless Podman. Afterwards the Nextflow bookkeeping that cleaning may have
    /// rewritten (`.nextflow`, `.nextflow.log`) is handed back to the worker's user, see
    /// [`worker_user`].
    ///
    /// # Arguments
    /// * `job` - the job whose project directory is cleaned
    pub fn cleanup_spec(&self, job: &Job) -> LaunchSpec {
        // The run itself already failed for an invalid version, so this is the pinned image
        let image = engine_image(&self.image, job.configuration.engine_version())
            .unwrap_or_else(|_| self.image.clone());
        let read_only: Vec<&std::path::Path> = Vec::new();
        let mut run_args = vec!["--user".to_string(), "0".to_string()];
        run_args.extend(self.engine_access.iter().cloned());
        let mut script = String::from("nextflow clean -f; status=$?;");
        if let Some(user) = worker_user() {
            script.push_str(&format!(
                " chown -R {user} .nextflow .nextflow.log 2>/dev/null;"
            ));
        }
        script.push_str(" exit $status");
        containerized_spec(
            &self.runtime_binary,
            &image,
            job,
            self.paths,
            &read_only,
            &run_args,
            vec!["sh".into(), "-c".into(), script],
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
            apptainer_cache: None,
            cwl_cache: None,
            configuration,
            progress: None,
        }
    }

    fn executor() -> NextflowExecutor {
        NextflowExecutor::new("podman", "img:1")
    }

    /// On a Windows host the mounts take the host path as source and the runtime's view of it
    /// as target, and everything the engine is told uses that view (identical inside and for the
    /// sibling containers).
    #[test]
    fn windows_host_paths_are_mapped_per_runtime() {
        for (mapping, project, workflow) in [
            (PathMapping::WslMount, "/mnt/c/data/p1", "/mnt/c/w/main.nf"),
            (PathMapping::DriveRoot, "/c/data/p1", "/c/w/main.nf"),
        ] {
            let mut job = job(Source::Local(LocalSource::new(
                r"C:\w".into(),
                "main.nf".into(),
            )));
            job.project_dir = r"C:\data\p1".into();
            let spec = executor()
                .with_path_mapping(mapping)
                .launch_spec(&job)
                .unwrap();
            let args = super::super::containerized_execution::without_worker_user(spec.args);
            assert!(args.contains(&format!(r"C:\data\p1:{project}")), "{args:?}");
            assert!(
                args.contains(&format!(r"C:\w:{}:ro", &workflow[..workflow.len() - 8])),
                "{args:?}"
            );
            assert_eq!(
                args[args.iter().position(|a| a == "-w").unwrap() + 1],
                project
            );
            let engine = args.iter().position(|a| a == "nextflow").unwrap();
            assert_eq!(args[engine + 2], workflow);
        }
    }

    /// The dialect follows the major version: the plugin from 26 on, the option before.
    #[test]
    fn weblog_plugin_from_version_26() {
        assert!(uses_weblog_plugin(Some("26.10.6")));
        assert!(uses_weblog_plugin(Some("27.04.0")));
        assert!(!uses_weblog_plugin(Some("25.10.9")));
        assert!(!uses_weblog_plugin(Some("24.10.9")));
        let default = Engine::Nextflow.catalog().unwrap().default.clone();
        assert_eq!(uses_weblog_plugin(None), uses_weblog_plugin(Some(&default)));
    }

    /// Nextflow 26 has no `-with-weblog`: a generated config file in the scratch directory
    /// enables the `nf-weblog` plugin and is passed with `-c`, without parameters too.
    #[test]
    fn version_26_reports_progress_through_the_plugin() {
        let mut job = job(Source::Local(LocalSource::new(
            "/w".into(),
            "main.nf".into(),
        )));
        *job.configuration.engine_version_mut() = Some("26.10.6".into());
        job.progress = Some(ProgressEndpoint {
            base_url: "http://host:4000/tok".into(),
            run_args: vec![],
        });
        let spec = executor().launch_spec(&job).unwrap();
        assert!(!spec.args.contains(&"-with-weblog".to_string()));
        assert!(!spec.args.contains(&"-params-file".to_string()));
        let position = spec.args.iter().position(|a| a == "-c").unwrap();
        let directory = spec.scratch_directory.clone().unwrap();
        let file = PathBuf::from(&spec.args[position + 1]);
        assert_eq!(file, directory.join("weblog.config"));
        let config = std::fs::read_to_string(&file).unwrap();
        assert!(config.contains("id 'nf-weblog'"));
        assert!(config.contains("url = 'http://host:4000/tok/nextflow'"));
        assert!(
            spec.args
                .contains(&format!("{0}:{0}:ro", directory.display()))
        );
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn progress_endpoint_adds_weblog_and_host_mapping() {
        let mut job = job(Source::Local(LocalSource::new(
            "/w".into(),
            "main.nf".into(),
        )));
        // `-with-weblog` is the option of Nextflow 25 and older
        *job.configuration.engine_version_mut() = Some("25.10.9".into());
        job.progress = Some(ProgressEndpoint {
            base_url: "http://host.docker.internal:4000/tok".into(),
            run_args: vec![
                "--add-host".into(),
                "host.docker.internal:host-gateway".into(),
            ],
        });
        let args = executor().launch_spec(&job).unwrap().args;
        // run option before the image, engine option after the script
        let image = args.iter().position(|a| a == "img:25.10.9").unwrap();
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
        let args = executor()
            .launch_spec(&job(Source::Local(LocalSource::new(
                "/w".into(),
                "main.nf".into(),
            ))))
            .unwrap()
            .args;
        let user = args.iter().position(|a| a == "--user").unwrap();
        assert_eq!(
            Some(args[user + 1].clone()),
            crate::executors::worker_user()
        );
        assert!(args.windows(2).any(|pair| pair == ["-e", "HOME=/tmp"]));
    }

    /// The cleanup runs `nextflow clean -f` as root in the engine image and the project
    /// directory, then returns the bookkeeping files to the worker's user.
    #[test]
    fn cleanup_runs_nextflow_clean_as_root_in_the_engine_image() {
        let spec = executor().cleanup_spec(&job(Source::Local(LocalSource::new(
            "/w".into(),
            "main.nf".into(),
        ))));
        let args = spec.args;
        let image = args.iter().position(|a| a == "img:1").unwrap();
        assert!(args[..image].windows(2).any(|pair| pair == ["--user", "0"]));
        assert_eq!(args[..image].iter().filter(|a| *a == "--user").count(), 1);
        assert_eq!(&args[image + 1..image + 3], ["sh", "-c"]);
        let script = &args[image + 3];
        assert!(script.starts_with("nextflow clean -f;"), "{script}");
        assert!(script.ends_with("exit $status"), "{script}");
        #[cfg(unix)]
        assert!(script.contains(&format!(
            "chown -R {} .nextflow .nextflow.log",
            crate::executors::worker_user().unwrap()
        )));
        assert!(args.contains(&"/data/p1:/data/p1".to_string()));
        assert!(!args.iter().any(|a| a.ends_with(":ro")));
    }

    /// Removes what every launch adds for [`DEPLOYMENT_CONFIG`] (the read-only mount of the
    /// scratch directory and `-c <file>`) and deletes the directory, so tests of other
    /// arguments stay readable. Checks that the file is there first.
    ///
    /// # Arguments
    /// * `spec` - the launch specification to strip
    fn without_deployment_config(mut spec: LaunchSpec) -> LaunchSpec {
        let directory = spec.scratch_directory.take().unwrap();
        let file = directory.join(DEPLOYMENT_CONFIG);
        assert!(file.is_file());
        let mount = format!("{0}:{0}:ro", directory.display());
        let at = spec.args.iter().position(|a| *a == mount).unwrap();
        spec.args.drain(at - 1..=at);
        let at = spec
            .args
            .iter()
            .position(|a| *a == file.display().to_string())
            .unwrap();
        spec.args.drain(at - 1..=at);
        std::fs::remove_dir_all(directory).unwrap();
        spec
    }

    /// Every launch passes the generated deployment configuration with `-c`, after the weblog
    /// one, whatever the runtime and version: local executor, `docker` as the only container
    /// engine and no other software deployment.
    #[test]
    fn deployment_is_fixed_by_a_generated_configuration() {
        for version in ["24.10.9", "26.10.6"] {
            let mut job = job(Source::Local(LocalSource::new(
                "/w".into(),
                "main.nf".into(),
            )));
            *job.configuration.engine_version_mut() = Some(version.into());
            let spec = executor().launch_spec(&job).unwrap();
            let directory = spec.scratch_directory.clone().unwrap();
            let file = directory.join(DEPLOYMENT_CONFIG);
            let position = spec
                .args
                .iter()
                .position(|a| *a == file.display().to_string())
                .unwrap();
            assert_eq!(spec.args[position - 1], "-c");
            let config = std::fs::read_to_string(&file).unwrap();
            for line in [
                "process.executor = 'local'",
                "withName: '.*' { executor = 'local' }",
                "docker.enabled = true",
                "podman.enabled = false",
                "singularity.enabled = false",
                "conda.enabled = false",
            ] {
                assert!(config.lines().any(|l| l.trim() == line), "{line}");
            }
            std::fs::remove_dir_all(directory).unwrap();
        }
    }

    // Native mapping with POSIX host paths: a Windows host uses a mapped style (see the `PathMapping` tests).
    #[cfg(unix)]
    #[test]
    fn local_source() {
        let spec = without_deployment_config(
            executor()
                .launch_spec(&job(Source::Local(LocalSource::new(
                    "/w".into(),
                    "main.nf".into(),
                ))))
                .unwrap(),
        );
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
        let spec = without_deployment_config(
            executor()
                .launch_spec(&job(Source::Remote(RemoteSource::new(
                    "https://example.org/x.git".into(),
                    Some("v1".into()),
                    None,
                ))))
                .unwrap(),
        );
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
            let spec = without_deployment_config(
                executor()
                    .launch_spec(&job(Source::NfCore(NfCoreSource::new(name.into(), None))))
                    .unwrap(),
            );
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

    /// A pinned engine version selects that tag of the engine image, a bad one fails the job.
    #[test]
    fn pinned_engine_version_selects_the_image_tag() {
        let mut job = job(Source::Local(LocalSource::new(
            "/w".into(),
            "main.nf".into(),
        )));
        *job.configuration.engine_version_mut() = Some("24.10.0".into());
        let spec = executor().launch_spec(&job).unwrap();
        assert!(spec.args.contains(&"img:24.10.0".to_string()));
        assert!(!spec.args.contains(&"img:1".to_string()));
        *job.configuration.engine_version_mut() = Some("--privileged".into());
        assert!(matches!(
            executor().launch_spec(&job),
            Err(ExecutorError::InvalidEngineVersion(_))
        ));
    }

    #[test]
    fn image_source_is_rejected() {
        let result = executor().launch_spec(&job(Source::Image(Default::default())));
        assert!(matches!(result, Err(ExecutorError::UnsupportedSource(_))));
    }
}
