use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use macworp_core::{
    entities::{FailureReason, LogStream},
    workflow_configuration::engine::Engine,
};
use tokio::{
    sync::{mpsc, watch},
    task::JoinHandle,
    time::{MissedTickBehavior, interval},
};

use crate::{
    apptainer_cache_store::ApptainerCacheStore,
    container_runtime::{RuntimeKind, SharedFolders},
    executors::{Executor, Job, LogLine, Outcome},
    image_builder::{ImageBuilder, ImageHold},
    image_store::ImageStore,
    job_source::{Error as SourceError, JobSource},
    log_proxy::{HostAccess, ProgressReceiver},
    progress::ProgressUpdate,
};

#[derive(Debug, Clone)]
pub struct WorkerOptions {
    /// Log lines are sent to the source in chunks, at least this often ...
    pub log_flush_interval: Duration,
    /// ... or as soon as one stream has buffered this many bytes
    pub log_flush_bytes: usize,
    /// How often the source is asked whether the run should be cancelled
    pub cancel_poll_interval: Duration,
    /// Engine progress is reported to the source at most this often (the latest state is sent)
    pub progress_flush_interval: Duration,
    /// Wait between claims when the queue is empty
    pub idle_interval: Duration,
    /// How often reporting the result to the source is tried before it is given up (the run then
    /// recovers through lease expiry), at least 1
    pub finish_attempts: u32,
    /// Wait between attempts to report the result. Attempts times interval should stay well below
    /// the backend's lease so a short outage does not make the run execute twice.
    pub finish_retry_interval: Duration,
    /// An engine that reports progress (Nextflow, Snakemake) sends its first event right after it
    /// starts. If none arrived this long after its first output, a warning goes to the run log:
    /// the receiver is probably unreachable (firewall). Only the wait for the first event is
    /// limited, never the gaps between events or the total run time.
    pub progress_warning_after: Duration,
    /// Same wait, after which the run is failed with that explanation. `None` waits forever, for
    /// engine images that cannot report progress.
    pub progress_timeout: Option<Duration>,
}

impl Default for WorkerOptions {
    fn default() -> Self {
        Self {
            log_flush_interval: Duration::from_secs(1),
            log_flush_bytes: 64 * 1024,
            cancel_poll_interval: Duration::from_secs(2),
            progress_flush_interval: Duration::from_millis(250),
            idle_interval: Duration::from_secs(2),
            finish_attempts: 5,
            finish_retry_interval: Duration::from_secs(5),
            progress_warning_after: Duration::from_secs(120),
            progress_timeout: Some(Duration::from_secs(600)),
        }
    }
}

/// Claims jobs from a [`JobSource`] and runs them one at a time with the executor of their engine.
pub struct Worker {
    source: Arc<dyn JobSource>,
    executors: HashMap<Engine, Arc<dyn Executor>>,
    options: WorkerOptions,
    progress: Option<Arc<ProgressReceiver>>,
    /// Builders of the images of workflows, one per usable container runtime
    image_builders: Vec<ImageBuilder>,
    /// Image usage tracking and collection, one per container runtime
    image_stores: Vec<Arc<ImageStore>>,
    /// Trims the Apptainer image cache of Snakemake after runs
    apptainer_cache_store: Option<Arc<ApptainerCacheStore>>,
    /// The folders the container VM sees (macOS); a run whose folders are outside fails to
    /// start with a clear message instead of an engine mount error. `None` where the VM sees
    /// everything.
    shared_folders: Option<Arc<SharedFolders>>,
}

impl Worker {
    pub fn new(
        source: Arc<dyn JobSource>,
        executors: Vec<Arc<dyn Executor>>,
        options: WorkerOptions,
    ) -> Self {
        Self {
            source,
            executors: executors.into_iter().map(|e| (e.engine(), e)).collect(),
            options,
            progress: None,
            image_builders: Vec::new(),
            image_stores: Vec::new(),
            apptainer_cache_store: None,
            shared_folders: None,
        }
    }

    /// Lets the worker build the images of workflows that bring a build file, with the builder
    /// of the runtime the job's executor uses. Without one such runs fail to start.
    ///
    /// # Arguments
    /// * `builders` - one per container runtime of this worker
    pub fn with_image_builders(mut self, builders: Vec<ImageBuilder>) -> Self {
        self.image_builders = builders;
        self
    }

    /// Lets the worker record which images its runs used and remove the least recently used
    /// ones when the stores' limits are exceeded.
    ///
    /// # Arguments
    /// * `stores` - one per container runtime of this worker, shared by all slots
    pub fn with_image_stores(mut self, stores: Vec<Arc<ImageStore>>) -> Self {
        self.image_stores = stores;
        self
    }

    /// Lets the worker trim the Apptainer image cache of the runs it executes.
    ///
    /// # Arguments
    /// * `store` - shared by all slots
    pub fn with_apptainer_cache_store(mut self, store: Arc<ApptainerCacheStore>) -> Self {
        self.apptainer_cache_store = Some(store);
        self
    }

