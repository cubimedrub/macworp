use dioxus::prelude::*;

use macworp_core::account_deletion::RULES;

/// The rules for what happens to a user's projects, workflows and groups when their account is
/// deleted, to show in the delete-account confirmation.
#[component]
pub fn AccountDeletionRules() -> Element {
    rsx! {
        p { "Deleting an account cannot be undone. What happens to what the account owns:" }
        ul {
            for rule in RULES {
                li { "{rule}" }
            }
        }
    }
}
