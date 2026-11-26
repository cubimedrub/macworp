//! Makes the images a workflow built locally (ADR 0007) usable by Snakemake. Snakemake runs
//! `container:` rules with Apptainer inside the engine container, which cannot see the runtime's
//! local images and would look the name up on Docker Hub. It does not pull when the image cache
//! already holds `<prefix>/<md5 of the container URL>.simg`, so the worker converts the built
//! image into exactly that file beforehand: `<runtime> save`, then `apptainer build` from the
//! archive in a short run of the engine image.
//!
//! The file name is an implementation detail of Snakemake (the same on 7, 8 and 9 in the spike of
//! 2026-11-23). The URL must equal the string of the Snakefile, which is `docker://<name>` with
//! the name the maintainer entered.

use std::path::{Path, PathBuf};

use macworp_core::{entities::LogStream, paths::PathMapping};
use md5::{Digest, Md5};
use tokio::{
    process::Command,
    sync::{mpsc, watch},
};

use rand::{RngExt, distr::Alphanumeric};

use super::containerized_execution::path_arg;
use super::{Job, LaunchSpec, LogLine, Outcome, run_process, selinux_args, selinux_enforcing};
use crate::console_window::HideConsoleWindow;
use crate::image_builder::failed;

/// File next to a seeded image that records which image it was made from.
const SOURCE_SUFFIX: &str = "macworp-image";

/// File name Snakemake uses for the image of `name` in its cache: the md5 of `docker://<name>`.
///
/// # Arguments
/// * `name` - image name as the workflow's Snakefile uses it, without the `docker://` scheme
pub fn cache_file_name(name: &str) -> String {
    let digest = Md5::digest(format!("docker://{name}").as_bytes());
    let hex: String = digest.iter().map(|byte| format!("{byte:02x}")).collect();
    format!("{hex}.simg")
}

/// How the conversion container is started: the same runtime, engine image, `run` options and
/// path view as the engine run it prepares.
pub struct ConversionRun<'a> {
    /// Container runtime binary
    pub runtime: &'a Path,
    /// Snakemake engine image, it provides Apptainer
    pub engine_image: &'a str,
    /// The `run` options of the engine container (user, security options, cache mount)
    pub run_arguments: &'a [String],
    /// How host paths look inside the runtime's containers, applied to every path the
    /// conversion command line gives to the container (the `docker save` call on the host keeps
    /// host paths)
    pub mapping: PathMapping,
}

/// Seeds the image cache with every image of the job's workflow that is missing or was made
/// from another image. Runs under the name reservation of the run, so no other run changes what
/// the names point to meanwhile.
///
/// # Arguments
/// * `conversion` - how the conversion container is started
/// * `job` - the claimed run
/// * `cache` - the Apptainer image cache the engine is given as its prefix
/// * `logs` - run log
/// * `cancel` - turns `true` when a user stops the run
pub async fn seed(
    conversion: &ConversionRun<'_>,
    job: &Job,
    cache: &Path,
    logs: &mpsc::Sender<LogLine>,
    cancel: watch::Receiver<bool>,
) -> Outcome {
    let ConversionRun {
        runtime, mapping, ..
    } = *conversion;
    for image in job.configuration.images() {
        let say = |text: String| async {
            let _ = logs
                .send(LogLine {
                    stream: LogStream::Engine,
                    line: text,
                })
                .await;
        };
        let target = cache.join(cache_file_name(image.name()));
        let marker = target.with_extension(format!("simg.{SOURCE_SUFFIX}"));
        let id = match image_id(runtime, image.name()).await {
            Ok(id) => id,
            Err(error) => return failed(format!("Cannot read image {}: {error}", image.name())),
        };
        if target.is_file() && std::fs::read_to_string(&marker).is_ok_and(|seen| seen.trim() == id)
        {
            say(format!(
                "Apptainer image of {} is up to date, not converting again",
                image.name()
            ))
            .await;
            continue;
        }
        say(format!(
            "Converting image {} for Snakemake's Apptainer image cache",
            image.name()
        ))
        .await;
        let scratch = match tempfile::Builder::new().prefix(".seed-").tempdir_in(cache) {
            Ok(scratch) => scratch,
            Err(error) => {
                return failed(format!("Cannot write to the image cache: {error}"));
            }
        };
        let archive = scratch.path().join("image.tar");
        let converted = scratch.path().join("image.simg");
        let save = LaunchSpec {
            program: runtime.to_path_buf(),
            args: vec![
                "save".into(),
                "--output".into(),
                archive.display().to_string(),
                image.name().to_string(),
            ],
            container_name: None,
            scratch_directory: None,
        };
        match run_process(&save, logs.clone(), cancel.clone()).await {
            Outcome::Succeeded => {}
            other => return other,
        }
        let build = conversion_spec(
            conversion,
            cache,
            vec![
                "apptainer".into(),
                "build".into(),
                "--force".into(),
                path_arg(mapping, &converted),
                format!("docker-archive:{}", path_arg(mapping, &archive)),
            ],
        );
        match run_process(&build, logs.clone(), cancel.clone()).await {
            Outcome::Succeeded => {}
            other => return other,
        }
        // Atomic on the same filesystem: a run that reads the cache never sees half a file
        if let Err(error) = std::fs::rename(&converted, &target)
            .and_then(|()| std::fs::write(&marker, format!("{id}\n")))
        {
            return failed(format!("Cannot store the converted image: {error}"));
        }
    }
    Outcome::Succeeded
}

