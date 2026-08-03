use std::{path::PathBuf, str::FromStr};

use dioxus::prelude::*;
use macworp_core::{
    responses::WorkflowIntegrationResponse,
    workflow_configuration::{
        engine::{Engine, EngineParameter},
        parameters::Parameter,
        source::{LocalSource, RemoteSource, Source},
    },
};
use strum::IntoEnumIterator;
use thiserror::Error;

use crate::{
    client::Client,
    components::{separator::Separator, workflow_integrations::*, ConfirmationDialog, Spinner},
    configuration::Configuration,
    router::IsRouter,
    storage::IsStorage,
};

#[derive(Debug, Error)]
pub enum Error<S: IsStorage> {
    #[error("Config not loaded")]
    ConfigNotLoaded,
    #[error("Failed to fetch project: {0}")]
    FetchProject(crate::client::Error),
    #[error("Failed to delete projects: {0}")]
    DeleteProject(crate::client::Error),
    #[error("Failed to save workflow integration: {0}")]
    SaveWorkflowIntegration(crate::client::Error),
    #[error("Failed to upload file: {0}")]
    Upload(crate::client::Error),
    #[error("Not logged in")]
    NotLoggedIn,
    #[error("{0}")]
    Storage(S::Error),
    #[error("Unable to reqd upload queue")]
    PoisenedUploadQueue,
}

pub struct DisassambledWorkflowIntegration {
    inner: WorkflowIntegrationResponse,
    engine_parameters: Vec<Signal<EngineParameter>>,
    dynamic_parameters: Vec<Signal<Parameter>>,
    fixed_parameters: Vec<Signal<Parameter>>,
}

impl DisassambledWorkflowIntegration {
    pub fn inner(&self) -> &WorkflowIntegrationResponse {
        &self.inner
    }

    pub fn inner_mut(&mut self) -> &mut WorkflowIntegrationResponse {
        &mut self.inner
    }

    pub fn engine_parameters(&self) -> &[Signal<EngineParameter>] {
        &self.engine_parameters
    }

    pub fn dynamic_parameters(&self) -> &[Signal<Parameter>] {
        &self.dynamic_parameters
    }

    pub fn dynamic_parameters_mut(&mut self) -> &mut Vec<Signal<Parameter>> {
        &mut self.dynamic_parameters
    }

    pub fn fixed_parameters(&self) -> &[Signal<Parameter>] {
        &self.fixed_parameters
    }

    pub fn fixed_parameters_mut(&mut self) -> &mut Vec<Signal<Parameter>> {
        &mut self.fixed_parameters
    }

    /// Reassembles the disassembled parameter signals back into the inner
    /// response's configuration and returns it, consuming self.
    ///
    pub fn into_response(mut self) -> WorkflowIntegrationResponse {
        *self.inner.configuration_mut().engine_parameters_mut() = self
            .engine_parameters
            .drain(..)
            .map(|param| param.read().clone())
            .collect();

        *self.inner.configuration_mut().dynamic_parameters_mut() = self
            .dynamic_parameters
            .drain(..)
            .map(|param| param.read().clone())
            .collect();

        *self.inner.configuration_mut().fixed_parameters_mut() = self
            .fixed_parameters
            .drain(..)
            .map(|param| param.read().clone())
            .collect();

        self.inner
    }
}

impl From<WorkflowIntegrationResponse> for DisassambledWorkflowIntegration {
    fn from(mut workflow_integration: WorkflowIntegrationResponse) -> Self {
        let engine_parameters = workflow_integration
            .configuration_mut()
            .engine_parameters_mut()
            .drain(..)
            .map(|param| Signal::new(param))
            .collect::<Vec<_>>();

        let dynamic_parameters = workflow_integration
            .configuration_mut()
            .dynamic_parameters_mut()
            .drain(..)
            .map(|param| Signal::new(param))
            .collect::<Vec<_>>();

        let fixed_parameters = workflow_integration
            .configuration_mut()
            .fixed_parameters_mut()
            .drain(..)
            .map(|param| Signal::new(param))
            .collect::<Vec<_>>();

        Self {
            inner: workflow_integration,
            engine_parameters,
            dynamic_parameters,
            fixed_parameters,
        }
    }
}

