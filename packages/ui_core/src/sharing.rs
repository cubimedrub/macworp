use dioxus::prelude::*;

/// Whether the platform supports sharing and groups. Provided as context by platforms that want
/// to change the default; without it sharing is enabled. The desktop app has one local user and
/// provides `SharingEnabled(false)`, which hides share dialogs, groups and the owner column.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SharingEnabled(pub bool);

impl SharingEnabled {
    /// The flag provided by the platform, enabled if it provides none. Call it in a component.
    pub fn from_context() -> Self {
        try_use_context::<Self>().unwrap_or(Self(true))
    }

    /// Whether sharing UI should be shown.
    pub fn is_enabled(self) -> bool {
        self.0
    }
}
