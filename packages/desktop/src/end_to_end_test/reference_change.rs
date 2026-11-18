//! What does a run observe when the global reference data changes while it runs?
//!
//! Each workflow (`tests/fixtures/end_to_end/reference_change`) reads reference file `a.txt`,
//! then waits, then reads `b.txt`. The test changes the reference data during the wait, either
//! by replacing both files the way an upload does (rename over the target) or by deleting `b.txt`.
//! Every combination runs twice: once with the worker reading the live folder (what a run
//! without a snapshot sees) and once reading a hard-link snapshot taken before the run (what the
//! standalone backend hands out). The snapshot runs must see the old data and succeed; the live
//! runs only report what happens, they document the problem the snapshot solves.
//!
//! Nextflow and Snakemake signal with `ready` and wait for `go` in the project directory. The
//! CWL tool cannot reach the project directory and sleeps instead, the test changes the data a
//! while after the run started. The Container engine is not covered.
//!
//! ```text
//! cargo test -p macworp_desktop reference_change -- --ignored --nocapture
//! ```

use std::{fs, path::Path, time::Duration};

use macworp_backend::{
    reference_directory::ReferenceDirectory, reference_snapshots::ReferenceSnapshots,
};
use macworp_core::{
    entities::RunStatus,
    workflow_configuration::{engine::Engine, WorkflowConfiguration},
};
use serde_json::json;

use super::{path, prepare_image, require_runtime, TestApp};

/// Content of the reference files before the change
const OLD: [&str; 2] = ["old-a", "old-b"];

/// How long the CWL tool sleeps, and how long after the start the test changes the data
const CWL_SLEEP: Duration = Duration::from_secs(60);

/// How the reference data is changed while a run is between its two reads.
#[derive(Clone, Copy, Debug)]
enum Change {
    /// Both files are replaced by new content (an admin uploads a new reference set)
    Replace,
    /// `b.txt` is deleted
    DeleteSecond,
}

impl Change {
    /// Applies the change to the live reference folder like the reference API does: new content
    /// is written next to the file and renamed over it, deletion unlinks.
    ///
    /// # Arguments
    /// * `reference` - The live reference folder
    fn apply(self, reference: &Path) {
        match self {
            Self::Replace => {
                for (name, content) in [("a.txt", "new-a"), ("b.txt", "new-b")] {
                    let partial = reference.join(format!(".{name}.macworp_partial_upload"));
                    fs::write(&partial, content).unwrap();
                    fs::rename(partial, reference.join(name)).unwrap();
                }
            }
            Self::DeleteSecond => fs::remove_file(reference.join("b.txt")).unwrap(),
        }
    }
}

/// What a run saw: its end state and, if it wrote one, its result.
#[derive(Debug)]
struct Observation {
    /// Final status of the run
    status: RunStatus,
    /// Content of the result file, `None` if the run wrote none
    result: Option<String>,
    /// Log text, shown when a snapshot run does not behave
    log: String,
}

/// Workflow definition that reads `a.txt` and `b.txt` of the reference data.
///
/// # Arguments
/// * `engine` - Nextflow, Snakemake or Cwl
/// * `workflow_directory` - Folder holding the workflow files
fn configuration(engine: &Engine, workflow_directory: &Path) -> WorkflowConfiguration {
    let (engine_parameters, script) = match engine {
        Engine::Nextflow => (json!([{"name": "ansi-log", "value": "false"}]), "main.nf"),
        Engine::Snakemake => (json!([]), "Snakefile"),
        Engine::Cwl => (json!([]), "tool.cwl"),
        other => panic!("no reference change workflow for {other}"),
    };
    let reference = |name: &str| format!("{}/{name}", macworp_core::paths::REFERENCE_PREFIX);
    let reference_a = reference("a.txt");
    let reference_b = reference("b.txt");
    serde_json::from_value(json!({
        "engine": engine,
        "engine_parameters": engine_parameters,
        "source": {"Local": {"location": workflow_directory, "script": script}},
        "fixed_parameters": [
            path("ref_a", false, 1, &[reference_a.as_str()]),
            path("ref_b", false, 1, &[reference_b.as_str()]),
        ],
        "dynamic_parameters": [],
    }))
    .expect("reference change workflow configuration is valid")
}

