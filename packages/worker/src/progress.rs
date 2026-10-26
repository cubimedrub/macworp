use macworp_core::workflow_configuration::engine::Engine;
use serde_json::Value;

/// Progress of a run as far as the engine reported it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ProgressUpdate {
    /// Tasks announced so far. Grows during a run, the engines do not know it up front.
    pub total: i32,
    /// Tasks finished
    pub done: i32,
    /// Latest detail, e.g. the task that changed state
    pub message: Option<String>,
    /// Error report of the engine, shown in the log
    pub error: Option<String>,
}

/// Turns the monitoring events of an engine into [`ProgressUpdate`]s.
///
/// Nextflow (`-with-weblog`) reports one event per task state change, so the tracker counts them.
/// Snakemake (`--wms-monitor`) reports absolute `done`/`total` numbers.
#[derive(Debug, Clone)]
pub struct ProgressTracker {
    engine: Engine,
    total: i32,
    done: i32,
}

impl ProgressTracker {
    pub fn new(engine: Engine) -> Self {
        Self {
            engine,
            total: 0,
            done: 0,
        }
    }

    /// The numbers so far, without a message
    pub fn current(&self) -> ProgressUpdate {
        ProgressUpdate {
            total: self.total,
            done: self.done,
            ..Default::default()
        }
    }

    /// Handles one event given as JSON text. `None` if it is not JSON or carries nothing of interest.
    pub fn handle(&mut self, payload: &str) -> Option<ProgressUpdate> {
        let event: Value = serde_json::from_str(payload).ok()?;
        match self.engine {
            Engine::Nextflow => self.nextflow(&event),
            Engine::Snakemake => self.snakemake(&event),
            _ => None,
        }
    }

    fn update(&mut self, message: Option<String>, error: Option<String>) -> ProgressUpdate {
        // cached tasks may be reported as completed without a submission, never exceed 100 %
        self.total = self.total.max(self.done);
        ProgressUpdate {
            total: self.total,
            done: self.done,
            message,
            error,
        }
    }

    fn nextflow(&mut self, event: &Value) -> Option<ProgressUpdate> {
        let kind = event.get("event")?.as_str()?;
        if let Some(trace) = event.get("trace") {
            match kind {
                "process_submitted" => self.total += 1,
                "process_completed" => self.done += 1,
                _ => {}
            }
            let id = trace.get("task_id").map(display_value).unwrap_or_default();
            let name = trace.get("name").and_then(Value::as_str).unwrap_or("?");
            let status = trace.get("status").and_then(Value::as_str).unwrap_or("?");
            return Some(self.update(Some(format!("Task {id}: {name} - {status}")), None));
        }
        let error = event
            .pointer("/metadata/workflow/errorReport")
            .and_then(Value::as_str)?;
        Some(self.update(None, Some(error.to_string())))
    }

    fn snakemake(&mut self, event: &Value) -> Option<ProgressUpdate> {
        let text = |key: &str| event.get(key).and_then(Value::as_str);
        match event.get("level")?.as_str()? {
            "progress" => {
                // absolute numbers
                self.total = event.get("total")?.as_i64()?.clamp(0, i32::MAX as i64) as i32;
                self.done = event.get("done")?.as_i64()?.clamp(0, i32::MAX as i64) as i32;
                Some(self.update(None, None))
            }
            "job_info" => {
                let id = event.get("jobid").map(display_value).unwrap_or_default();
                let name = text("name").unwrap_or("?");
                let message = match text("msg") {
                    Some(msg) => format!("Task {id}: {name} - {msg}"),
                    None => format!("Task {id}: {name}"),
                };
                Some(self.update(Some(message), None))
            }
            "run_info" | "info" => {
                let message = text("msg")?.to_string();
                Some(self.update(Some(message), None))
            }
            "error" => {
                let error = text("msg")?.to_string();
                Some(self.update(None, Some(error)))
            }
            _ => None,
        }
    }
}

