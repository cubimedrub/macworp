use dioxus::prelude::*;

#[derive(Clone, PartialEq, Props)]
pub struct ErrorProps {
    pub message: String,
}

#[component]
pub fn Error<R: Routable>(props: ErrorProps) -> Element {
    tracing::error!("Error page: {}", props.message);

    rsx! {
        div {
            class: "alert alert-danger",
            "{props.message}"
        }
    }
}
