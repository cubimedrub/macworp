use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Where the workflow of a run came from, kept because the run snapshot only points at the
/// immutable checkout in the repository cache.
///
/// Only runs of remote and nf-core workflows have one; local workflows have none.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "database", derive(sea_orm::FromJsonQueryResult))]
pub struct RunProvenance {
    /// Repository URL, for nf-core pipelines the URL of the pipeline repository
    pub url: String,
    /// Tag, branch or commit as configured, `None` if the default branch was used
    pub requested_ref: Option<String>,
    /// Full hash of the commit the run executes
    pub commit: String,
    /// Main script of the workflow, relative to the repository root
    pub script: PathBuf,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The JSON round trip keeps all fields, also an absent reference.
    #[test]
    fn round_trip() {
        let provenance = RunProvenance {
            url: "https://github.com/nf-core/rnaseq".into(),
            requested_ref: None,
            commit: "a".repeat(40),
            script: "main.nf".into(),
        };
        let json = serde_json::to_string(&provenance).unwrap();
        assert_eq!(
            serde_json::from_str::<RunProvenance>(&json).unwrap(),
            provenance
        );
    }
}
