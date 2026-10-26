use dioxus::prelude::*;
use macworp_core::{
    requests::ParameterValue,
    responses::WorkflowIntegrationResponse,
    workflow_configuration::{
        parameters::{parameter::IsParameter, Parameter},
        render::{apply_values_lenient, value_json},
    },
};
use thiserror::Error;

use crate::{
    client::Client,
    components::{
        projects::workflow_integrations::{
            GlobParameter, NumericParameter, PathParameter, TextParameter, ValueSelectParameter,
        },
        workflow_integrations::DynamicParameter,
        Spinner,
    },
    configuration::Configuration,
    pages::workflow_integrations::edit::DisassambledWorkflowIntegration,
    runtime_status::engine_hint,
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

/// Applies the values of the project's last run of `integration` to its dynamic parameters.
/// Without usable history (none yet, request failed) the stored defaults stay.
async fn with_history<S: IsStorage + 'static>(
    storage: Signal<S>,
    config: Signal<Configuration>,
    project_id: i64,
    mut integration: WorkflowIntegrationResponse,
) -> DisassambledWorkflowIntegration {
    let jwt = storage.read().jwt().ok().flatten();
    if let Some(jwt) = jwt {
        let config = config.read().clone();
        let last = Client::new(config.base_url().to_string(), config.timeout())
            .runs()
            .last(&jwt, project_id, Some(integration.id))
            .await;
        if let Ok(last) = last {
            apply_values_lenient(integration.configuration_mut(), &last.parameters);
        }
    }
    integration.into()
}

#[derive(Clone, PartialEq, Props)]
pub struct WorkflowStartDialogProps {
    /// Project the workflow is started on, its files can be picked for path parameters
    pub project_id: i64,
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

    // Preselect the workflow of the last run once the dialog is open and the list is loaded.
    // A manual choice made meanwhile wins.
    use_effect(move || {
        if !*props.is_open.read() {
            return;
        }
        let integrations = match &*workflow_integrations.read() {
            Some(Ok(integrations)) => integrations.clone(),
            _ => return,
        };
        spawn(async move {
            let jwt = storage.read().jwt().ok().flatten();
            let Some(jwt) = jwt else { return };
            let client_config = config.read().clone();
            let last = Client::new(
                client_config.base_url().to_string(),
                client_config.timeout(),
            )
            .runs()
            .last(&jwt, props.project_id, None)
            .await;
            let Some(integration) = last
                .ok()
                .and_then(|l| integrations.into_iter().find(|i| i.id == l.workflow_id))
                .filter(|i| engine_hint(i.configuration().engine()).is_none())
            else {
                return;
            };
            let prefilled = with_history(storage, config, props.project_id, integration).await;
            if selected.peek().is_none() {
                selected.set(Some(prefilled));
            }
        });
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
                                                {
                                                    let hint = engine_hint(integration.configuration().engine());
                                                    rsx! {
                                                button {
                                                    r#type: "button",
                                                    class: if hint.is_some() { "list-group-item list-group-item-action disabled" } else { "list-group-item list-group-item-action" },
                                                    disabled: hint.is_some(),
                                                    title: hint.clone(),
                                                    onclick: {
                                                        let integration = integration.clone();
                                                        move |_| {
                                                            let integration = integration.clone();
                                                            spawn(async move {
                                                                let prefilled = with_history(
                                                                    storage,
                                                                    config,
                                                                    props.project_id,
                                                                    integration,
                                                                )
                                                                .await;
                                                                selected.set(Some(prefilled));
                                                            });
                                                        }
                                                    },
                                                    "{integration.name}"
                                                    if let Some(hint) = &hint {
                                                        small { class: "d-block text-muted", "{hint}" }
                                                    }
                                                }
                                                    }
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
                                        Parameter::Path(_) => rsx! {
                                            PathParameter<DynamicParameter, S> {
                                                parameter: *parameter,
                                                parameter_idx,
                                                project_id: props.project_id,
                                            },
                                        },
                                        Parameter::ValueSelect(_) => rsx! {
                                            ValueSelectParameter<DynamicParameter> {
                                                parameter: *parameter,
                                                parameter_idx
                                            },
                                        },
                                        Parameter::Separator(separator) => rsx! {
                                            hr {}
                                            h6 { "{separator.label()}" }
                                            small {
                                                class: "form-text text-muted",
                                                "{separator.desc()}"
                                            }
                                        },
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
                                        .filter(|parameter| !matches!(&*parameter.read(), Parameter::Separator(_)))
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
