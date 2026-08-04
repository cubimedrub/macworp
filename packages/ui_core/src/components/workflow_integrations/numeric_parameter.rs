use std::{fmt::Display, str::FromStr};

use dioxus::prelude::*;
use macworp_core::workflow_configuration::parameters::{
    number::Number, parameter::IsParameterWithValue, Parameter,
};
use serde::{de::DeserializeOwned, Serialize};

use crate::components::workflow_integrations::parameter::{
    IsParameter as IsParameterElement, Parameter as ParameterElement,
};

pub trait NumericKind:
    FromStr + num_traits::Num + Serialize + DeserializeOwned + Clone + Copy + Display + 'static
{
    fn from_parameter(p: &Parameter) -> Option<&Number<Self>>;
    fn from_parameter_mut(p: &mut Parameter) -> Option<&mut Number<Self>>;
    fn placeholder() -> &'static str;
}

impl NumericKind for f64 {
    fn from_parameter(p: &Parameter) -> Option<&Number<Self>> {
        if let Parameter::Float(f) = p {
            Some(f)
        } else {
            None
        }
    }

    fn from_parameter_mut(p: &mut Parameter) -> Option<&mut Number<Self>> {
        if let Parameter::Float(f) = p {
            Some(f)
        } else {
            None
        }
    }

    fn placeholder() -> &'static str {
        "Float value"
    }
}

impl NumericKind for i64 {
    fn from_parameter(p: &Parameter) -> Option<&Number<Self>> {
        if let Parameter::Integer(i) = p {
            Some(i)
        } else {
            None
        }
    }

    fn from_parameter_mut(p: &mut Parameter) -> Option<&mut Number<Self>> {
        if let Parameter::Integer(i) = p {
            Some(i)
        } else {
            None
        }
    }

    fn placeholder() -> &'static str {
        "Integer value"
    }
}

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
            div {
                class: "input-group mb-3",
                span {
                    class: "input-group-text",
                    id: "workflow-integration-configuration-{P::ID_PART}-parameter-{props.parameter_idx}-min",
                    "Min"
                }
                input {
                    r#type: "number",
                    class: "form-control",
                    placeholder: "Minimum value",
                    aria_label: "Minimum value",
                    aria_describedby: "workflow-integration-configuration-{P::ID_PART}-parameter-{props.parameter_idx}-min",
                    min: num_param.min().map(|v| v.to_string()).unwrap_or_default(),
                    max: num_param.max().map(|v| v.to_string()).unwrap_or_default(),
                    value: "{num_param.min().map(|v| v.to_string()).unwrap_or_default()}",
                    oninput: move |evt| {
                        if let Ok(value) = evt.value().parse::<T>() {
                            let mut param = props.parameter.write();
                            if let Some(num_param) = T::from_parameter_mut(&mut param){
                                *num_param.min_mut() = Some(value);
                            }
                        }
                    }
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
                    r#type: "number",
                    class: "form-control",
                    placeholder: "Maximum value",
                    aria_label: "Maximum value",
                    aria_describedby: "workflow-integration-configuration-{P::ID_PART}-parameter-{props.parameter_idx}-max",
                    min: num_param.min().map(|v| v.to_string()).unwrap_or_default(),
                    max: num_param.max().map(|v| v.to_string()).unwrap_or_default(),
                    value: "{num_param.max().map(|v| v.to_string()).unwrap_or_default()}",
                    oninput: move |evt| {
                        if let Ok(value) = evt.value().parse::<T>() {
                            let mut param = props.parameter.write();
                            if let Some(num_param) = T::from_parameter_mut(&mut param){
                                *num_param.max_mut() = Some(value);
                            }
                        }
                    }
                }
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
