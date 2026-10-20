use dioxus::prelude::*;
use macworp_core::workflow_configuration::parameters::{
    parameter::IsParameterWithValue, Parameter,
};

use crate::{
    components::workflow_integrations::parameter::IsParameter as IsParameterElement,
    storage::IsStorage,
};

#[derive(Clone, PartialEq, Props)]
pub struct PathProps {
    pub parameter: Signal<Parameter>,
    pub parameter_idx: usize,
}

#[component]
pub fn PathParameter<P: IsParameterElement + 'static, S: IsStorage + 'static>(
    mut props: PathProps,
) -> Element {
    match &*props.parameter.read() {
        Parameter::Path(path_param) => rsx! {
            div {
                class: "input-group",
                span {
                    class: "input-group-text",
                    id: "workflow-integration-configuration-{P::ID_PART}-parameter-{props.parameter_idx}-value",
                    "{props.parameter.read().label()}"
                }
                match path_param.value() {
                    Some(paths) => rsx! {
                        for (path_idx, path) in paths.iter().enumerate() {
                            input {
                                r#type: "text",
                                class: "form-control",
                                placeholder: "Select path parameter {path_idx}",
                                aria_label: "Select path parameter {path_idx}",
                                aria_describedby: "workflow-integration-configuration-{P::ID_PART}-parameter-{props.parameter_idx}-value",
                                disabled: true,
                                value: "{path.display()}",
                            }
                        }
                    },
                    None => rsx!{
                        div {
                            class: "alert alert-warning",
                            "No paths selected"
                        }
                    }
                }
            }
            small {
                class: "form-text text-muted mb-3",
                "{props.parameter.read().desc()}"
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
