use std::{collections::HashMap, sync::Arc, time::Duration};

use macworp_core::{entities::LogStream, workflow_configuration::engine::Engine};
use tokio::{
    sync::{mpsc, watch},
    time::{MissedTickBehavior, interval},
};

use crate::{
    executors::{Executor, Job, LogLine, Outcome},
    job_source::{JobSource, SourceError},
};

#[derive(Debug, Clone)]
pub struct WorkerOptions {
    /// Log lines are sent to the source in chunks, at least this often ...
    pub log_flush_interval: Duration,
    /// ... or as soon as one stream has buffered this many bytes
    pub log_flush_bytes: usize,
    /// How often the source is asked whether the run should be cancelled
    pub cancel_poll_interval: Duration,
    /// Wait between claims when the queue is empty
    pub idle_interval: Duration,
}

impl Default for WorkerOptions {
    fn default() -> Self {
        Self {
            log_flush_interval: Duration::from_secs(1),
            log_flush_bytes: 64 * 1024,
            cancel_poll_interval: Duration::from_secs(2),
            idle_interval: Duration::from_secs(2),
        }
    }
}

/// Claims jobs from a [`JobSource`] and runs them one at a time with the executor of their engine.
pub struct Worker {
    source: Arc<dyn JobSource>,
    executors: HashMap<Engine, Arc<dyn Executor>>,
    options: WorkerOptions,
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
        }
    }

    /// Claims and runs one job. `false` if there was none.
    pub async fn run_next(&self) -> Result<bool, SourceError> {
        let Some(job) = self.source.claim().await? else {
            return Ok(false);
        };
        let run_id = job.run_id;
        let outcome = self.execute(job).await;
        tracing::info!("Run {run_id} ended: {outcome:?}");
        self.source.finish(run_id, outcome).await?;
        Ok(true)
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

    async fn execute(&self, job: Job) -> Outcome {
        let engine = job.configuration.engine().clone();
        let Some(executor) = self.executors.get(&engine) else {
            return Outcome::Failed {
                exit_code: None,
                error: Some(format!("This worker has no executor for {engine}")),
            };
        };

        let (cancel_tx, cancel_rx) = watch::channel(false);
        let (log_tx, log_rx) = mpsc::channel(256);

        let logs = tokio::spawn(forward_logs(
            self.source.clone(),
            job.run_id,
            log_rx,
            self.options.clone(),
        ));
        let cancel_poll = tokio::spawn(poll_cancel(
            self.source.clone(),
            job.run_id,
            cancel_tx,
            self.options.cancel_poll_interval,
        ));

        let outcome = executor.run(&job, log_tx, cancel_rx).await;

        cancel_poll.abort();
        // The sender is gone once the executor returned, this flushes the rest
        let _ = logs.await;
        outcome
    }
}

async fn poll_cancel(
    source: Arc<dyn JobSource>,
    run_id: i64,
    cancel: watch::Sender<bool>,
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

/// Buffers lines per stream and sends them as chunks.
async fn forward_logs(
    source: Arc<dyn JobSource>,
    run_id: i64,
    mut lines: mpsc::Receiver<LogLine>,
    options: WorkerOptions,
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
    use crate::container_runtime::BoxFuture;

    #[derive(Default)]
    struct FakeSource {
        jobs: Mutex<Vec<Job>>,
        logs: Mutex<Vec<(i64, LogStream, Vec<String>)>>,
        finished: Mutex<Vec<(i64, Outcome)>>,
        cancel: Mutex<bool>,
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
        fn cancel_requested(&self, _: i64) -> BoxFuture<'_, Result<bool, SourceError>> {
            Box::pin(async { Ok(*self.cancel.lock().unwrap()) })
        }
        fn finish(&self, run_id: i64, outcome: Outcome) -> BoxFuture<'_, Result<(), SourceError>> {
            Box::pin(async move {
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
                    }
                }
            })
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
            configuration,
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
                    error: None
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
}
