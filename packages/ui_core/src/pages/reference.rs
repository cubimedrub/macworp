use dioxus::prelude::*;
use macworp_core::routing::ReferenceRouting;

use super::projects::edit::{Edit, IsProjectEdit};

/// Page of the global reference data: browse for everybody, upload and delete for Editors and
/// admins (web), share for admins. The same page as a project's files, minus runs and deletion.
#[component]
pub fn ReferenceEdit<T: IsProjectEdit + 'static>() -> Element {
    rsx! {
        Edit::<T> {
            id: ReferenceRouting::SINGLETON_ID,
            reference: true,
        }
    }
}
