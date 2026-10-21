use dioxus::prelude::*;
use macworp_core::{
    requests::ParameterValue,
    responses::WorkflowIntegrationResponse,
    workflow_configuration::{parameters::Parameter, render::value_json},
};
use thiserror::Error;

use crate::{
    client::Client,
    components::{
        projects::workflow_integrations::{GlobParameter, NumericParameter, TextParameter},
        workflow_integrations::DynamicParameter,
        Spinner,
    },
    configuration::Configuration,
    pages::workflow_integrations::edit::DisassambledWorkflowIntegration,
    storage::IsStorage,
};

#[derive(Debug, Error)]
pub enum Error<S: IsStorage> {
    #[error("Failed to fetch workflow integrations: {0}")]
    FetchWorkflowIntegrations(#[from] crate::client::Error),
    #[error("Not logged in")]
    NotLoggedIn,
    #[error("{0}")]
    Storage(S::Error),
}

#[derive(Clone, PartialEq, Props)]
pub struct WorkflowStartDialogProps {
    pub is_open: ReadSignal<bool>,
    pub cancel_callback: Callback<(), ()>,
    pub start_callback: Callback<(i64, Vec<ParameterValue>), ()>,
}

/// Dialog for selecting a workflow integration to start and previewing its dynamic parameters.
///
/// # Arguments
/// * `is_open` - A signal that indicates whether the dialog is open or not
/// * `cancel_callback` - The callback to call when the dialog is closed without starting
/// * `start_callback` - The callback to call with the selected workflow's id and the entered dynamic parameter values when the user confirms
///
#[component]
pub fn WorkflowStartDialog<S: IsStorage + 'static>(props: WorkflowStartDialogProps) -> Element {
    let storage = use_context::<Signal<S>>();
    let config = use_context::<Signal<Configuration>>();

    let mut selected: Signal<Option<DisassambledWorkflowIntegration>> = use_signal(|| None);

    use_effect(move || {
        if *props.is_open.read() {
            selected.set(None);
        }
    });

    let mut workflow_integrations: Resource<Result<Vec<WorkflowIntegrationResponse>, Error<S>>> =
        use_resource(move || async move {
            let jwt = storage
                .read()
                .jwt()
                .map_err(Error::Storage)?
                .ok_or(Error::NotLoggedIn)?;

            let config = config.read().clone();

            Client::new(config.base_url().to_string(), config.timeout())
                .workflow_integrations()
                .workflow_integrations(&jwt)
                .await
                .map_err(Error::FetchWorkflowIntegrations)
        });

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
                            "Start workflow"
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
                        match &*selected.read() {
                            None => rsx! {
                                match &*workflow_integrations.read() {
                                    Some(Ok(integrations)) => rsx! {
                                        div {
                                            class: "list-group",
                                            for integration in integrations {
                                                button {
                                                    r#type: "button",
                                                    class: "list-group-item list-group-item-action",
                                                    onclick: {
                                                        let integration = integration.clone();
                                                        move |_| selected.set(Some(integration.clone().into()))
                                                    },
                                                    "{integration.name}"
                                                }
                                            }
                                        }
                                    },
                                    Some(Err(err)) => rsx! {
                                        div {
                                            class: "alert alert-danger",
                                            ondoubleclick: move |_| workflow_integrations.restart(),
                                            "{err}"
                                        }
                                    },
                                    None => rsx! {
                                        Spinner {}
                                    },
                                }
                            },
                            Some(integration) => rsx! {
                                // p {
                                //     class: "text-muted",
                                //     "{integration.inner().description()}"
                                // }
                                for (parameter_idx, parameter) in integration.dynamic_parameters().iter().enumerate() {
                                    match &*parameter.read() {
                                        Parameter::Float(_) => rsx! {
                                            NumericParameter<DynamicParameter, f64> {
                                                parameter: *parameter,
                                                parameter_idx
                                            },
                                        },
                                        Parameter::Glob(_) => rsx! {
                                            GlobParameter<DynamicParameter> {
                                                parameter: *parameter,
                                                parameter_idx
                                            },
                                        },
                                        Parameter::Integer(_) => rsx! {
                                            NumericParameter<DynamicParameter, i64> {
                                                parameter: *parameter,
                                                parameter_idx
                                            },
                                        },
                                        Parameter::Text(_) => rsx! {
                                            TextParameter<DynamicParameter> {
                                                parameter: *parameter,
                                                parameter_idx
                                            },
                                        },
                                        _ => rsx! {
                                            div {
                                                class: "alert alert-danger",
                                                "Unsupported parameter type"
                                            }
                                        }
                                    }
                                }
                            },
                        }
                    }
                    div {
                        class: "modal-footer",
                        if selected.read().is_some() {
                            button {
                                r#type: "button",
                                class: "btn btn-secondary",
                                onclick: move |_| selected.set(None),
                                "Back"
                            }
                        }
                        button {
                            r#type: "button",
                            class: "btn btn-secondary",
                            onclick: move |_| props.cancel_callback.call(()),
                            "Close"
                        }
                        button {
                            r#type: "button",
                            class: "btn btn-primary",
                            disabled: selected.read().is_none(),
                            onclick: move |_| {
                                if let Some(integration) = &*selected.read() {
                                    let values = integration
                                        .dynamic_parameters()
                                        .iter()
                                        .map(|parameter| {
                                            let parameter = parameter.read();
                                            ParameterValue {
                                                name: parameter.name().to_string(),
                                                value: value_json(&parameter),
                                            }
                                        })
                                        .collect();
                                    props.start_callback.call((integration.inner().id, values));
                                }
                            },
                            "Start"
                        }
                    }
                }
            }
        }
    }
}