    /// Lets the worker refuse runs whose folders the container VM cannot see.
    ///
    /// # Arguments
    /// * `shared_folders` - what the VM of the runtime shares, see [`SharedFolders::detect`]
    pub fn with_shared_folders(mut self, shared_folders: Option<Arc<SharedFolders>>) -> Self {
        self.shared_folders = shared_folders;
        self
    }

    /// Engines that support it report their progress to the receiver
    pub fn with_progress_receiver(mut self, receiver: Arc<ProgressReceiver>) -> Self {
        self.progress = Some(receiver);
        self
    }

    /// Claims and runs one job. `false` if there was none.
    pub async fn run_next(&self) -> Result<bool, SourceError> {
        let Some(job) = self.source.claim().await? else {
            return Ok(false);
        };
        let run_id = job.run_id;
        let outcome = self.execute(job).await;
        tracing::info!("Run {run_id} ended: {outcome:?}");
        self.finish_with_retry(run_id, outcome).await?;
        Ok(true)
    }

    /// Reports the result to the source, retrying transient failures up to
    /// `options.finish_attempts` times so a short outage does not throw the result away.
    ///
    /// # Arguments
    /// * `run_id` - run that ended
    /// * `outcome` - how it ended
    async fn finish_with_retry(&self, run_id: i64, outcome: Outcome) -> Result<(), SourceError> {
        let attempts = self.options.finish_attempts.max(1);
        let mut attempt = 1;
        loop {
            match self.source.finish(run_id, outcome.clone()).await {
                Ok(()) => return Ok(()),
                Err(e) if attempt >= attempts => return Err(e),
                Err(e) => {
                    tracing::warn!(
                        "Reporting the result of run {run_id} failed (attempt {attempt}/{attempts}): {e}"
                    );
                    attempt += 1;
                    tokio::time::sleep(self.options.finish_retry_interval).await;
                }
            }
        }
    }

    /// Runs jobs until `shutdown` turns `true`. A running job is finished first.
    pub async fn run_until(&self, mut shutdown: watch::Receiver<bool>) {
        while !*shutdown.borrow() {
            match self.run_next().await {
                Ok(true) => continue,
                Ok(false) => {}
                Err(e) => tracing::error!("Job source failed: {e}"),
            }
            tokio::select! {
                _ = tokio::time::sleep(self.options.idle_interval) => {}
                _ = shutdown.changed() => {}
            }
        }
    }

    async fn execute(&self, mut job: Job) -> Outcome {
        let engine = job.configuration.engine().clone();
        let Some(executor) = self.executors.get(&engine) else {
            return Outcome::Failed {
                exit_code: None,
                error: Some(format!("This worker has no executor for {engine}")),
                reason: Some(FailureReason::StartFailed),
            };
        };

        if let Some(outcome) = self.unshared_folders_outcome(&job) {
            return outcome;
        }

        // Registered for the whole run: the token stops working when this is dropped
        let mut registered = self.progress.as_ref().and_then(|receiver| {
            let access = executor.runtime_kind().and_then(HostAccess::for_runtime);
            receiver.register(job.run_id, &engine, access.as_ref())
        });
        let mut progress_task = None;
        // Turns `true` with the first event of the engine, see `watch_first_progress`
        let mut reported_rx = watch::channel(false).1;
        if let Some(registered) = &mut registered {
            job.progress = Some(registered.endpoint.clone());
            reported_rx = registered.first_event();
            progress_task = registered.take_updates().map(|updates| {
                tokio::spawn(forward_progress(
                    self.source.clone(),
                    job.run_id,
                    updates,
                    self.options.progress_flush_interval,
                ))
            });
        }
        let expects_progress = progress_task.is_some();

        // Tracked from before the first container of the run, so the image events cover it
        let image_store = self.image_store_for(executor.runtime_kind());
        let image_tracking = image_store.as_ref().map(|store| store.begin_run());

        let cache_tracking = self
            .apptainer_cache_store
            .as_ref()
            .zip(job.apptainer_cache.clone())
            .map(|(store, directory)| (store.clone(), directory, store.begin_run()));

        let (cancel_tx, cancel_rx) = watch::channel(false);
        let cancel_tx = Arc::new(cancel_tx);
        let (log_tx, log_rx) = mpsc::channel(256);
        // Turns `true` with the first line of the engine's output
        let (started_tx, started_rx) = watch::channel(false);

        let logs = tokio::spawn(forward_logs(
            self.source.clone(),
            job.run_id,
            log_rx,
            self.options.clone(),
            started_tx,
        ));
        let cancel_poll = tokio::spawn(poll_cancel(
            self.source.clone(),
            job.run_id,
            cancel_tx.clone(),
            self.options.cancel_poll_interval,
        ));

        // Images the workflow brings are built before the engine starts
        // Kept until the run ended: the names stay reserved while the engine uses the images
        let mut image_hold = None;
        let built = self
            .build_images(
                &job,
                executor.runtime_kind(),
                &log_tx,
                &cancel_rx,
                &mut image_hold,
            )
            .await;

        // Own task, so a panic in the executor fails this run instead of ending the worker loop
        let executor = executor.clone();
        let silent = Arc::new(AtomicBool::new(false));
        let mut watchdog: Option<JoinHandle<()>> = None;
        let outcome = match built {
            Some(outcome) => {
                // Closes the log channel, the forwarding task ends once the rest is flushed
                drop(log_tx);
                outcome
            }
            None => match {
                if expects_progress {
                    watchdog = Some(tokio::spawn(watch_first_progress(
                        started_rx,
                        reported_rx,
                        log_tx.clone(),
                        cancel_tx.clone(),
                        silent.clone(),
                        self.options.progress_warning_after,
                        self.options.progress_timeout,
                    )));
                }
                tokio::spawn({
                    let job = job.clone();
                    async move { executor.run(&job, log_tx, cancel_rx).await }
                })
            }
            .await
            {
                Ok(outcome) => outcome,
                Err(e) => Outcome::Failed {
                    exit_code: None,
                    error: Some(format!("The executor crashed: {e}")),
                    reason: Some(FailureReason::StartFailed),
                },
            },
        };

        // Holds a log sender: stopped before the log task is awaited
        if let Some(watchdog) = watchdog {
            watchdog.abort();
        }
        let outcome = match outcome {
            Outcome::Cancelled if silent.load(Ordering::SeqCst) => Outcome::Failed {
                exit_code: None,
                error: Some(NO_PROGRESS_ERROR.into()),
                reason: Some(FailureReason::StartFailed),
            },
            outcome => outcome,
        };

        drop(image_hold);
        if let (Some(store), Some(tracking)) = (image_store, image_tracking) {
            store.end_run(tracking).await;
        }
        if let Some((store, directory, started)) = cache_tracking {
            store.end_run(&directory, started).await;
        }
        cancel_poll.abort();
        // The sender is gone once the executor returned, this flushes the rest
        let _ = logs.await;
        // Unregistering closes the update channel, the task then sends what is left
        drop(registered);
        if let Some(task) = progress_task {
            let _ = task.await;
        }
        outcome
    }
}

