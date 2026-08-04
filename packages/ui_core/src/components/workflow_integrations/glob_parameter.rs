use dioxus::prelude::*;
use macworp_core::workflow_configuration::parameters::Parameter;

use crate::components::workflow_integrations::parameter::{
    IsParameter as IsParameterElement, Parameter as ParameterElement,
};

#[derive(Clone, PartialEq, Props)]
pub struct GlobProps {
    pub parameter: Signal<Parameter>,
    pub parameter_idx: usize,
}

#[component]
pub fn GlobParameter<P: IsParameterElement + 'static>(mut props: GlobProps) -> Element {
    match &*props.parameter.read() {
        Parameter::Glob(glob_param) => rsx! {
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
                    r#type: "text",
                    class: "form-control",
                    placeholder: "Glob pattern",
                    aria_label: "Glob pattern",
                    aria_describedby: "workflow-integration-configuration-{P::ID_PART}-parameter-{props.parameter_idx}-value",
                    value: "{glob_param.value().map(|v| v.to_string()).unwrap_or_default()}",
                    oninput: move |e| {
                        let mut param = props.parameter.write();
                        if let Parameter::Glob(glob_param) = &mut *param {
                            *glob_param.value_mut() = Some(e.value());
                        }
                    }
                }
            }
        },
        _ => rsx! {
            div {
                class: "alert alert-danger",
                "Unsupported parameter type"
            }
        },
    }
}
