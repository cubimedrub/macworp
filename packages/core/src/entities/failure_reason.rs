#[cfg(feature = "database")]
use sea_orm::sea_query::StringLen;
use serde::{Deserialize, Serialize};
use strum::{Display, EnumString};

/// Why a run ended as `Failed`, stored as a snake_case string (e.g. `timed_out`).
///
/// Only set together with [`RunStatus::Failed`](super::RunStatus::Failed); the free-text
/// `error` of the run adds the details (exit code, signal number, limits).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Display, EnumString)]
#[strum(serialize_all = "snake_case")]
#[serde(rename_all = "snake_case")]
#[cfg_attr(
    feature = "database",
    derive(sea_orm::DeriveActiveEnum, sea_orm::EnumIter)
)]
#[cfg_attr(
    feature = "database",
    sea_orm(rs_type = "String", db_type = "String(StringLen::N(24))")
)]
pub enum FailureReason {
    /// The engine exited with a non-zero exit code
    #[cfg_attr(feature = "database", sea_orm(string_value = "exit_code"))]
    ExitCode,
    /// The engine process was killed by a signal (e.g. out of memory)
    #[cfg_attr(feature = "database", sea_orm(string_value = "signal"))]
    Signal,
    /// The run exceeded the configured running time limit
    #[cfg_attr(feature = "database", sea_orm(string_value = "timed_out"))]
    TimedOut,
    /// No worker picked the run up within the configured queue time limit
    #[cfg_attr(feature = "database", sea_orm(string_value = "queued_timeout"))]
    QueuedTimeout,
    /// The worker stopped reporting and the attempts are used up, or the application stopped
    #[cfg_attr(feature = "database", sea_orm(string_value = "worker_lost"))]
    WorkerLost,
    /// The engine could not be started (missing project, executor or image, spawn error)
    #[cfg_attr(feature = "database", sea_orm(string_value = "start_failed"))]
    StartFailed,
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;

    /// Every variant survives the string and JSON round trip with its snake_case name.
    #[test]
    fn round_trip() {
        for reason in [
            FailureReason::ExitCode,
            FailureReason::Signal,
            FailureReason::TimedOut,
            FailureReason::QueuedTimeout,
            FailureReason::WorkerLost,
            FailureReason::StartFailed,
        ] {
            assert_eq!(
                FailureReason::from_str(&reason.to_string()).unwrap(),
                reason
            );
            let json = serde_json::to_string(&reason).unwrap();
            assert_eq!(json, format!("\"{reason}\""));
            assert_eq!(
                serde_json::from_str::<FailureReason>(&json).unwrap(),
                reason
            );
        }
    }
}
