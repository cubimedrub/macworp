use dioxus::prelude::*;
use macworp_core::workflow_configuration::parameters::parameter::IsParameterWithValue;
use macworp_core::workflow_configuration::parameters::Parameter;

use crate::components::workflow_integrations::parameter::IsParameter as IsParameterElement;

#[derive(Clone, PartialEq, Props)]
pub struct TextProps {
    pub parameter: Signal<Parameter>,
    pub parameter_idx: usize,
}

#[component]
pub fn TextParameter<P: IsParameterElement + 'static>(mut props: TextProps) -> Element {
    match &*props.parameter.read() {
        Parameter::Text(text_param) => rsx! {
            div {
                class: "input-group",
                span {
                    class: "input-group-text",
                    id: "workflow-integration-configuration-{P::ID_PART}-parameter-{props.parameter_idx}-value",
                    "{props.parameter.read().label()}"
                }
                if text_param.is_multiline() {
                    textarea {
                        class: "form-control",
                        placeholder: "Multiline text parameter",
                        aria_label: "Multiline text parameter",
                        aria_describedby: "workflow-integration-configuration-{P::ID_PART}-parameter-{props.parameter_idx}-value",
                        value: "{text_param.value().map(|v| v.to_string()).unwrap_or_default()}",
                        oninput: move |e| {
                            let mut param = props.parameter.write();
                            if let Parameter::Text(text_param) = &mut *param {
                                *text_param.value_mut() = Some(e.value());
                            }
                        }
                    }
                } else {
                    input {
                        r#type: "text",
                        class: "form-control",
                        placeholder: "Text parameter",
                        aria_label: "Text taramteter",
                        aria_describedby: "workflow-integration-configuration-{P::ID_PART}-parameter-{props.parameter_idx}-value",
                        value: "{text_param.value().map(|v| v.to_string()).unwrap_or_default()}",
                        oninput: move |e| {
                            let mut param = props.parameter.write();
                            if let Parameter::Text(text_param) = &mut *param {
                                *text_param.value_mut() = Some(e.value());
                            }
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