fn on_workflow_source_change(
    mut workflow_integration: Signal<DisassambledWorkflowIntegration>,
    mut last_source: Signal<Option<Source>>,
    event: Event<FormData>,
) {
    // grab and hold write locks
    let mut workflow_integration = workflow_integration.write();
    let inner_integration = workflow_integration.inner_mut();
    let config = inner_integration.configuration_mut();
    let current_source = config.source_mut();
    let last_source = &mut *last_source.write();

    // Parse new source type
    let new_source_type = match Source::from_str(&event.value()) {
        Ok(source) => source,
        Err(err) => {
            tracing::error!("{err}");
            return;
        }
    };

    // Check if source type actually changed, if not do nothing to avoid resetting source specific fields
    if new_source_type.is_same_variant(current_source) {
        tracing::debug!("Same variants, doing nothing");
        return;
    }

    // Deal with uninitialized last source
    if last_source.is_none() {
        match new_source_type {
            Source::Local(_) => *last_source = Some(Source::Local(LocalSource::default())),
            Source::Remote(_) => *last_source = Some(Source::Remote(RemoteSource::default())),
        }
    }

    if last_source
        .as_ref()
        .unwrap()
        .is_same_variant(&new_source_type)
    {
        std::mem::swap(current_source, last_source.as_mut().unwrap());
    } else if new_source_type.is_local() {
        // if new is local, last must be remote which is "more complex"
        // so lets just create a new local so the complex type is not lost.
        *current_source = Source::Local(LocalSource::default());
    } else if new_source_type.is_remote() {
        // if new is remote, last must be local which less "more complex"
        // so we override the last source with a new remote and then swap
        // so the more complex setting got saved
        last_source.replace(Source::Remote(RemoteSource::default()));
        std::mem::swap(current_source, last_source.as_mut().unwrap());
    }
}

pub trait IsWorkFlowEdit {
    type Storage: IsStorage + 'static;
    type Router: IsRouter + 'static;
}

#[derive(Clone, PartialEq, Props)]
pub struct EditProps {
    pub id: i64,
}

