use std::{path::PathBuf, process::Stdio, time::Duration};

use macworp_core::entities::LogStream;
use tokio::{
    io::{AsyncBufReadExt, AsyncRead, BufReader},
    process::Command,
    sync::{mpsc, watch},
};

use super::{LogLine, Outcome};

/// Grace period for a cancelled container before it is killed
const STOP_TIMEOUT_SECONDS: u32 = 10;

/// A process to start, normally `<runtime> run ...`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchSpec {
    pub program: PathBuf,
    pub args: Vec<String>,
    /// Name of the container started by the process. On cancel the container is stopped through
    /// the runtime, killing only the client process would leave it running.
    pub container_name: Option<String>,
}

/// Starts the process, forwards its output line by line and stops it when `cancel` turns `true`.
pub async fn run_process(
    spec: &LaunchSpec,
    logs: mpsc::Sender<LogLine>,
    mut cancel: watch::Receiver<bool>,
) -> Outcome {
    let mut child = match Command::new(&spec.program)
        .args(&spec.args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
    {
        Ok(child) => child,
        Err(e) => {
            return Outcome::Failed {
                exit_code: None,
                error: Some(format!("Cannot start {}: {e}", spec.program.display())),
            };
        }
    };

    let stdout = tokio::spawn(forward(
        child.stdout.take().unwrap(),
        LogStream::Stdout,
        logs.clone(),
    ));
    let stderr = tokio::spawn(forward(
        child.stderr.take().unwrap(),
        LogStream::Stderr,
        logs,
    ));

    let outcome = tokio::select! {
        status = child.wait() => match status {
            Ok(status) if status.success() => Outcome::Succeeded,
            Ok(status) => Outcome::Failed { exit_code: status.code(), error: None },
            Err(e) => Outcome::Failed { exit_code: None, error: Some(format!("Waiting for process failed: {e}")) },
        },
        _ = cancelled(&mut cancel) => {
            stop(spec, &mut child).await;
            Outcome::Cancelled
        }
    };

    // Drain remaining output before reporting the end
    let _ = stdout.await;
    let _ = stderr.await;
    outcome
}

async fn cancelled(cancel: &mut watch::Receiver<bool>) {
    // Err: sender dropped, nobody can cancel anymore
    if cancel.wait_for(|cancelled| *cancelled).await.is_err() {
        std::future::pending::<()>().await;
    }
}

async fn stop(spec: &LaunchSpec, child: &mut tokio::process::Child) {
    if let Some(name) = &spec.container_name {
        let stopped = Command::new(&spec.program)
            .args(["stop", "-t", &STOP_TIMEOUT_SECONDS.to_string(), name])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await;
        if stopped.is_ok_and(|s| s.success())
            && tokio::time::timeout(Duration::from_secs(5), child.wait())
                .await
                .is_ok()
        {
            return;
        }
    }
    let _ = child.kill().await;
}

async fn forward(reader: impl AsyncRead + Unpin, stream: LogStream, logs: mpsc::Sender<LogLine>) {
    let mut lines = BufReader::new(reader).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        if logs.send(LogLine { stream, line }).await.is_err() {
            break;
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    fn sh(script: &str) -> LaunchSpec {
        LaunchSpec {
            program: "sh".into(),
            args: vec!["-c".into(), script.into()],
            container_name: None,
        }
    }

    async fn run(spec: LaunchSpec, cancel: watch::Receiver<bool>) -> (Outcome, Vec<LogLine>) {
        let (tx, mut rx) = mpsc::channel(64);
        let outcome = run_process(&spec, tx, cancel).await;
        let mut lines = Vec::new();
        while let Some(line) = rx.recv().await {
            lines.push(line);
        }
        (outcome, lines)
    }

    #[tokio::test]
    async fn success_forwards_both_streams() {
        let (_tx, cancel) = watch::channel(false);
        let (outcome, lines) = run(sh("echo out; echo err >&2"), cancel).await;
        assert_eq!(outcome, Outcome::Succeeded);
        assert!(lines.contains(&LogLine {
            stream: LogStream::Stdout,
            line: "out".into()
        }));
        assert!(lines.contains(&LogLine {
            stream: LogStream::Stderr,
            line: "err".into()
        }));
    }

    #[tokio::test]
    async fn non_zero_exit_is_failed() {
        let (_tx, cancel) = watch::channel(false);
        let (outcome, _) = run(sh("exit 3"), cancel).await;
        assert_eq!(
            outcome,
            Outcome::Failed {
                exit_code: Some(3),
                error: None
            }
        );
    }

    #[tokio::test]
    async fn missing_binary_is_failed() {
        let (_tx, cancel) = watch::channel(false);
        let spec = LaunchSpec {
            program: "/nonexistent/macworp-test".into(),
            args: vec![],
            container_name: None,
        };
        let (outcome, _) = run(spec, cancel).await;
        assert!(matches!(
            outcome,
            Outcome::Failed {
                exit_code: None,
                error: Some(_)
            }
        ));
    }

    #[tokio::test]
    async fn cancel_kills_process() {
        let (tx, cancel) = watch::channel(false);
        let task = tokio::spawn(run(sh("sleep 30"), cancel));
        tokio::time::sleep(Duration::from_millis(200)).await;
        tx.send(true).unwrap();
        let (outcome, _) = tokio::time::timeout(Duration::from_secs(5), task)
            .await
            .expect("cancel did not stop the process")
            .unwrap();
        assert_eq!(outcome, Outcome::Cancelled);
    }
}
