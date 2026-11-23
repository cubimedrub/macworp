use dioxus::prelude::*;
use macworp_core::{
    responses::WorkflowIntegrationResponse, workflow_configuration::parameters::Parameter,
};

pub trait IsParameter {
    const ID_PART: &'static str;

    fn paramters(integration: &WorkflowIntegrationResponse) -> &[Parameter];
    fn paramters_mut(integration: &mut WorkflowIntegrationResponse) -> &mut Vec<Parameter>;
}

pub struct DynamicParameter;

impl IsParameter for DynamicParameter {
    const ID_PART: &'static str = "dynamic";

    fn paramters(integration: &WorkflowIntegrationResponse) -> &[Parameter] {
        integration.configuration().dynamic_parameters()
    }

    fn paramters_mut(integration: &mut WorkflowIntegrationResponse) -> &mut Vec<Parameter> {
        integration.configuration_mut().dynamic_parameters_mut()
    }
}

pub struct FixedParameter;

impl IsParameter for FixedParameter {
    const ID_PART: &'static str = "fixed";

    fn paramters(integration: &WorkflowIntegrationResponse) -> &[Parameter] {
        integration.configuration().fixed_parameters()
    }

    fn paramters_mut(integration: &mut WorkflowIntegrationResponse) -> &mut Vec<Parameter> {
        integration.configuration_mut().fixed_parameters_mut()
    }
}

#[derive(Clone, PartialEq, Props)]
pub struct ParameterProps {
    pub parameter: Signal<Parameter>,
    pub parameter_idx: usize,
}

#[component]
pub fn Parameter<P: IsParameter>(mut props: ParameterProps) -> Element {
    rsx! {
        div {
            class: "input-group mb-3",
            span {
                class: "input-group-text",
                id: "workflow-integration-configuration-{P::ID_PART}-parameter-{props.parameter_idx}-name",
                "Name"
            }
            input {
                type: "text",
                class: "form-control",
                placeholder: "Name of the parameter as passed via CLI, e.g. '--configuration' would be just 'configuration'",
                aria_label: "Name for workflow {P::ID_PART} parameter {props.parameter_idx} ",
                aria_describedby: "workflow-integration-configuration-{P::ID_PART}-parameter-{props.parameter_idx}-name",
                value: "{props.parameter.read().name()}",
                oninput: move |event| {
                    let mut param = props.parameter.write();
                    *param.name_mut() = event.value();
                }
            }
        }
        div {
            class: "input-group mb-3",
            span {
                class: "input-group-text",
                id: "workflow-integration-configuration-{P::ID_PART}-parameter-{props.parameter_idx}-label",
                "Label"
            }
            input {
                type: "text",
                class: "form-control",
                placeholder: "Label shown in user UI",
                aria_label: "Label shown in user UI",
                aria_describedby: "workflow-integration-configuration-{P::ID_PART}-parameter-{props.parameter_idx}-label",
                value: "{props.parameter.read().label()}",
                oninput: move |event| {
                    let mut param = props.parameter.write();
                    *param.label_mut() = event.value();
                }
            }
        }
        div {
            class: "input-group mb-3",
            span {
                class: "input-group-text",
                id: "workflow-integration-configuration-{P::ID_PART}-parameter-{props.parameter_idx}-description",
                "Description"
            }
            textarea {
                class: "form-control",
                placeholder: "Description shown to user",
                aria_label: "Description shown to user",
                aria_describedby: "workflow-integration-configuration-{P::ID_PART}-parameter-{props.parameter_idx}-description",
                value: "{props.parameter.read().desc()}",
                oninput: move |event| {
                    let mut param = props.parameter.write();
                    *param.desc_mut() = event.value();
                }
            }
        }
    }
}