#[component]
pub fn Edit<S: IsStorage + 'static, R: IsRouter + 'static>(props: EditProps) -> Element {
    let storage = use_context::<Signal<S>>();
    let config = use_context::<Signal<Configuration>>();
    let navigator = navigator();

    let mut is_delete_confirmation_dialog_open = use_signal(|| false);
    let last_workflow_source = use_signal(|| None::<Source>);
    let mut dynamic_parameter_to_add = use_signal(Parameter::default);

    let workflow_integration_res: Resource<
        Result<Signal<DisassambledWorkflowIntegration>, Error<S>>,
    > = use_resource(move || async move {
        let jwt = storage
            .read()
            .jwt()
            .map_err(Error::Storage)?
            .ok_or(Error::NotLoggedIn)?;

        let config = config.read().clone();

        let workflow_integration = Client::new(config.base_url().to_string(), config.timeout())
            .workflow_integrations()
            .workflow_integration(&jwt, props.id)
            .await
            .map_err(Error::FetchProject)?;

        Ok(Signal::new(workflow_integration.into()))
    });

    let mut delete_action = use_action(move || async move {
        let jwt = storage
            .read()
            .jwt()
            .map_err(Error::Storage)?
            .ok_or(Error::NotLoggedIn)?;

        let config = config.read().clone();

        Client::new(config.base_url().to_string(), config.timeout())
            .workflow_integrations()
            .delete(&jwt, props.id)
            .await
            .map_err(Error::DeleteProject)?;

        navigator.push(R::workflow_integration_index_route());

        Ok::<_, Error<S>>(())
    });

    let mut save_action = use_action(move || async move {
        let jwt = storage
            .read()
            .jwt()
            .map_err(Error::Storage)?
            .ok_or(Error::NotLoggedIn)?;

        let config = config.read().clone();

        let mut workflow_integration_signal = match &*workflow_integration_res.read() {
            Some(Ok(signal)) => *signal,
            _ => return Ok::<_, Error<S>>(()),
        };

        let disassembled = {
            let current = workflow_integration_signal.read();
            DisassambledWorkflowIntegration {
                inner: current.inner().clone(),
                engine_parameters: current.engine_parameters().to_vec(),
                dynamic_parameters: current.dynamic_parameters().to_vec(),
                fixed_parameters: current.fixed_parameters().to_vec(),
            }
        };
        let response = disassembled.into_response();

        let updated = Client::new(config.base_url().to_string(), config.timeout())
            .workflow_integrations()
            .update_workflow_integration(
                &jwt,
                props.id,
                response.name.clone(),
                response.description.clone(),
                response.is_public,
                response.configuration().clone(),
            )
            .await
            .map_err(Error::SaveWorkflowIntegration)?;

        workflow_integration_signal.write().inner = updated;

        Ok::<_, Error<S>>(())
    });

    rsx! {
        match &*workflow_integration_res.read() {
            Some(Ok(mut workflow_integration)) => rsx!{
                ConfirmationDialog {
                    is_open: is_delete_confirmation_dialog_open,
                    title: "Delete workflow integration".to_string(),
                    body: "Are you sure you want to delete this workflow integration? This action cannot be undone.".to_string(),
                    confirmation_button_label: "Delete".to_string(),
                    confirmation_callback: Callback::new(move |_| {
                        is_delete_confirmation_dialog_open.set(false);
                        delete_action.call();
                    }),
                    cancel_callback: Callback::new(move |_| is_delete_confirmation_dialog_open.set(false))
                }
                div {
                    class: "logo d-flex flex-row justify-content-between mb-3",
                    h1 {
                        class: "mb-0",
                        "{workflow_integration.read().inner().name}"
                    }
                    div {
                        class: "d-flex flex-row",
                        button {
                            id: "save-btn",
                            class: "btn btn-sm btn-outline-primary me-2",
                            title: "Save workflow integration",
                            onclick: move |_| { save_action.call(); },
                            i {
                                class: "fa-solid fa-floppy-disk me-1"
                            }
                            "Save"
                        }
                        button {
                            id: "collapse-btn",
                            class: "btn btn-sm btn-outline-danger",
                            title: "Collapse sidebar",
                            onclick: move |_| is_delete_confirmation_dialog_open.set(true),
                            i {
                                class: "fa-solid fa-trash me-1"
                            }
                            "Delete"
                        }
                    }
                }
                match delete_action.value() {
                    Some(Err(err)) => rsx! {
                        div {
                            class: "alert alert-danger",
                            role: "alert",
                            "Failed to delete project: {err}" }
                    },
                    _ => rsx! {}
                }
                match save_action.value() {
                    Some(Err(err)) => rsx! {
                        div {
                            class: "alert alert-danger",
                            role: "alert",
                            "Failed to save workflow integration: {err}" }
                    },
                    _ => rsx! {}
                }
                div {
                    class: "input-group mb-3",
                    span {
                        class: "input-group-text",
                        id: "workflow-integration-name",
                        "Name"
                    }
                    input {
                        type: "text",
                        class: "form-control",
                        placeholder: "Workflow integration name",
                        aria_label: "Workflow integration name",
                        aria_describedby: "workflow-integration-name",
                        value: "{workflow_integration.read().inner().name}",
                        oninput: move |event| workflow_integration.write().inner_mut().name = event.value(),
                    }
                }
                div {
                    class: "input-group mb-3",
                    span {
                        class: "input-group-text",
                        id: "workflow-integration-description",
                        "Description"
                    }
                    textarea {
                        class: "form-control",
                        placeholder: "Workflow integration description",
                        aria_label: "Workflow integration description",
                        value: "{workflow_integration.read().inner().description}",
                        oninput: move |event| workflow_integration.write().inner_mut().description = event.value(),
                    }
                }
                h2 {
                    "Configuration"
                },
                p {
                    "This setting defines the kind of workflow (Snakemake, Nextflow, Docker, ...), where to get the workflow source and how the paramter inputs should be rendered for the user."
                }
                Separator{
                    label: "Engine"
                }
                div {
                    class: "input-group mb-3",
                    span {
                        class: "input-group-text",
                        id: "workflow-integration-configuration-engine",
                        "Engine"
                    }
                    select {
                        class: "form-select",
                        aria_label: "Workflow engine",
                        onchange: move |event| {
                            let engine = event.value();
                            if let Ok(engine) = engine.parse::<Engine>() {
                                *workflow_integration.write().inner_mut().configuration.engine_mut() = engine;
                            }
                        },
                        for engine in Engine::iter() {
                            option {
                                selected: *workflow_integration.read().inner().configuration.engine() == engine,
                                value: "{engine}",
                                "{engine}"
                            }
                        }
                    }
                }

                Separator{
                    label: "Source"
                }

                div {
                    class: "input-group mb-3",
                    span {
                        class: "input-group-text",
                        id: "workflow-integration-configuration-engine",
                        "Engine"
                    }
                    select {
                        class: "form-select",
                        aria_label: "Workflow source",
                        onchange: move |event| on_workflow_source_change(workflow_integration, last_workflow_source, event),
                        for source in Source::iter() {
                            option {
                                selected: source.is_same_variant(workflow_integration.read().inner().configuration.source()),
                                value: "{source}",
                                "{source}"
                            }
                        }
                    }
                }

                match workflow_integration.read().inner().configuration().source() {
                    Source::Local(source) => rsx! {
                        div {
                            class: "input-group mb-3",
                            span {
                                class: "input-group-text",
                                id: "workflow-integration-configuration-source-local-location",
                                "Location"
                            }
                            input {
                                type: "text",
                                class: "form-control",
                                placeholder: "Location folder containing the workflow",
                                aria_label: "Location folder containing the workflow",
                                aria_describedby: "workflow-integration-configuration-source-local-location",
                                value: "{source.location().display()}",
                                oninput: move |event| if let Source::Local(source) = workflow_integration.write().inner_mut().configuration_mut().source_mut() {
                                    *source.location_mut() = PathBuf::from(event.value());
                                }
                            }
                        },
                        div {
                            class: "input-group mb-3",
                            span {
                                class: "input-group-text",
                                id: "workflow-integration-configuration-source-local-script",
                                "Script"
                            }
                            input {
                                type: "text",
                                class: "form-control",
                                placeholder: "Path to main script within location",
                                aria_label: "Path to main script within location",
                                aria_describedby: "workflow-integration-configuration-source-local-script",
                                value: "{source.script().display()}",
                                oninput: move |event| if let Source::Local(source) = workflow_integration.write().inner_mut().configuration_mut().source_mut() {
                                    *source.script_mut() = PathBuf::from(event.value());
                                }
                            }
                        }
                    },
                    Source::Remote(source) => rsx!{
                        div {
                            class: "input-group mb-3",
                            span {
                                class: "input-group-text",
                                id: "workflow-integration-configuration-source-remote-url",
                                "URL"
                            }
                            input {
                                type: "text",
                                class: "form-control",
                                placeholder: "git@github.com:cubimedrub/macworp-genomics-demo-workflow-nextflow.git",
                                aria_label: "URL to the remote repository",
                                aria_describedby: "workflow-integration-configuration-source-remote-url",
                                value: "{source.url()}",
                                oninput: move |event| if let Source::Remote(source) = workflow_integration.write().inner_mut().configuration_mut().source_mut() {
                                    *source.url_mut() = event.value();
                                }
                            }
                        },
                        div {
                            class: "input-group mb-3",
                            span {
                                class: "input-group-text",
                                id: "workflow-integration-configuration-source-remote-version",
                                "Version"
                            }
                            input {
                                type: "text",
                                class: "form-control",
                                placeholder: "main | master | dev | 1.2.1 | #663eee7",
                                aria_label: "Workflow version",
                                aria_describedby: "workflow-integration-configuration-source-remote-version",
                                value: "{source.version().map(|version| version.as_str()).unwrap_or_default()}",
                                oninput: move |event| if let Source::Remote(source) = workflow_integration.write().inner_mut().configuration_mut().source_mut() {
                                    *source.version_mut() = if event.value().is_empty() { None } else { Some(event.value()) }
                                }
                            }
                        }
                        div {
                            class: "input-group mb-3",
                            span {
                                class: "input-group-text",
                                id: "workflow-integration-configuration-source-remote-script",
                                "Script"
                            }
                            input {
                                type: "text",
                                class: "form-control",
                                placeholder: "Path to main script within location",
                                aria_label: "Path to main script within location",
                                aria_describedby: "workflow-integration-configuration-source-remote-script",
                                value: "{source.script().map(|path| path.display().to_string()).unwrap_or_default()}",
                                oninput: move |event| if let Source::Remote(source) = workflow_integration.write().inner_mut().configuration_mut().source_mut() {
                                    *source.script_mut() = if event.value().is_empty() { None } else { Some(PathBuf::from(event.value())) }
                                }
                            }
                        }
                    }
                }

                Separator{
                    label: "Dynamic paramters"
                }
                p {
                    "These parameters can be changed by the user when selecting the workflow for execution"
                }

                for (parameter_idx, parameter) in workflow_integration.read().dynamic_parameters().iter().enumerate() {
                    match &*parameter.read() {
                        Parameter::Float(_) => rsx! {
                            NumericParameter<DynamicParameter, f64> {
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
                            p {
                                "{parameter}"
                            }
                        }
                    }
                }

                div {
                    class: "input-group mb-3",
                    span {
                        class: "input-group-text",
                        id: "workflow-integration-configuration-new-dynamic-parameter",
                        "Add new parameter"
                    }
                    select {
                        class: "form-select",
                        aria_label: "Dynamic paramter to add",
                        onchange: move |event|{
                            event.prevent_default();
                            let param_type = event.value();
                            if let Ok(param_type) = param_type.parse::<Parameter>() {
                                dynamic_parameter_to_add.set(param_type);
                            }
                        },
                        for param_type in Parameter::iter() {
                            option {
                                selected: dynamic_parameter_to_add.read().is_same_variant(&param_type),
                                value: "{param_type}",
                                "{param_type}"
                            }
                        }
                    }
                    button {
                        class: "btn btn-outline-secondary",
                        r#type: "button",
                        onclick: move |event| {
                            event.prevent_default();
                            workflow_integration.write().dynamic_parameters_mut().push(Signal::new(dynamic_parameter_to_add.read().clone()))
                        },
                        i {
                            class: "fas fa-plus"
                        }
                    }
                }
            },
            Some(Err(err)) => rsx!(
                div {
                    class: "alert alert-danger",
                    "{err}"
                }
            ),
            None => rsx!(
                Spinner {}
            )
        }
    }
}
