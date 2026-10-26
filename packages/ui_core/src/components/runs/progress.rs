use macworp_core::{entities::RunStatus, responses::RunResponse};

/// What the progress bar of a run shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProgressView {
    /// Filled share of the bar, 0 to 100
    pub percent: u32,
    /// Text on the bar
    pub label: String,
    /// Bootstrap classes of the bar
    pub class: &'static str,
    /// The engine reports no task numbers (yet), the bar is full and animated instead
    pub indeterminate: bool,
}

/// Derives the progress bar from the run's status and the task numbers its engine reported.
///
/// The total grows while the run discovers tasks, so the percentage can drop during a run.
pub fn progress_view(run: &RunResponse) -> ProgressView {
    let (total, done) = (run.tasks_total.max(0) as i64, run.tasks_done.max(0) as i64);
    let percent = if total == 0 {
        0
    } else {
        (done * 100 / total).clamp(0, 100) as u32
    };
    let tasks = format!("{done}/{total} tasks");
    let done_text = |outcome: &str| {
        if total == 0 {
            outcome.to_string()
        } else {
            format!("{tasks} - {outcome}")
        }
    };
    match run.status {
        RunStatus::Queued => ProgressView {
            percent: 0,
            label: "Queued".into(),
            class: "bg-secondary",
            indeterminate: false,
        },
        RunStatus::Running if total == 0 => ProgressView {
            percent: 100,
            label: "Running, waiting for tasks...".into(),
            class: "progress-bar-striped progress-bar-animated bg-primary",
            indeterminate: true,
        },
        RunStatus::Running => ProgressView {
            percent,
            label: format!("{tasks} ({percent} %)"),
            class: "progress-bar-striped progress-bar-animated bg-primary",
            indeterminate: false,
        },
        // a run that succeeded is complete, whatever the engine counted
        RunStatus::Succeeded => ProgressView {
            percent: 100,
            label: done_text("done"),
            class: "bg-success",
            indeterminate: false,
        },
        RunStatus::Failed => ProgressView {
            percent,
            label: done_text("failed"),
            class: "bg-danger",
            indeterminate: false,
        },
        RunStatus::Cancelled => ProgressView {
            percent,
            label: done_text("cancelled"),
            class: "bg-secondary",
            indeterminate: false,
        },
    }
}

#[cfg(test)]
mod tests {
    use chrono::DateTime;

    use super::*;

    fn run(status: RunStatus, total: i32, done: i32) -> RunResponse {
        RunResponse {
            id: 1,
            project_id: 1,
            workflow_id: None,
            workflow_name: String::new(),
            status,
            exit_code: None,
            error: None,
            tasks_total: total,
            tasks_done: done,
            progress_message: None,
            comment: String::new(),
            cancel_requested: false,
            locked: false,
            created_at: DateTime::UNIX_EPOCH,
            started_at: None,
            finished_at: None,
        }
    }

    #[test]
    fn running_without_numbers_is_indeterminate() {
        let view = progress_view(&run(RunStatus::Running, 0, 0));
        assert!(view.indeterminate);
        assert_eq!(view.percent, 100);
    }

    #[test]
    fn running_shows_share_of_tasks() {
        let view = progress_view(&run(RunStatus::Running, 8, 2));
        assert_eq!(view.percent, 25);
        assert_eq!(view.label, "2/8 tasks (25 %)");
        assert!(!view.indeterminate);
    }

    #[test]
    fn succeeded_is_full_even_if_counts_disagree() {
        let view = progress_view(&run(RunStatus::Succeeded, 5, 3));
        assert_eq!(view.percent, 100);
        assert_eq!(view.class, "bg-success");
        assert_eq!(
            progress_view(&run(RunStatus::Succeeded, 0, 0)).label,
            "done"
        );
    }

    #[test]
    fn failed_keeps_reached_share() {
        let view = progress_view(&run(RunStatus::Failed, 4, 1));
        assert_eq!(view.percent, 25);
        assert_eq!(view.class, "bg-danger");
        assert_eq!(view.label, "1/4 tasks - failed");
    }

    #[test]
    fn bad_numbers_are_clamped() {
        assert_eq!(progress_view(&run(RunStatus::Running, 2, 9)).percent, 100);
        // negative numbers count as none reported
        assert!(progress_view(&run(RunStatus::Running, -1, -1)).indeterminate);
    }
}