/// Runs the reference change workflow of `engine` once.
///
/// `None` (with a message) when the engine cannot run here.
///
/// # Arguments
/// * `engine` - Nextflow, Snakemake or Cwl
/// * `change` - What happens to the reference data mid-run
/// * `snapshot` - Whether the worker reads a snapshot taken before the run instead of the live folder
async fn observe(engine: Engine, change: Change, snapshot: bool) -> Option<Observation> {
    let (runtime, unavailable_engines) = require_runtime(&engine).await?;
    if !prepare_image(runtime.as_ref(), &engine).await {
        return None;
    }
    let app = TestApp::start(runtime, unavailable_engines).await;
    let fixtures =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/end_to_end/reference_change");
    for file in ["main.nf", "Snakefile", "tool.cwl"] {
        fs::copy(fixtures.join(file), app.workflow_dir.join(file)).unwrap();
    }
    fs::create_dir_all(&app.project_dir).unwrap();
    fs::write(app.reference_directory.join("a.txt"), OLD[0]).unwrap();
    fs::write(app.reference_directory.join("b.txt"), OLD[1]).unwrap();

    if snapshot {
        let snapshots = ReferenceSnapshots::new(
            ReferenceDirectory::new(app.reference_directory.clone()),
            app.reference_directory
                .with_file_name("reference_snapshots"),
        );
        let created = snapshots.create().unwrap();
        app.worker_reference.set(snapshots.path_of(&created.name));
    }

    let run = app
        .start_run(configuration(&engine, &app.workflow_dir), Vec::new())
        .await
        .unwrap();

    // Wait until the run is between its reads, then change the data and let it go on
    if matches!(engine, Engine::Cwl) {
        let started = std::time::Instant::now();
        while app
            .client
            .runs()
            .get(&app.jwt, run.id)
            .await
            .unwrap()
            .status
            != RunStatus::Running
        {
            assert!(
                started.elapsed() < Duration::from_secs(120),
                "run did not start"
            );
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
        tokio::time::sleep(CWL_SLEEP / 3).await;
        change.apply(&app.reference_directory);
    } else {
        let started = std::time::Instant::now();
        while !app.project_dir.join("ready").exists() {
            let current = app.client.runs().get(&app.jwt, run.id).await.unwrap();
            assert!(
                !current.status.is_terminal(),
                "run ended before it was ready: {current:?}"
            );
            assert!(
                started.elapsed() < Duration::from_secs(180),
                "run never got ready"
            );
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
        change.apply(&app.reference_directory);
        fs::write(app.project_dir.join("go"), "").unwrap();
    }

    let (run, log) = app.finished_run(run.id).await;
    let project_dir = app.project_dir.clone();
    let _temp = app.stop().await;
    let result = ["results/result.txt", "result.txt"]
        .iter()
        .find_map(|name| fs::read_to_string(project_dir.join(name)).ok())
        .map(|content| content.trim().to_string());
    Some(Observation {
        status: run.status,
        result,
        log,
    })
}

/// Runs all combinations for `engine`, prints what the live-folder runs saw and asserts that the
/// snapshot runs saw the old data and succeeded.
///
/// # Arguments
/// * `engine` - Nextflow, Snakemake or Cwl
async fn check_engine(engine: Engine) {
    for change in [Change::Replace, Change::DeleteSecond] {
        let Some(live) = observe(engine.clone(), change, false).await else {
            return;
        };
        eprintln!(
            "RESULT {engine} {change:?} live folder: status {:?}, result {:?}",
            live.status, live.result
        );
        // A live run that fails although only files were replaced is surprising: show why
        if live.status != RunStatus::Succeeded && matches!(change, Change::Replace) {
            let tail: Vec<&str> = live.log.lines().rev().take(40).collect();
            eprintln!(
                "LIVE LOG TAIL {engine}:\n{}",
                tail.into_iter().rev().collect::<Vec<_>>().join("\n")
            );
        }
        let snapshot = observe(engine.clone(), change, true)
            .await
            .expect("the engine ran before");
        eprintln!(
            "RESULT {engine} {change:?} snapshot:    status {:?}, result {:?}",
            snapshot.status, snapshot.result
        );
        assert_eq!(
            snapshot.status,
            RunStatus::Succeeded,
            "{engine} {change:?} with snapshot\n--- log ---\n{}",
            snapshot.log
        );
        assert_eq!(
            snapshot.result.as_deref(),
            Some("a=old-a b=old-b"),
            "{engine} {change:?} with snapshot\n--- log ---\n{}",
            snapshot.log
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs a container runtime and the macworp-nextflow image"]
async fn reference_change_nextflow() {
    check_engine(Engine::Nextflow).await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs docker and the macworp-snakemake image"]
async fn reference_change_snakemake() {
    check_engine(Engine::Snakemake).await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs a container runtime, the macworp-cwl image and the debian image"]
async fn reference_change_cwl() {
    check_engine(Engine::Cwl).await;
}