impl Worker {
    /// How the run ends when a folder it needs is outside what the container VM shares, `None`
    /// when all are visible (or the VM sees everything).
    ///
    /// # Arguments
    /// * `job` - the claimed run
    fn unshared_folders_outcome(&self, job: &Job) -> Option<Outcome> {
        let shared = self.shared_folders.as_ref()?;
        let folders = [
            Some(&job.project_dir),
            job.reference_dir.as_ref(),
            job.apptainer_cache.as_ref(),
            job.cwl_cache.as_ref(),
        ];
        let missing = shared.missing(folders.into_iter().flatten().map(|path| path.as_path()));
        if missing.is_empty() {
            return None;
        }
        let list: Vec<String> = missing
            .iter()
            .map(|path| format!("`{}`", path.display()))
            .collect();
        Some(Outcome::Failed {
            exit_code: None,
            error: Some(format!(
                "{} does not share {}, so the engine cannot use it. To fix it, {}",
                shared.owner(),
                list.join(", "),
                shared.fix()
            )),
            reason: Some(FailureReason::StartFailed),
        })
    }

    /// The image store of the runtime `kind` (the first one if unknown or none matches).
    ///
    /// # Arguments
    /// * `kind` - runtime of the job's executor
    fn image_store_for(&self, kind: Option<RuntimeKind>) -> Option<Arc<ImageStore>> {
        kind.and_then(|kind| {
            self.image_stores
                .iter()
                .find(|store| store.runtime().kind() == kind)
        })
        .or_else(|| self.image_stores.first())
        .cloned()
    }

    /// Builds the images of the job's workflow, if it has any.
    ///
    /// # Arguments
    /// * `job` - the claimed run
    /// * `kind` - runtime of the job's executor, its builder is used (the first one if unknown)
    /// * `logs` - run log
    /// * `cancel` - turns `true` when a user stops the run
    /// * `held` - receives the reservation of the image names, to keep until the run ended
    ///
    /// # Returns
    /// `None` when the engine can start (nothing to build, or all built), otherwise how the run
    /// ends instead.
    async fn build_images(
        &self,
        job: &Job,
        kind: Option<RuntimeKind>,
        logs: &mpsc::Sender<LogLine>,
        cancel: &watch::Receiver<bool>,
        held: &mut Option<ImageHold>,
    ) -> Option<Outcome> {
        if job.configuration.images().is_empty() {
            return None;
        }
        let builder = self
            .image_builders
            .iter()
            .find(|builder| Some(builder.kind()) == kind)
            .or(self.image_builders.first());
        let Some(builder) = builder else {
            return Some(Outcome::Failed {
                exit_code: None,
                error: Some("This worker cannot build container images".into()),
                reason: Some(FailureReason::StartFailed),
            });
        };
        match builder.build_all(job, logs, cancel.clone()).await {
            Ok(hold) => {
                *held = Some(hold);
                None
            }
            Err(outcome) => Some(outcome),
        }
    }
}

