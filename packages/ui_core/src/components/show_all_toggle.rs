use dioxus::prelude::*;

use crate::{current_user::CurrentUser, sharing::SharingEnabled, storage::IsStorage};

/// The administrator's "Show all" choice, kept in the [`IsStorage`] (browser local storage on
/// web) so it holds across lists and reloads.
pub struct ShowAll<S: IsStorage + 'static>(Signal<S>);

impl<S: IsStorage + 'static> Clone for ShowAll<S> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<S: IsStorage + 'static> Copy for ShowAll<S> {}

impl<S: IsStorage + 'static> ShowAll<S> {
    /// The choice backed by the storage provided as context. Call it in a component.
    pub fn from_context() -> Self {
        Self(use_context::<Signal<S>>())
    }

    /// Whether the user chose "Show all". Not enough to ask the backend for everything, see
    /// [`Self::is_active`].
    pub fn is_chosen(&self) -> bool {
        self.0.read().show_all().unwrap_or(false)
    }

    /// Whether lists should request everything: chosen, the user is an administrator and sharing
    /// is enabled. A stale choice stored by an administrator is therefore ignored for other users
    /// of the same browser instead of being refused by the backend. Reactive: call it inside the
    /// resource that fetches the list.
    pub fn is_active(&self) -> bool {
        self.is_chosen()
            && SharingEnabled::from_context().is_enabled()
            && CurrentUser::from_context().is_some_and(|user| user.is_admin())
    }

    /// Stores the choice, failures to store it are ignored (the choice then only lasts until the
    /// next reload)
    pub fn set(&mut self, show_all: bool) {
        let _ = self.0.write().set_show_all(show_all);
    }
}

/// Switch above a list that makes administrators see every project, workflow or group instead of
/// only the visible ones. Renders nothing for other users and where sharing is disabled
/// (desktop). The lists read the choice with [`ShowAll::is_active`].
#[component]
pub fn ShowAllToggle<S: IsStorage + 'static>() -> Element {
    let mut show_all = ShowAll::<S>::from_context();
    let is_admin = CurrentUser::from_context().is_some_and(|user| user.is_admin());
    if !SharingEnabled::from_context().is_enabled() || !is_admin {
        return rsx! {};
    }
    rsx! {
        div {
            class: "form-check form-switch h-fit-content me-3",
            input {
                class: "form-check-input",
                r#type: "checkbox",
                role: "switch",
                id: "show-all-toggle",
                checked: show_all.is_chosen(),
                onchange: move |event| show_all.set(event.checked()),
            }
            label {
                class: "form-check-label",
                r#for: "show-all-toggle",
                "Show all"
            }
        }
    }
}
