#[cfg(feature = "database")]
use sea_orm::sea_query::StringLen;
use serde::{Deserialize, Serialize};
use strum::{Display, EnumString};

/// Origin of a run log line, stored as a snake_case string (e.g. `stderr`).
///
/// Lets the UI filter or colour the log, e.g. hide engine bookkeeping.
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
pub enum LogStream {
    /// Standard output of the workflow engine or container
    #[default]
    #[cfg_attr(feature = "database", sea_orm(string_value = "stdout"))]
    Stdout,
    /// Standard error of the workflow engine or container
    #[cfg_attr(feature = "database", sea_orm(string_value = "stderr"))]
    Stderr,
    /// Messages from MAcWorP itself or engine progress events (e.g. the Nextflow weblog), not process output
    #[cfg_attr(feature = "database", sea_orm(string_value = "engine"))]
    Engine,
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;

    #[test]
    fn round_trip() {
        assert_eq!(LogStream::from_str("stderr").unwrap(), LogStream::Stderr);
        assert_eq!(LogStream::Engine.to_string(), "engine");
    }
}