async fn poll_cancel(
    source: Arc<dyn JobSource>,
    run_id: i64,
    cancel: Arc<watch::Sender<bool>>,
    every: Duration,
) {
    let mut ticker = interval(every);
    ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
    loop {
        ticker.tick().await;
        match source.cancel_requested(run_id).await {
            Ok(true) => {
                let _ = cancel.send(true);
                return;
            }
            Ok(false) => {}
            Err(e) => tracing::warn!("Cannot check cancellation of run {run_id}: {e}"),
        }
    }
}

/// Forwards the progress of a run to the source, coalescing updates that arrive faster than
/// `every`: numbers are absolute, so only the latest matters. Error reports are never dropped.
async fn forward_progress(
    source: Arc<dyn JobSource>,
    run_id: i64,
    mut updates: mpsc::UnboundedReceiver<ProgressUpdate>,
    every: Duration,
) {
    let mut ticker = interval(every);
    ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
    let mut pending: Option<ProgressUpdate> = None;
    loop {
        tokio::select! {
            update = updates.recv() => match update {
                Some(update) => {
                    pending = Some(match pending.take() {
                        Some(previous) => merge(previous, update),
                        None => update,
                    });
                }
                None => {
                    send_progress(&*source, run_id, pending.take()).await;
                    return;
                }
            },
            _ = ticker.tick() => send_progress(&*source, run_id, pending.take()).await,
        }
    }
}

async fn send_progress(source: &dyn JobSource, run_id: i64, update: Option<ProgressUpdate>) {
    if let Some(update) = update
        && let Err(e) = source.report_progress(run_id, update).await
    {
        tracing::warn!("Cannot report progress of run {run_id}: {e}");
    }
}

/// `next` is newer: its numbers win, a message wins if there is one, errors are joined
fn merge(previous: ProgressUpdate, next: ProgressUpdate) -> ProgressUpdate {
    ProgressUpdate {
        total: next.total,
        done: next.done,
        message: next.message.or(previous.message),
        error: match (previous.error, next.error) {
            (Some(a), Some(b)) => Some(format!("{a}\n{b}")),
            (a, b) => a.or(b),
        },
    }
}

/// Why a run is failed when the engine never reported progress.
const NO_PROGRESS_ERROR: &str = "The engine reported no progress after it started, its reports probably cannot reach the worker (firewall: allow the worker program, see `macworp_worker doctor`) or the engine image cannot report progress";

/// Guards the wait for the first progress event of an engine that reports progress. The clock
/// starts with the engine's first output (pulling and building before it do not count) and ends
/// with the first event, from then on nothing is watched: tasks may take hours between events.
/// After `warning_after` a line is written to the run log, after `timeout` the run is cancelled
/// and marked by `silent` so it ends as failed instead of cancelled.
///
/// # Arguments
/// * `started` - turns `true` with the first output of the engine
/// * `reported` - turns `true` with the first progress event
/// * `log` - run log, receives the warning
/// * `cancel` - stops the run
/// * `silent` - set when the run is stopped because of the timeout
/// * `warning_after` - wait for the first event until the warning
/// * `timeout` - wait for the first event until the run fails, `None` for never
async fn watch_first_progress(
    mut started: watch::Receiver<bool>,
    mut reported: watch::Receiver<bool>,
    log: mpsc::Sender<LogLine>,
    cancel: Arc<watch::Sender<bool>>,
    silent: Arc<AtomicBool>,
    warning_after: Duration,
    timeout: Option<Duration>,
) {
    if started.wait_for(|started| *started).await.is_err() {
        return;
    }
    let first_event = async |reported: &mut watch::Receiver<bool>, wait: Duration| {
        tokio::select! {
            _ = reported.wait_for(|reported| *reported) => true,
            _ = tokio::time::sleep(wait) => false,
        }
    };
    if first_event(&mut reported, warning_after).await {
        return;
    }
    let _ = log
        .send(LogLine {
            stream: LogStream::Engine,
            line: format!(
                "No progress report from the engine for {} s. If this persists the run is failed: its reports probably cannot reach the worker (firewall: allow the worker program, see `macworp_worker doctor`)",
                warning_after.as_secs()
            ),
        })
        .await;
    let Some(timeout) = timeout else {
        return;
    };
    if first_event(&mut reported, timeout.saturating_sub(warning_after)).await {
        return;
    }
    let _ = log
        .send(LogLine {
            stream: LogStream::Engine,
            line: format!(
                "No progress report from the engine for {} s, stopping the run",
                timeout.as_secs()
            ),
        })
        .await;
    silent.store(true, Ordering::SeqCst);
    let _ = cancel.send(true);
}

