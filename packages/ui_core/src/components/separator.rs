use dioxus::prelude::*;

#[derive(Clone, PartialEq, Props)]
pub struct SeparatorProps {
    label: String,
}

#[component]
pub fn Separator(props: SeparatorProps) -> Element {
    rsx! {
        div {
            class: "d-flex justify-content-between",
            hr {
                class: "w-100",
            }
            div {
                class: "ds-flex align-self-center px-3",
                "{props.label}"
            }
            hr {
                class: "w-100",
            }
        }
    }
}
