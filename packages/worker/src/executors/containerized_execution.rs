use std::path::{Path, PathBuf};

use tokio::sync::{mpsc, watch};

use super::{ExecutorError, Job, LaunchSpec, LogLine, Outcome, run_process};

pub(super) fn path_arg(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

/// `<runtime> run --rm --name macworp-run-<id> -v <project>:<project> -w <project> [-v <dir>:<dir>:ro]... <image> <command>`
///
/// The project directory is mounted read-write and every `read_only` directory read-only, all at
/// their host path so mounts the engine creates itself resolve on the host. `engine_access` are
/// extra `run` arguments for sibling-container access ([`crate::container_runtime::RuntimeInfo::engine_access_args`]).
/// Hardening is not part of it yet, that is P6.
pub fn containerized_spec(
    runtime_binary: &Path,
    image: &str,
    job: &Job,
    read_only: &[&Path],
    engine_access: &[String],
    command: Vec<String>,
) -> LaunchSpec {
    let container_name = format!("macworp-run-{}", job.run_id);
    let project_dir = path_arg(&job.project_dir);

    let mut args: Vec<String> = vec![
        "run".into(),
        "--rm".into(),
        "--name".into(),
        container_name.clone(),
        "-v".into(),
        format!("{project_dir}:{project_dir}"),
        "-w".into(),
        project_dir,
    ];
    for dir in read_only {
        let dir = path_arg(dir);
        args.extend(["-v".into(), format!("{dir}:{dir}:ro")]);
    }
    args.extend(engine_access.iter().cloned());
    if let Some(progress) = &job.progress {
        args.extend(progress.run_args.iter().cloned());
    }
    args.push(image.to_string());
    args.extend(command);

    LaunchSpec {
        program: PathBuf::from(runtime_binary),
        args,
        container_name: Some(container_name),
    }
}

/// Runs the spec, or reports why there is none as a failed run.
pub async fn run_spec(
    spec: Result<LaunchSpec, ExecutorError>,
    logs: mpsc::Sender<LogLine>,
    cancel: watch::Receiver<bool>,
) -> Outcome {
    match spec {
        Ok(spec) => run_process(&spec, logs, cancel).await,
        Err(e) => Outcome::Failed {
            exit_code: None,
            error: Some(e.to_string()),
        },
    }
}
