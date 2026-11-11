use dioxus::prelude::*;
use macworp_core::responses::UserResponse;

/// The logged in user, fetched once by the layout and provided as context so that navigation and
/// pages can tell whether the user administers the instance.
#[derive(Clone, Copy)]
pub struct CurrentUser(Resource<Option<UserResponse>>);

impl CurrentUser {
    /// Wraps the resource that loads the logged in user, `None` if loading failed
    pub fn new(resource: Resource<Option<UserResponse>>) -> Self {
        Self(resource)
    }

    /// The current user as provided by the layout, `None` outside of it. Call it in a component.
    pub fn from_context() -> Option<Self> {
        try_use_context::<Self>()
    }

    /// The user, `None` while loading or if loading failed
    pub fn user(&self) -> Option<UserResponse> {
        self.0.read().clone().flatten()
    }

    /// Whether the server lets the user create local workflows. Assumed true while the user is
    /// unknown (the backend enforces the policy anyway).
    pub fn can_create_local_workflows(&self) -> bool {
        self.user()
            .and_then(|user| user.can_create_local_workflows)
            .unwrap_or(true)
    }

    /// Whether the user is known and administers the instance
    pub fn is_admin(&self) -> bool {
        self.user().is_some_and(|user| user.is_admin)
    }
}