fn display_value(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nf(event: &str, task: i32, status: &str) -> String {
        format!(
            r#"{{"runName":"x","event":"{event}","trace":{{"task_id":{task},"name":"foo ({task})","status":"{status}"}}}}"#
        )
    }

    #[test]
    fn nextflow_counts_task_events() {
        let mut tracker = ProgressTracker::new(Engine::Nextflow);
        let update = tracker
            .handle(&nf("process_submitted", 1, "SUBMITTED"))
            .unwrap();
        assert_eq!((update.total, update.done), (1, 0));
        assert_eq!(
            update.message.as_deref(),
            Some("Task 1: foo (1) - SUBMITTED")
        );
        tracker
            .handle(&nf("process_submitted", 2, "SUBMITTED"))
            .unwrap();
        // started does not change the counts
        let update = tracker
            .handle(&nf("process_started", 1, "RUNNING"))
            .unwrap();
        assert_eq!((update.total, update.done), (2, 0));
        let update = tracker
            .handle(&nf("process_completed", 1, "COMPLETED"))
            .unwrap();
        assert_eq!((update.total, update.done), (2, 1));
    }

    #[test]
    fn nextflow_cached_task_never_exceeds_total() {
        let mut tracker = ProgressTracker::new(Engine::Nextflow);
        let update = tracker
            .handle(&nf("process_completed", 1, "CACHED"))
            .unwrap();
        assert_eq!((update.total, update.done), (1, 1));
    }

    #[test]
    fn nextflow_error_report() {
        let mut tracker = ProgressTracker::new(Engine::Nextflow);
        let update = tracker
            .handle(r#"{"event":"error","metadata":{"workflow":{"errorReport":"boom"}}}"#)
            .unwrap();
        assert_eq!(update.error.as_deref(), Some("boom"));
        // workflow metadata without an error is not interesting
        assert!(
            tracker
                .handle(r#"{"event":"started","metadata":{"workflow":{"errorReport":null}}}"#)
                .is_none()
        );
    }

    #[test]
    fn snakemake_progress_is_absolute_and_may_decrease() {
        let mut tracker = ProgressTracker::new(Engine::Snakemake);
        let update = tracker
            .handle(r#"{"level":"progress","done":1,"total":5}"#)
            .unwrap();
        assert_eq!((update.total, update.done), (5, 1));
        let update = tracker
            .handle(r#"{"level":"progress","done":2,"total":4}"#)
            .unwrap();
        assert_eq!((update.total, update.done), (4, 2));
    }

    #[test]
    fn snakemake_messages_and_errors() {
        let mut tracker = ProgressTracker::new(Engine::Snakemake);
        let update = tracker
            .handle(r#"{"level":"job_info","jobid":3,"name":"align","msg":null}"#)
            .unwrap();
        assert_eq!(update.message.as_deref(), Some("Task 3: align"));
        let update = tracker
            .handle(r#"{"level":"job_info","jobid":3,"name":"align","msg":"sample a"}"#)
            .unwrap();
        assert_eq!(update.message.as_deref(), Some("Task 3: align - sample a"));
        let update = tracker
            .handle(r#"{"level":"run_info","msg":"Job stats"}"#)
            .unwrap();
        assert_eq!(update.message.as_deref(), Some("Job stats"));
        let update = tracker.handle(r#"{"level":"error","msg":"bad"}"#).unwrap();
        assert_eq!(update.error.as_deref(), Some("bad"));
        // unknown level and non-JSON are ignored
        assert!(tracker.handle(r#"{"level":"debug","msg":"x"}"#).is_none());
        assert!(tracker.handle("not json").is_none());
    }

    #[test]
    fn other_engines_report_nothing() {
        let mut tracker = ProgressTracker::new(Engine::Container);
        assert!(
            tracker
                .handle(r#"{"level":"progress","done":1,"total":2}"#)
                .is_none()
        );
    }
}
