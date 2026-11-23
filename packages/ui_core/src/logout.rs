use dioxus::prelude::*;

use crate::{router::IsRouter, storage::IsStorage};

/// Whether the platform has a login and therefore a logout. Provided as context by platforms that
/// want to change the default; without it logout is enabled. The desktop app has no login and
/// provides `LogoutEnabled(false)`, which hides the logout entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LogoutEnabled(pub bool);

impl LogoutEnabled {
    /// The flag provided by the platform, enabled if it provides none. Call it in a component.
    pub fn from_context() -> Self {
        try_use_context::<Self>().unwrap_or(Self(true))
    }

    /// Whether the logout entry should be shown.
    pub fn is_enabled(self) -> bool {
        self.0
    }
}

/// Forgets the stored token and navigates to the login page showing `message`; if the token
/// cannot be forgotten, navigates to the error page instead. Used after logout and whenever the
/// backend reports that the token is no longer valid.
pub(crate) fn return_to_login<R: IsRouter, S: IsStorage + 'static>(
    mut storage: Signal<S>,
    message: &str,
) {
    let stored = storage.write().set_jwt(None);
    let nav = navigator();
    match stored {
        Ok(()) => nav.push(R::login_route(Some(message.to_string()))),
        Err(err) => nav.push(R::error_route(format!("{err}"))),
    };
}