/// The `<runtime> run` command of the conversion. Unlike the engine run it does not mount the
/// project directory: it is not needed, and a runtime that creates a missing mount source (Docker)
/// would create the project directory owned by root.
///
/// # Arguments
/// * `conversion` - how the container is started, its `run_arguments` mount `cache`
/// * `cache` - the Apptainer image cache, the working directory (host path, mapped here)
/// * `command` - what to run in the container
fn conversion_spec(
    conversion: &ConversionRun<'_>,
    cache: &Path,
    command: Vec<String>,
) -> LaunchSpec {
    let suffix: String = rand::rng()
        .sample_iter(Alphanumeric)
        .take(8)
        .map(char::from)
        .collect();
    let container_name = format!("macworp-seed-{suffix}");
    let mut args: Vec<String> = vec![
        "run".into(),
        "--rm".into(),
        "--name".into(),
        container_name.clone(),
        "-w".into(),
        path_arg(conversion.mapping, cache),
    ];
    args.extend(selinux_args(selinux_enforcing()));
    args.extend(conversion.run_arguments.iter().cloned());
    args.push(conversion.engine_image.to_string());
    args.extend(command);
    LaunchSpec {
        program: conversion.runtime.to_path_buf(),
        args,
        container_name: Some(container_name),
        scratch_directory: None,
    }
}

/// The content id of the image currently tagged `name`.
///
/// # Arguments
/// * `runtime` - container runtime binary
/// * `name` - image name or tag
async fn image_id(runtime: &Path, name: &str) -> Result<String, String> {
    let output = Command::new(runtime)
        .hide_console_window()
        .args(["image", "inspect", "--format", "{{.Id}}", name])
        .stdin(std::process::Stdio::null())
        .output()
        .await
        .map_err(|error| error.to_string())?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    let id = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if id.is_empty() {
        return Err("the runtime returned no image id".into());
    }
    Ok(id)
}

/// Where `seed` keeps the converted image of `name` below `cache`, for tests and diagnostics.
///
/// # Arguments
/// * `cache` - the Apptainer image cache
/// * `name` - image name as the Snakefile uses it
pub fn cache_file(cache: &Path, name: &str) -> PathBuf {
    cache.join(cache_file_name(name))
}

#[cfg(all(test, unix))]
mod tests {
    use std::os::unix::fs::PermissionsExt;

    use macworp_core::workflow_configuration::{
        WorkflowConfiguration,
        images::ImageBuild,
        source::{LocalSource, Source},
    };

    use super::*;

    /// The name Snakemake gives the cache file is the md5 of the URL (value computed with
    /// Python's `hashlib.md5`, which is what Snakemake uses).
    #[test]
    fn cache_file_name_is_the_md5_of_the_url() {
        assert_eq!(
            cache_file_name("myteam/tool:1.0"),
            "eda8cdab8c0dd351ae01bf07cb6c9711.simg"
        );
    }

