use dioxus::prelude::*;
use macworp_core::workflow_configuration::parameters::{
    parameter::IsParameterWithValue, Parameter,
};

use crate::components::workflow_integrations::parameter::{
    IsParameter as IsParameterElement, Parameter as ParameterElement,
};

#[derive(Clone, PartialEq, Props)]
pub struct FloatProps {
    pub parameter: Signal<Parameter>,
    pub parameter_idx: usize,
}

#[component]
pub fn FloatParameter<P: IsParameterElement + 'static>(props: FloatProps) -> Element {
    rsx! {
        match &*props.parameter.read() {
            Parameter::Float(float_param) => rsx!{
                ParameterElement<P> {
                    parameter: props.parameter,
                    parameter_idx: props.parameter_idx,
                }
                div {
                    class: "input-group mb-3",
                    span {
                        class: "input-group-text",
                        id: "workflow-integration-configuration-{P::ID_PART}-parameter-{props.parameter_idx}-value",
                        "Value"
                    }
                    input {
                        type: "number",
                        class: "form-control",
                        placeholder: "Initial float value",
                        aria_label: "Initial float value",
                        aria_describedby: "workflow-integration-configuration-{P::ID_PART}-parameter-{props.parameter_idx}-value",
                        min: float_param.min().map(|limit| limit.to_string()).unwrap_or_default(),
                        max: float_param.max().map(|limit| limit.to_string()).unwrap_or_default(),
                        value: "{float_param.value().map(|limit| limit.to_string()).unwrap_or_default()}",
                        // oninput: move |event| if let Source::Remote(source) = workflow_integration.write().configuration_mut().source_mut() {
                        //     *source.version_mut() = if event.value().is_empty() { None } else { Some(event.value()) }
                        // }
                    }
                }
                div {
                    class: "input-group mb-3",
                    span {
                        class: "input-group-text",
                        id: "workflow-integration-configuration-{P::ID_PART}-parameter-{props.parameter_idx}-min",
                        "Min"
                    }
                    input {
                        type: "number",
                        class: "form-control",
                        placeholder: "Miniumum value",
                        aria_label: "Miniumum value",
                        aria_describedby: "workflow-integration-configuration-{P::ID_PART}-parameter-{props.parameter_idx}-min",
                        min: float_param.min().map(|limit| limit.to_string()).unwrap_or_default(),
                        max: float_param.max().map(|limit| limit.to_string()).unwrap_or_default(),
                        value: "{float_param.min().map(|limit| limit.to_string()).unwrap_or_default()}",
                        // oninput: move |event| if let Source::Remote(source) = workflow_integration.write().configuration_mut().source_mut() {
                        //     *source.version_mut() = if event.value().is_empty() { None } else { Some(event.value()) }
                        // }
                    }
                }

                div {
                    class: "input-group mb-3",
                    span {
                        class: "input-group-text",
                        id: "workflow-integration-configuration-{P::ID_PART}-parameter-{props.parameter_idx}-max",
                        "Max"
                    }
                    input {
                        type: "number",
                        class: "form-control",
                        placeholder: "Maximum value",
                        aria_label: "Maximum value",
                        aria_describedby: "workflow-integration-configuration-{P::ID_PART}-parameter-{props.parameter_idx}-max",
                        min: float_param.min().map(|limit| limit.to_string()).unwrap_or_default(),
                        max: float_param.max().map(|limit| limit.to_string()).unwrap_or_default(),
                        value: "{float_param.max().map(|limit| limit.to_string()).unwrap_or_default()}",
                        // oninput: move |event| if let Source::Remote(source) = workflow_integration.write().configuration_mut().source_mut() {
                        //     *source.version_mut() = if event.value().is_empty() { None } else { Some(event.value()) }
                        // }
                    }
                }


            },
             _ => rsx!(
                div {
                    class: "alert alert-danger",
                    "Unsupported parameter type"
                }
            )
        }
    }
}
