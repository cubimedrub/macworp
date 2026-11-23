use dioxus::prelude::*;
use macworp_core::workflow_configuration::parameters::Parameter;

use crate::components::workflow_integrations::parameter::IsParameter as IsParameterElement;

#[derive(Clone, PartialEq, Props)]
pub struct GroupProps {
    pub parameter: Signal<Parameter>,
    pub parameter_idx: usize,
}

/// Editor for a group header, which has only a label and a description (no name, no value)
#[component]
pub fn GroupParameter<P: IsParameterElement + 'static>(mut props: GroupProps) -> Element {
    rsx! {
        div {
            class: "input-group mb-3",
            span {
                class: "input-group-text",
                id: "workflow-integration-configuration-{P::ID_PART}-parameter-{props.parameter_idx}-label",
                "Group label"
            }
            input {
                r#type: "text",
                class: "form-control",
                placeholder: "Title of the group",
                aria_label: "Group label",
                aria_describedby: "workflow-integration-configuration-{P::ID_PART}-parameter-{props.parameter_idx}-label",
                value: "{props.parameter.read().label()}",
                oninput: move |event| {
                    *props.parameter.write().label_mut() = event.value();
                }
            }
        }
        div {
            class: "input-group",
            span {
                class: "input-group-text",
                "Description"
            }
            input {
                r#type: "text",
                class: "form-control",
                placeholder: "Optional text shown under the title",
                aria_label: "Group description",
                value: "{props.parameter.read().desc()}",
                oninput: move |event| {
                    *props.parameter.write().desc_mut() = event.value();
                }
            }
        }
    }
}
