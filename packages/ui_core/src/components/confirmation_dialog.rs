use dioxus::prelude::*;

#[derive(Clone, PartialEq, Props)]
pub struct ConfirmationDialogProps {
    pub title: String,
    pub body: String,
    pub confirmation_button_label: String,
    pub confirmation_callback: Callback<(), ()>,
    pub is_open: ReadSignal<bool>,
    pub cancel_callback: Callback<(), ()>,
}

/// Confimration dialog for securing deltion etc.
///
/// # Arguments
/// * `title` - The title of the dialog
/// * `body` - The body of the dialog
/// * `confirmation_button_label` - The label of the confirmation button
/// * `confirmation_callback` - The callback to call when the confirmation button is clicked
/// * `is_open` - A signal that indicates whether the dialog is open or not
/// * `cancel_callback` - The callback to call when the cancel button is clicked
///
#[component]
pub fn ConfirmationDialog(props: ConfirmationDialogProps) -> Element {
    let modal_class = use_memo(move || {
        props
            .is_open
            .read()
            .then(|| "modal show")
            .unwrap_or("modal")
            .to_string()
    });

    let modal_style = use_memo(move || {
        props
            .is_open
            .read()
            .then(|| "display: block;")
            .unwrap_or("display: none;")
            .to_string()
    });

    rsx! {
        div {
            class: modal_class,
            tabindex: "-1",
            style: modal_style,
            div {
                class: "modal-dialog",
                div {
                    class: "modal-content",
                    div {
                        class: "modal-header",
                        h5 {
                            class: "modal-title",
                            "{props.title}"
                        }
                        button {
                            r#type: "button",
                            class: "btn-close",
                            aria_label: "Close",
                            onclick: move |_| props.cancel_callback.call(())
                        }
                    }
                    div {
                        class: "modal-body",
                        p {
                            "{props.body}"
                        }
                    }
                    div {
                        class: "modal-footer",
                        button {
                            r#type: "button",
                            class: "btn btn-secondary",
                            onclick: move |_| props.cancel_callback.call(()),
                            "Close"
                        }
                        button {
                            r#type: "button",
                            class: "btn btn-primary",
                            onclick: move |_| props.confirmation_callback.call(()),
                            "{props.confirmation_button_label}"
                        }
                    }
                }
            }
        }
    }
}