/// Buffers lines per stream and sends them as chunks.
async fn forward_logs(
    source: Arc<dyn JobSource>,
    run_id: i64,
    mut lines: mpsc::Receiver<LogLine>,
    options: WorkerOptions,
    started: watch::Sender<bool>,
) {
    // Stream order is fixed so chunks of one flush are sent deterministically
    let mut buffers: [(LogStream, Vec<String>, usize); 3] = [
        (LogStream::Stdout, Vec::new(), 0),
        (LogStream::Stderr, Vec::new(), 0),
        (LogStream::Engine, Vec::new(), 0),
    ];
    let mut ticker = interval(options.log_flush_interval);
    ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);

    loop {
        let mut closed = false;
        tokio::select! {
            line = lines.recv() => match line {
                Some(LogLine { stream, line }) => {
                    started.send_replace(true);
                    let (_, buffer, bytes) = buffers.iter_mut().find(|(s, ..)| *s == stream).unwrap();
                    *bytes += line.len() + 1;
                    buffer.push(line);
                    if *bytes < options.log_flush_bytes {
                        continue;
                    }
                }
                None => closed = true,
            },
            _ = ticker.tick() => {}
        }

        for (stream, buffer, bytes) in buffers.iter_mut() {
            if buffer.is_empty() {
                continue;
            }
            let chunk = std::mem::take(buffer);
            *bytes = 0;
            if let Err(e) = source.append_logs(run_id, *stream, chunk).await {
                tracing::warn!("Cannot send logs of run {run_id}: {e}");
            }
        }
        if closed {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;
    use crate::{container_runtime::BoxFuture, log_proxy::HostAccess};

    #[derive(Default)]
    struct FakeSource {
        jobs: Mutex<Vec<Job>>,
        logs: Mutex<Vec<(i64, LogStream, Vec<String>)>>,
        finished: Mutex<Vec<(i64, Outcome)>>,
        cancel: Mutex<bool>,
        progress: Mutex<Vec<(i64, ProgressUpdate)>>,
        /// Number of `finish` calls that still fail before one succeeds
        finish_failures: Mutex<u32>,
    }

    impl JobSource for FakeSource {
        fn claim(&self) -> BoxFuture<'_, Result<Option<Job>, SourceError>> {
            Box::pin(async { Ok(self.jobs.lock().unwrap().pop()) })
        }
        fn append_logs(
            &self,
            run_id: i64,
            stream: LogStream,
            lines: Vec<String>,
        ) -> BoxFuture<'_, Result<(), SourceError>> {
            Box::pin(async move {
                self.logs.lock().unwrap().push((run_id, stream, lines));
                Ok(())
            })
        }
        fn report_progress(
            &self,
            run_id: i64,
            update: ProgressUpdate,
        ) -> BoxFuture<'_, Result<(), SourceError>> {
            Box::pin(async move {
                self.progress.lock().unwrap().push((run_id, update));
                Ok(())
            })
        }
        fn cancel_requested(&self, _: i64) -> BoxFuture<'_, Result<bool, SourceError>> {
            Box::pin(async { Ok(*self.cancel.lock().unwrap()) })
        }
        fn finish(&self, run_id: i64, outcome: Outcome) -> BoxFuture<'_, Result<(), SourceError>> {
            Box::pin(async move {
                let mut failures = self.finish_failures.lock().unwrap();
                if *failures > 0 {
                    *failures -= 1;
                    return Err(SourceError("transient".into()));
                }
                self.finished.lock().unwrap().push((run_id, outcome));
                Ok(())
            })
        }
    }

    /// Emits lines, then waits for cancel if `wait`.
    struct FakeExecutor {
        wait: bool,
    }

    impl Executor for FakeExecutor {
        fn engine(&self) -> Engine {
            Engine::Nextflow
        }
        fn run<'a>(
            &'a self,
            _: &'a Job,
            logs: mpsc::Sender<LogLine>,
            mut cancel: watch::Receiver<bool>,
        ) -> BoxFuture<'a, Outcome> {
            Box::pin(async move {
                for (stream, line) in [
                    (LogStream::Stdout, "a"),
                    (LogStream::Stderr, "b"),
                    (LogStream::Stdout, "c"),
                ] {
                    logs.send(LogLine {
                        stream,
                        line: line.into(),
                    })
                    .await
                    .unwrap();
                }
                if self.wait {
                    let _ = cancel.wait_for(|c| *c).await;
                    Outcome::Cancelled
                } else {
                    Outcome::Failed {
                        exit_code: Some(1),
                        error: None,
                        reason: Some(FailureReason::ExitCode),
                    }
                }
            })
        }
    }

    /// Panics instead of returning an outcome.
    struct PanickingExecutor;

    impl Executor for PanickingExecutor {
        fn engine(&self) -> Engine {
            Engine::Nextflow
        }
        fn run<'a>(
            &'a self,
            _: &'a Job,
            _: mpsc::Sender<LogLine>,
            _: watch::Receiver<bool>,
        ) -> BoxFuture<'a, Outcome> {
            Box::pin(async { panic!("boom") })
        }
    }

    fn job(engine: &str) -> Job {
        let configuration = serde_json::from_value(serde_json::json!({
            "engine": engine,
            "engine_parameters": [],
            "source": { "Local": { "location": "/w", "script": "main.nf" } },
            "dynamic_parameters": [],
            "fixed_parameters": []
        }))
        .unwrap();
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

    fn fast() -> WorkerOptions {
        WorkerOptions {
            log_flush_interval: Duration::from_millis(20),
            cancel_poll_interval: Duration::from_millis(20),
            idle_interval: Duration::from_millis(20),
            ..Default::default()
        }
    }

    fn worker(source: &Arc<FakeSource>, wait: bool) -> Worker {
        Worker::new(
            source.clone(),
            vec![Arc::new(FakeExecutor { wait })],
            fast(),
        )
    }

    /// Reports two tasks the way Nextflow does, through the endpoint it was given
    struct WeblogExecutor;

    impl Executor for WeblogExecutor {
        fn engine(&self) -> Engine {
            Engine::Nextflow
        }
        fn run<'a>(
            &'a self,
            job: &'a Job,
            _: mpsc::Sender<LogLine>,
            _: watch::Receiver<bool>,
        ) -> BoxFuture<'a, Outcome> {
            Box::pin(async move {
                let url = format!("{}/nextflow", job.progress.as_ref().unwrap().base_url);
                let client = reqwest::Client::new();
                for (event, task) in [
                    ("process_submitted", 1),
                    ("process_submitted", 2),
                    ("process_completed", 1),
                ] {
                    let body = format!(
                        r#"{{"event":"{event}","trace":{{"task_id":{task},"name":"t","status":"X"}}}}"#
                    );
                    client.post(&url).body(body).send().await.unwrap();
                }
                Outcome::Succeeded
            })
        }
    }

    #[tokio::test]
    async fn engine_progress_reaches_the_source_before_the_run_finishes() {
        let source = Arc::new(FakeSource::default());
        source.jobs.lock().unwrap().push(job("Nextflow"));
        let receiver = Arc::new(
            ProgressReceiver::start(HostAccess {
                host: "127.0.0.1".into(),
                run_args: vec![],
            })
            .await
            .unwrap(),
        );
        let worker = Worker::new(source.clone(), vec![Arc::new(WeblogExecutor)], fast())
            .with_progress_receiver(receiver);
        worker.run_next().await.unwrap();

        let progress = source.progress.lock().unwrap();
        let last = progress.last().expect("progress was reported");
        assert_eq!(last.0, 1);
        assert_eq!((last.1.total, last.1.done), (2, 1));
        assert_eq!(*source.finished.lock().unwrap(), [(1, Outcome::Succeeded)]);
    }

    /// A Nextflow-like engine that prints a line, waits `delay` and then reports its first event
    /// (never when `report` is false); it ends when cancelled or after `run_for`.
    struct SilentExecutor {
        delay: Duration,
        report: bool,
        run_for: Duration,
    }

    impl Executor for SilentExecutor {
        fn engine(&self) -> Engine {
            Engine::Nextflow
        }
        fn run<'a>(
            &'a self,
            job: &'a Job,
            log: mpsc::Sender<LogLine>,
            mut cancel: watch::Receiver<bool>,
        ) -> BoxFuture<'a, Outcome> {
            Box::pin(async move {
                let _ = log
                    .send(LogLine {
                        stream: LogStream::Stdout,
                        line: "engine started".into(),
                    })
                    .await;
                tokio::time::sleep(self.delay).await;
                if self.report {
                    let url = format!("{}/nextflow", job.progress.as_ref().unwrap().base_url);
                    let body = r#"{"event":"started"}"#;
                    reqwest::Client::new()
                        .post(&url)
                        .body(body)
                        .send()
                        .await
                        .unwrap();
                }
                tokio::select! {
                    _ = cancel.wait_for(|cancelled| *cancelled) => Outcome::Cancelled,
                    _ = tokio::time::sleep(self.run_for) => Outcome::Succeeded,
                }
            })
        }
    }

    /// Runs one Nextflow job on a [`SilentExecutor`] and returns the source with its result.
    ///
    /// # Arguments
    /// * `executor` - the engine behaviour
    /// * `warning_after` - see `WorkerOptions::progress_warning_after`
    /// * `timeout` - see `WorkerOptions::progress_timeout`
    async fn run_silent(
        executor: SilentExecutor,
        warning_after: Duration,
        timeout: Option<Duration>,
    ) -> Arc<FakeSource> {
        let source = Arc::new(FakeSource::default());
        source.jobs.lock().unwrap().push(job("Nextflow"));
        let receiver = Arc::new(
            ProgressReceiver::start(HostAccess {
                host: "127.0.0.1".into(),
                run_args: vec![],
            })
            .await
            .unwrap(),
        );
        let options = WorkerOptions {
            progress_warning_after: warning_after,
            progress_timeout: timeout,
            ..fast()
        };
        Worker::new(source.clone(), vec![Arc::new(executor)], options)
            .with_progress_receiver(receiver)
            .run_next()
            .await
            .unwrap();
        source
    }

    /// Whether the run log of the source has a line containing `text`.
    fn logged(source: &FakeSource, text: &str) -> bool {
        source
            .logs
            .lock()
            .unwrap()
            .iter()
            .any(|(_, _, lines)| lines.iter().any(|line| line.contains(text)))
    }

    /// An engine that never reports is warned about, then failed (not cancelled).
    #[tokio::test]
    async fn silent_engine_is_failed_after_the_timeout() {
        let source = run_silent(
            SilentExecutor {
                delay: Duration::ZERO,
                report: false,
                run_for: Duration::from_secs(30),
            },
            Duration::from_millis(100),
            Some(Duration::from_millis(300)),
        )
        .await;
        let finished = source.finished.lock().unwrap();
        assert!(matches!(
            &finished[0].1,
            Outcome::Failed { reason: Some(FailureReason::StartFailed), error: Some(e), .. }
                if e.contains("no progress")
        ));
        assert!(logged(&source, "firewall"));
        assert!(logged(&source, "stopping the run"));
    }

    /// The first event stops the watch: a long run after it is never failed.
    #[tokio::test]
    async fn reporting_engine_is_never_failed() {
        let source = run_silent(
            SilentExecutor {
                delay: Duration::from_millis(50),
                report: true,
                run_for: Duration::from_millis(600),
            },
            Duration::from_millis(100),
            Some(Duration::from_millis(300)),
        )
        .await;
        assert_eq!(*source.finished.lock().unwrap(), [(1, Outcome::Succeeded)]);
        assert!(!logged(&source, "firewall"));
    }

    /// Without a timeout the run is warned about but left running.
    #[tokio::test]
    async fn no_timeout_only_warns() {
        let source = run_silent(
            SilentExecutor {
                delay: Duration::ZERO,
                report: false,
                run_for: Duration::from_millis(500),
            },
            Duration::from_millis(100),
            None,
        )
        .await;
        assert_eq!(*source.finished.lock().unwrap(), [(1, Outcome::Succeeded)]);
        assert!(logged(&source, "firewall"));
    }

    /// A job whose workflow brings one image build.
    fn job_with_image() -> Job {
        use macworp_core::workflow_configuration::images::ImageBuild;
        let mut job = job("Nextflow");
        job.configuration
            .images_mut()
            .push(ImageBuild::new("Dockerfile".into(), "team/tool:1".into()));
        job
    }

    /// Without an image builder a run with images fails to start, the engine does not run.
    #[tokio::test]
    async fn images_without_a_builder_fail_the_start() {
        let source = Arc::new(FakeSource::default());
        source.jobs.lock().unwrap().push(job_with_image());
        worker(&source, false).run_next().await.unwrap();
        let finished = source.finished.lock().unwrap();
        assert!(matches!(
            &finished[0].1,
            Outcome::Failed { exit_code: None, error: Some(e), reason: Some(FailureReason::StartFailed) }
                if e.contains("cannot build")
        ));
        // The fake executor would have logged lines
        assert!(source.logs.lock().unwrap().is_empty());
    }

    /// A builder whose build fails ends the run before the engine starts; a job without images
    /// never touches the builder.
    #[cfg(unix)]
    #[tokio::test]
    async fn failed_build_skips_the_engine() {
        use crate::container_runtime::RuntimeKind;
        let source = Arc::new(FakeSource::default());
        source.jobs.lock().unwrap().push(job_with_image());
        let builder = ImageBuilder::new("false".into(), RuntimeKind::Podman);
        worker(&source, false)
            .with_image_builders(vec![builder.clone()])
            .run_next()
            .await
            .unwrap();
        assert!(matches!(
            &source.finished.lock().unwrap()[0].1,
            Outcome::Failed {
                reason: Some(FailureReason::StartFailed),
                ..
            }
        ));

        // No images: the engine runs although the builder would fail
        source.finished.lock().unwrap().clear();
        source.jobs.lock().unwrap().push(job("Nextflow"));
        worker(&source, false)
            .with_image_builders(vec![builder])
            .run_next()
            .await
            .unwrap();
        assert!(matches!(
            &source.finished.lock().unwrap()[0].1,
            Outcome::Failed {
                reason: Some(FailureReason::ExitCode),
                ..
            }
        ));
    }

    #[tokio::test]
    async fn empty_queue() {
        let source = Arc::new(FakeSource::default());
        assert!(!worker(&source, false).run_next().await.unwrap());
    }

    #[tokio::test]
    async fn failed_run_is_not_succeeded_and_logs_are_chunked_per_stream() {
        let source = Arc::new(FakeSource::default());
        source.jobs.lock().unwrap().push(job("Nextflow"));
        assert!(worker(&source, false).run_next().await.unwrap());

        assert_eq!(
            *source.finished.lock().unwrap(),
            [(
                1,
                Outcome::Failed {
                    exit_code: Some(1),
                    error: None,
                    reason: Some(FailureReason::ExitCode),
                }
            )]
        );
        let logs = source.logs.lock().unwrap();
        let lines_of = |s: LogStream| -> Vec<String> {
            logs.iter()
                .filter(|(_, st, _)| *st == s)
                .flat_map(|(_, _, l)| l.clone())
                .collect()
        };
        assert_eq!(lines_of(LogStream::Stdout), ["a", "c"]);
        assert_eq!(lines_of(LogStream::Stderr), ["b"]);
    }

    #[tokio::test]
    async fn cancel_request_reaches_executor() {
        let source = Arc::new(FakeSource::default());
        source.jobs.lock().unwrap().push(job("Nextflow"));
        *source.cancel.lock().unwrap() = true;
        tokio::time::timeout(Duration::from_secs(5), worker(&source, true).run_next())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(*source.finished.lock().unwrap(), [(1, Outcome::Cancelled)]);
    }

    #[tokio::test]
    async fn missing_executor_fails_run() {
        let source = Arc::new(FakeSource::default());
        source.jobs.lock().unwrap().push(job("Snakemake"));
        worker(&source, false).run_next().await.unwrap();
        let finished = source.finished.lock().unwrap();
        assert!(
            matches!(&finished[0].1, Outcome::Failed { error: Some(e), .. } if e.contains("no executor"))
        );
    }

    /// A transient failure while reporting the result is retried, the result is not lost.
    #[tokio::test]
    async fn finish_is_retried() {
        let source = Arc::new(FakeSource::default());
        source.jobs.lock().unwrap().push(job("Nextflow"));
        *source.finish_failures.lock().unwrap() = 2;
        let worker = Worker::new(
            source.clone(),
            vec![Arc::new(FakeExecutor { wait: false })],
            WorkerOptions {
                finish_retry_interval: Duration::from_millis(5),
                ..fast()
            },
        );
        worker.run_next().await.unwrap();
        assert_eq!(source.finished.lock().unwrap().len(), 1);
    }

    /// When every attempt fails the error is returned after exactly `finish_attempts` tries.
    #[tokio::test]
    async fn finish_gives_up_after_the_attempts() {
        let source = Arc::new(FakeSource::default());
        source.jobs.lock().unwrap().push(job("Nextflow"));
        *source.finish_failures.lock().unwrap() = 10;
        let worker = Worker::new(
            source.clone(),
            vec![Arc::new(FakeExecutor { wait: false })],
            WorkerOptions {
                finish_attempts: 3,
                finish_retry_interval: Duration::from_millis(5),
                ..fast()
            },
        );
        assert!(worker.run_next().await.is_err());
        assert_eq!(*source.finish_failures.lock().unwrap(), 7);
        assert!(source.finished.lock().unwrap().is_empty());
    }

    /// A panicking executor fails its run and the worker keeps working.
    #[tokio::test]
    async fn executor_panic_fails_the_run() {
        let source = Arc::new(FakeSource::default());
        source.jobs.lock().unwrap().push(job("Nextflow"));
        let worker = Worker::new(source.clone(), vec![Arc::new(PanickingExecutor)], fast());
        assert!(worker.run_next().await.unwrap());
        let finished = source.finished.lock().unwrap();
        assert!(matches!(
            &finished[0].1,
            Outcome::Failed { error: Some(e), reason: Some(FailureReason::StartFailed), .. } if e.contains("crashed")
        ));
    }

    /// A run whose project folder the VM does not see fails to start with the fix, one inside reaches the engine.
    #[tokio::test]
    async fn run_outside_the_shared_folders_fails_to_start() {
        let shared = Arc::new(
            SharedFolders::from_docker_desktop(r#"{"FilesharingDirectories":["/shared"]}"#)
                .unwrap(),
        );
        for (project, runs) in [("/shared/p", true), ("/opt/p", false)] {
            let source = Arc::new(FakeSource::default());
            let mut job = job("Nextflow");
            job.project_dir = project.into();
            source.jobs.lock().unwrap().push(job);
            worker(&source, false)
                .with_shared_folders(Some(shared.clone()))
                .run_next()
                .await
                .unwrap();
            let finished = source.finished.lock().unwrap();
            if runs {
                // The fake engine ran (and failed on its own terms)
                assert!(matches!(
                    &finished[0].1,
                    Outcome::Failed {
                        reason: Some(FailureReason::ExitCode),
                        ..
                    }
                ));
            } else {
                assert!(matches!(
                    &finished[0].1,
                    Outcome::Failed { reason: Some(FailureReason::StartFailed), error: Some(e), .. }
                        if e.contains("/opt/p") && e.contains("File sharing")
                ));
            }
        }
    }
}
