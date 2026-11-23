use serde::{Deserialize, Serialize};

/// What a user may do with a shared project or workflow. Ordered: a higher role includes the lower
/// ones.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Role {
    /// See and use
    Viewer = 1,
    /// Modify, and run workflows on a project
    Editor = 2,
    /// Delete and manage shares
    Owner = 3,
}
