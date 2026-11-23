use dioxus::prelude::*;
use macworp_core::workflow_configuration::parameters::{
    parameter::IsParameterWithValue, Parameter,
};

use crate::components::workflow_integrations::{
    parameter::IsParameter as IsParameterElement, NumericKind,
};

#[derive(Clone, PartialEq, Props)]
pub struct NumericProps {
    pub parameter: Signal<Parameter>,
    pub parameter_idx: usize,
}

#[component]
pub fn NumericParameter<P: IsParameterElement + 'static, T: NumericKind>(
    mut props: NumericProps,
) -> Element {
    let param = props.parameter.read();
    match T::from_parameter(&param) {
        Some(num_param) => rsx! {
            div {
                class: "input-group",
                span {
                    class: "input-group-text",
                    id: "workflow-integration-configuration-{P::ID_PART}-parameter-{props.parameter_idx}-value",
                    "{param.label()}"
                }
                input {
                    r#type: "number",
                    class: "form-control",
                    placeholder: T::placeholder(),
                    aria_label: T::placeholder(),
                    aria_describedby: "workflow-integration-configuration-{P::ID_PART}-parameter-{props.parameter_idx}-value",
                    min: num_param.min().map(|v| v.to_string()).unwrap_or_default(),
                    max: num_param.max().map(|v| v.to_string()).unwrap_or_default(),
                    value: "{num_param.value().map(|v| v.to_string()).unwrap_or_default()}",
                    oninput: move |evt| {
                        if let Ok(value) = evt.value().parse::<T>() {
                            let mut param = props.parameter.write();
                            if let Some(num_param) = T::from_parameter_mut(&mut param){
                                *num_param.value_mut() = Some(value);
                            }
                        }
                    }
                }
            }
            small {
                class: "form-text text-muted mb-3",
                "{param.desc()}"
            }
        },
        None => rsx! {
            div {
                class: "alert alert-danger",
                "Unsupported parameter type"
            }
        },
    }
}
