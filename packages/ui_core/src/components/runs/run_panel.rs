use std::time::Duration;

use crate::timer::sleep;

use dioxus::prelude::*;
use macworp_core::{
    entities::{LogStream, RunStatus},
    responses::{RunLogChunk, RunResponse},
};
use thiserror::Error;

use super::progress::progress_view;
use crate::{client::Client, configuration::Configuration, storage::IsStorage};

/// Poll interval while a run is queued or running
const ACTIVE_POLL: Duration = Duration::from_secs(1);
/// Poll interval when nothing is active, only to notice runs started elsewhere
const IDLE_POLL: Duration = Duration::from_secs(5);
/// Log chunks kept in the view, older ones are dropped (chunks are ~1 s of output each)
const MAX_VISIBLE_CHUNKS: usize = 1000;

#[derive(Debug, Error)]
enum Error<S: IsStorage> {
    #[error("Not logged in")]
    NotLoggedIn,
    #[error("{0}")]
    Storage(S::Error),
    #[error("{0}")]
    Client(crate::client::Error),
}

#[derive(Clone, PartialEq, Props)]
pub struct RunPanelProps {
    pub project_id: i64,
    /// Changes whenever a run was started, the panel then reloads at once and shows the newest run
    pub refresh: ReadSignal<u32>,
}

