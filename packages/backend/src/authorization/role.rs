use macworp_core::entities::Role;

/// Stored form of [`Role`] in `shares.role`.
pub(crate) trait RoleDb: Sized {
    /// Value stored in the column
    fn to_db(self) -> i16;
    /// Role of a stored value, `None` for unknown values
    fn from_db(value: i16) -> Option<Self>;
}

impl RoleDb for Role {
    fn to_db(self) -> i16 {
        self as i16
    }

    fn from_db(value: i16) -> Option<Self> {
        match value {
            1 => Some(Self::Viewer),
            2 => Some(Self::Editor),
            3 => Some(Self::Owner),
            _ => None,
        }
    }
}

/// Who a share is granted to, stored in `shares.principal_type`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrincipalType {
    /// A single user
    User = 1,
    /// All members of a group
    Group = 2,
    /// Every user, no principal id
    Public = 3,
}

impl PrincipalType {
    /// Value stored in the column
    pub(crate) fn to_db(self) -> i16 {
        self as i16
    }
}

/// What a share is granted on, stored in `shares.resource_type`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResourceType {
    /// A project, see [`crate::models::Project`]
    Project = 1,
    /// A workflow, see [`crate::models::WorkflowIntegration`]
    Workflow = 2,
    /// The global reference data, see [`crate::models::ReferenceData`]
    ReferenceData = 3,
}

impl ResourceType {
    /// Value stored in the column
    pub(crate) fn to_db(self) -> i16 {
        self as i16
    }
}
