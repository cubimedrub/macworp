use dioxus::prelude::*;

#[component]
pub fn Spinner() -> Element {
    rsx! {
        div { class: "d-flex justify-content-center",
            div { class: "spinner-border", role: "status",
                span { class: "visually-hidden", "Loading..." }
            }
        }
    }
}
