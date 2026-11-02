use dioxus::prelude::*;

/// Badge "owner: none" marking a project, workflow or group nobody owns anymore (its owner deleted their
/// account). Administrators can assign a new owner.
#[component]
pub fn OwnerlessBadge() -> Element {
    rsx! {
        span {
            class: "badge text-bg-warning ms-2",
            title: "Nobody owns this; an administrator can assign an owner",
            "owner: none"
        }
    }
}
