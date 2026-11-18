use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// What the global reference data looked like when a run was started, kept because the snapshot
/// the run read is deleted after the run. Lets users see which state of the reference data a
/// result belongs to, and notice that two runs read different states.
///
/// Only runs that read a snapshot have one (not those of the desktop app).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "database", derive(sea_orm::FromJsonQueryResult))]
pub struct ReferenceFingerprint {
    /// Number of files in the snapshot
    pub file_count: u64,
    /// Sum of the file sizes in bytes
    pub total_bytes: u64,
    /// Modification time of the most recently changed file, `None` for an empty reference folder
    pub last_modified: Option<DateTime<Utc>>,
    /// When the snapshot was taken
    pub snapshot_at: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The JSON round trip keeps all fields, also an absent modification time.
    #[test]
    fn round_trip() {
        let fingerprint = ReferenceFingerprint {
            file_count: 3,
            total_bytes: 1024,
            last_modified: None,
            snapshot_at: DateTime::UNIX_EPOCH,
        };
        let json = serde_json::to_string(&fingerprint).unwrap();
        assert_eq!(
            serde_json::from_str::<ReferenceFingerprint>(&json).unwrap(),
            fingerprint
        );
    }
}