/// Runs of a project: list with cancel buttons, progress bar and the output of the selected run.
///
/// Polls the backend: the run list and, for the selected run, the log chunks after the last one
/// received (cursor `seq`). Polling stops mattering once everything is finished (slow interval).
#[component]
pub fn RunPanel<S: IsStorage + 'static>(props: RunPanelProps) -> Element {
    let storage = use_context::<Signal<S>>();
    let config = use_context::<Signal<Configuration>>();

    let mut runs: Signal<Vec<RunResponse>> = use_signal(Vec::new);
    let mut selected: Signal<Option<i64>> = use_signal(|| None);
    let mut chunks: Signal<Vec<RunLogChunk>> = use_signal(Vec::new);
    let mut hidden_chunks = use_signal(|| 0_usize);
    let mut cursor: Signal<Option<i64>> = use_signal(|| None);
    let mut error: Signal<Option<String>> = use_signal(|| None);

    let project_id = props.project_id;

    // Restarts, and so reloads at once, when `refresh` changes
    let _poller = use_resource(move || async move {
        let refreshed = *props.refresh.read();
        let mut first = true;
        loop {
            let result: Result<(), Error<S>> = async {
                let jwt = storage
                    .read()
                    .jwt()
                    .map_err(Error::Storage)?
                    .ok_or(Error::NotLoggedIn)?;
                let config = config.read().clone();
                let client = Client::new(config.base_url().to_string(), config.timeout());

                let list = client
                    .runs()
                    .index(&jwt, project_id)
                    .await
                    .map_err(Error::Client)?;

                // A new run (refresh) or no selection yet: show the newest run
                let current = *selected.peek();
                let still_there = current.is_some_and(|id| list.iter().any(|r| r.id == id));
                let wants_newest = first && refreshed > 0 || current.is_none() || !still_there;
                if let Some(newest) = list.first().filter(|_| wants_newest) {
                    if current != Some(newest.id) {
                        selected.set(Some(newest.id));
                        chunks.set(Vec::new());
                        hidden_chunks.set(0);
                        cursor.set(None);
                    }
                }
                runs.set(list);

                let Some(id) = *selected.peek() else {
                    return Ok(());
                };
                let after = *cursor.peek();
                let logs = client
                    .runs()
                    .logs(&jwt, id, after)
                    .await
                    .map_err(Error::Client)?;
                // the user may have picked another run while waiting
                if *selected.peek() == Some(id) && *cursor.peek() == after {
                    if let Some(last) = logs.chunks.last() {
                        cursor.set(Some(last.seq));
                        let mut visible = chunks.write();
                        visible.extend(logs.chunks);
                        if visible.len() > MAX_VISIBLE_CHUNKS {
                            let excess = visible.len() - MAX_VISIBLE_CHUNKS;
                            visible.drain(..excess);
                            *hidden_chunks.write() += excess;
                        }
                    }
                    let mut list = runs.write();
                    if let Some(entry) = list.iter_mut().find(|r| r.id == id) {
                        *entry = logs.run;
                    }
                }
                Ok(())
            }
            .await;
            first = false;
            match &result {
                Ok(()) => error.set(None),
                Err(e) => error.set(Some(e.to_string())),
            }

            let active = runs.peek().iter().any(|r| !r.status.is_terminal());
            sleep(if active { ACTIVE_POLL } else { IDLE_POLL }).await;
        }
    });

    let mut cancel = use_action(move |id: i64| async move {
        let jwt = storage
            .read()
            .jwt()
            .map_err(Error::<S>::Storage)?
            .ok_or(Error::NotLoggedIn)?;
        let config = config.read().clone();
        let run = Client::new(config.base_url().to_string(), config.timeout())
            .runs()
            .cancel(&jwt, id)
            .await
            .map_err(Error::Client)?;
        let mut list = runs.write();
        if let Some(entry) = list.iter_mut().find(|r| r.id == id) {
            *entry = run;
        }
        Ok::<(), Error<S>>(())
    });

    let selected_run = selected
        .read()
        .and_then(|id| runs.read().iter().find(|r| r.id == id).cloned());

    rsx! {
        if runs.read().is_empty() && error.read().is_none() {
            // nothing started yet, keep the page quiet
        } else {
            div { class: "card mb-3",
                div { class: "card-header d-flex justify-content-between align-items-center",
                    strong { "Runs" }
                }
                if let Some(message) = error.read().as_ref() {
                    div { class: "alert alert-danger m-2", role: "alert", "{message}" }
                }
                if let Some(Err(err)) = cancel.value().as_ref().map(|r| r.as_ref().map(|_| ()).map_err(|e| e.to_string())) {
                    div { class: "alert alert-danger m-2", role: "alert", "{err}" }
                }
                ul { class: "list-group list-group-flush",
                    for run in runs.read().iter().cloned() {
                        li {
                            key: "{run.id}",
                            class: if *selected.read() == Some(run.id) { "list-group-item list-group-item-action active" } else { "list-group-item list-group-item-action" },
                            style: "cursor: pointer",
                            onclick: move |_| {
                                if *selected.peek() != Some(run.id) {
                                    selected.set(Some(run.id));
                                    chunks.set(Vec::new());
                                    hidden_chunks.set(0);
                                    cursor.set(None);
                                }
                            },
                            div { class: "d-flex justify-content-between align-items-center",
                                span {
                                    "#{run.id} "
                                    if run.workflow_name.is_empty() { "(deleted workflow)" } else { "{run.workflow_name}" }
                                    small { class: "ms-2 opacity-75", "{created_text(&run)}" }
                                }
                                span {
                                    span { class: "badge {status_class(&run.status)} me-2", "{run.status}" }
                                    if !run.status.is_terminal() {
                                        button {
                                            class: "btn btn-sm btn-outline-danger",
                                            title: "Cancel run",
                                            disabled: run.cancel_requested,
                                            onclick: move |event| {
                                                event.stop_propagation();
                                                cancel.call(run.id);
                                            },
                                            if run.cancel_requested { "Cancelling..." } else { "Cancel" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                if let Some(run) = selected_run {
                    {run_details(&run, &chunks.read(), *hidden_chunks.read())}
                }
            }
        }
    }
}

fn created_text(run: &RunResponse) -> String {
    run.created_at.format("%Y-%m-%d %H:%M:%S").to_string()
}

fn status_class(status: &RunStatus) -> &'static str {
    match status {
        RunStatus::Queued => "text-bg-secondary",
        RunStatus::Running => "text-bg-primary",
        RunStatus::Succeeded => "text-bg-success",
        RunStatus::Failed => "text-bg-danger",
        RunStatus::Cancelled => "text-bg-warning",
    }
}

fn stream_class(stream: &LogStream) -> &'static str {
    match stream {
        LogStream::Stdout => "",
        LogStream::Stderr => "text-warning",
        LogStream::Engine => "text-info",
    }
}

/// First characters of a commit hash, as git shows it
///
/// # Arguments
/// * `commit` - Full commit hash
fn short_commit(commit: &str) -> &str {
    commit.get(..7).unwrap_or(commit)
}

/// A size in bytes for people: binary units, one decimal from KiB up.
///
/// # Arguments
/// * `bytes` - Size in bytes
fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

/// Progress bar, latest detail and output of one run
fn run_details(run: &RunResponse, chunks: &[RunLogChunk], hidden_chunks: usize) -> Element {
    let view = progress_view(run);
    let last_changed = run
        .reference_fingerprint
        .as_ref()
        .and_then(|fingerprint| fingerprint.last_modified)
        .map(|time| time.format("%Y-%m-%d %H:%M UTC").to_string());
    rsx! {
        div { class: "card-body",
            div { class: "progress mb-1", style: "height: 1.5rem", role: "progressbar",
                div {
                    class: "progress-bar {view.class}",
                    style: "width: {view.percent}%",
                    "{view.label}"
                }
            }
            if run.status == RunStatus::Running && run.tasks_total > 0 {
                small { class: "text-muted d-block",
                    "The progress may decrease during the run: the number of tasks is not known in advance."
                }
            }
            if let Some(message) = run.progress_message.as_ref().filter(|_| !run.status.is_terminal()) {
                small { class: "text-muted d-block text-truncate", "{message}" }
            }
            if let Some(error) = run.error.as_ref() {
                div { class: "alert alert-danger mt-2 mb-0", role: "alert", "{error}" }
            }
            if let Some(provenance) = run.provenance.as_ref() {
                small { class: "text-muted d-block mt-2",
                    "Workflow "
                    code { "{provenance.url}" }
                    if let Some(requested_ref) = provenance.requested_ref.as_ref() {
                        " at "
                        code { "{requested_ref}" }
                    }
                    " (commit "
                    code { title: "{provenance.commit}", "{short_commit(&provenance.commit)}" }
                    "), script "
                    code { "{provenance.script.display()}" }
                }
            }
            if let Some(fingerprint) = run.reference_fingerprint.as_ref() {
                small { class: "text-muted d-block",
                    "Reference data at start: "
                    code { "{fingerprint.file_count}" }
                    " files, "
                    code { "{format_bytes(fingerprint.total_bytes)}" }
                    if let Some(last_changed) = last_changed.as_ref() {
                        ", last changed "
                        code { "{last_changed}" }
                    }
                }
            }
            // column-reverse keeps the view at the end while output grows
            div {
                class: "mt-2 bg-dark rounded",
                style: "max-height: 400px; overflow-y: auto; display: flex; flex-direction: column-reverse",
                pre { class: "text-light m-0 p-2", style: "white-space: pre-wrap; word-break: break-all",
                    if hidden_chunks > 0 {
                        div { class: "text-muted", "[{hidden_chunks} older output blocks hidden]" }
                    }
                    if chunks.is_empty() {
                        div { class: "text-muted", "No output yet" }
                    }
                    for chunk in chunks.iter() {
                        div { key: "{chunk.seq}", class: stream_class(&chunk.stream), "{chunk.content}" }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::format_bytes;

    /// Sizes below 1 KiB stay exact, larger ones use binary units with one decimal.
    #[test]
    fn format_bytes_uses_binary_units() {
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(1023), "1023 B");
        assert_eq!(format_bytes(1024), "1.0 KiB");
        assert_eq!(format_bytes(1536), "1.5 KiB");
        assert_eq!(format_bytes(5 * 1024 * 1024 * 1024), "5.0 GiB");
    }
}
