use dioxus::prelude::*;
use macworp_core::workflow_configuration::parameters::Parameter;

use crate::components::workflow_integrations::parameter::IsParameter as IsParameterElement;

#[derive(Clone, PartialEq, Props)]
pub struct GlobProps {
    pub parameter: Signal<Parameter>,
    pub parameter_idx: usize,
}

#[component]
pub fn GlobParameter<P: IsParameterElement + 'static>(mut props: GlobProps) -> Element {
    match &*props.parameter.read() {
        Parameter::Glob(glob_param) => rsx! {
            div {
                class: "input-group",
                span {
                    class: "input-group-text",
                    id: "workflow-integration-configuration-{P::ID_PART}-parameter-{props.parameter_idx}-value",
                    "{props.parameter.read().label()}"
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
                small {
                    class: "form-text text-muted mb-3",
                    "{props.parameter.read().desc()}"
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