    /// A runtime that records its calls in `calls`, answers `image inspect` with the content of
    /// `id` and creates the output file of `apptainer build`.
    fn fake_runtime(directory: &Path) -> (PathBuf, PathBuf) {
        let calls = directory.join("calls");
        let script = directory.join("runtime");
        std::fs::write(
            &script,
            format!(
                "#!/bin/sh\necho \"$@\" >> '{calls}'\n\
                 if [ \"$1\" = image ]; then cat '{id}'; exit 0; fi\n\
                 if [ \"$1\" = run ]; then\n\
                   for argument in \"$@\"; do\n\
                     if [ \"$previous\" = --force ]; then echo converted > \"$argument\"; fi\n\
                     previous=\"$argument\"\n\
                   done\n\
                 fi\n\
                 exit 0\n",
                calls = calls.display(),
                id = directory.join("id").display(),
            ),
        )
        .unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        (script, calls)
    }

    fn job() -> Job {
        let mut configuration = WorkflowConfiguration::default();
        *configuration.source_mut() =
            Source::Local(LocalSource::new("/w".into(), "Snakefile".into()));
        configuration.images_mut().push(ImageBuild::new(
            "Dockerfile".into(),
            "myteam/tool:1.0".into(),
        ));
        Job {
            run_id: 1,
            project_dir: "/p".into(),
            reference_dir: None,
            apptainer_cache: None,
            cwl_cache: None,
            configuration,
            progress: None,
        }
    }

    /// The image is converted once per image id, again when the name points at another image.
    #[tokio::test]
    async fn converts_when_the_image_changed() {
        let directory = tempfile::tempdir().unwrap();
        let cache = directory.path().join("cache");
        std::fs::create_dir_all(&cache).unwrap();
        let (runtime, calls) = fake_runtime(directory.path());
        std::fs::write(directory.path().join("id"), "sha256:one\n").unwrap();
        let (logs, _rx) = mpsc::channel(64);
        let (_tx, cancel) = watch::channel(false);
        let arguments = vec!["--user".to_string(), "1000".to_string()];
        let job = job();
        let conversion = ConversionRun {
            runtime: &runtime,
            engine_image: "engine:1",
            run_arguments: &arguments,
            mapping: PathMapping::Native,
        };
        let run = || seed(&conversion, &job, &cache, &logs, cancel.clone());
        let converted = |calls: &Path| {
            std::fs::read_to_string(calls)
                .unwrap_or_default()
                .lines()
                .filter(|line| line.contains("apptainer build"))
                .count()
        };

        assert_eq!(run().await, Outcome::Succeeded);
        let target = cache_file(&cache, "myteam/tool:1.0");
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "converted\n");
        assert_eq!(converted(&calls), 1);
        let recorded = std::fs::read_to_string(&calls).unwrap();
        assert!(recorded.contains("save --output"));
        assert!(recorded.contains("docker-archive:"));
        assert!(recorded.contains("--user 1000"));

        // Same image: nothing to do
        assert_eq!(run().await, Outcome::Succeeded);
        assert_eq!(converted(&calls), 1);

        // The name points at other content: replaced
        std::fs::write(directory.path().join("id"), "sha256:two\n").unwrap();
        assert_eq!(run().await, Outcome::Succeeded);
        assert_eq!(converted(&calls), 2);
        // No scratch directory is left in the cache
        let leftovers = std::fs::read_dir(&cache)
            .unwrap()
            .flatten()
            .filter(|entry| entry.file_name().to_string_lossy().starts_with(".seed-"))
            .count();
        assert_eq!(leftovers, 0);
    }

    /// Every host path in the conversion's container command line is mapped for the runtime's
    /// VM, the working directory included.
    #[test]
    fn conversion_maps_host_paths() {
        let cache = Path::new(r"C:\cache");
        let command = vec![format!(
            "docker-archive:{}",
            path_arg(PathMapping::WslMount, &cache.join("image.tar"))
        )];
        for (mapping, root) in [
            (PathMapping::WslMount, "/mnt/c/cache"),
            (PathMapping::DriveRoot, "/c/cache"),
        ] {
            let conversion = ConversionRun {
                runtime: Path::new("podman"),
                engine_image: "engine:1",
                run_arguments: &[],
                mapping,
            };
            let spec = conversion_spec(&conversion, cache, command.clone());
            let position = spec
                .args
                .iter()
                .position(|argument| argument == "-w")
                .unwrap();
            assert_eq!(spec.args[position + 1], root);
        }
    }
}
