use dioxus::prelude::*;
use macworp_core::workflow_configuration::parameters::Parameter;

use crate::components::workflow_integrations::parameter::IsParameter as IsParameterElement;

#[derive(Clone, PartialEq, Props)]
pub struct SeparatorProps {
    pub parameter: Signal<Parameter>,
    pub parameter_idx: usize,
}

/// Editor for a separator, which has only a label and a description (no name, no value)
#[component]
pub fn SeparatorParameter<P: IsParameterElement + 'static>(mut props: SeparatorProps) -> Element {
    rsx! {
        div {
            class: "input-group mb-3",
            span {
                class: "input-group-text",
                id: "workflow-integration-configuration-{P::ID_PART}-parameter-{props.parameter_idx}-label",
                "Separator label"
            }
            input {
                r#type: "text",
                class: "form-control",
                placeholder: "Label shown above the following parameters",
                aria_label: "Separator label",
                aria_describedby: "workflow-integration-configuration-{P::ID_PART}-parameter-{props.parameter_idx}-label",
                value: "{props.parameter.read().label()}",
                oninput: move |event| {
                    *props.parameter.write().label_mut() = event.value();
                }
            }
        }
    }
}
