#[cfg(feature = "database")]
use sea_orm::sea_query::StringLen;
use serde::{Deserialize, Serialize};
use strum::{Display, EnumString};

/// Lifecycle status of a run, stored as a snake_case string (e.g. `succeeded`).
///
/// A run moves `Queued` -> `Running` -> one of the terminal states
/// (`Succeeded`, `Failed`, `Cancelled`), see [`RunStatus::is_terminal`].
/// A run that did not finish successfully is always `Failed`, never `Succeeded`.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Display, EnumString,
)]
#[strum(serialize_all = "snake_case")]
#[serde(rename_all = "snake_case")]
#[cfg_attr(
    feature = "database",
    derive(sea_orm::DeriveActiveEnum, sea_orm::EnumIter)
)]
#[cfg_attr(
    feature = "database",
    sea_orm(rs_type = "String", db_type = "String(StringLen::N(16))")
)]
pub enum RunStatus {
    /// Waiting for a worker to pick it up
    #[default]
    #[cfg_attr(feature = "database", sea_orm(string_value = "queued"))]
    Queued,
    /// A worker is executing the workflow
    #[cfg_attr(feature = "database", sea_orm(string_value = "running"))]
    Running,
    /// The workflow finished with exit code 0
    #[cfg_attr(feature = "database", sea_orm(string_value = "succeeded"))]
    Succeeded,
    /// The workflow exited with an error, timed out, or could not be started
    #[cfg_attr(feature = "database", sea_orm(string_value = "failed"))]
    Failed,
    /// Stopped by a user before it finished
    #[cfg_attr(feature = "database", sea_orm(string_value = "cancelled"))]
    Cancelled,
}

impl RunStatus {
    /// A terminal run will not change anymore and may be evicted by the per-project run limit.
    /// `Queued` and `Running` runs are never terminal.
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Succeeded | Self::Failed | Self::Cancelled)
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;

    #[test]
    fn round_trip() {
        for s in [
            RunStatus::Queued,
            RunStatus::Running,
            RunStatus::Succeeded,
            RunStatus::Failed,
            RunStatus::Cancelled,
        ] {
            assert_eq!(RunStatus::from_str(&s.to_string()).unwrap(), s);
            let json = serde_json::to_string(&s).unwrap();
            assert_eq!(json, format!("\"{s}\""));
            assert_eq!(serde_json::from_str::<RunStatus>(&json).unwrap(), s);
        }
    }

    #[test]
    fn terminal_states() {
        assert!(!RunStatus::Queued.is_terminal());
        assert!(!RunStatus::Running.is_terminal());
        assert!(RunStatus::Succeeded.is_terminal());
        assert!(RunStatus::Failed.is_terminal());
        assert!(RunStatus::Cancelled.is_terminal());
    }
}
