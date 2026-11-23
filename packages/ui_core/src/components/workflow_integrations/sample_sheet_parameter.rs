use dioxus::prelude::*;
use macworp_core::workflow_configuration::parameters::{
    parameter::IsParameterWithValue, Parameter,
};

use crate::components::workflow_integrations::parameter::{
    IsParameter as IsParameterElement, Parameter as ParameterElement,
};

#[derive(Clone, PartialEq, Props)]
pub struct SampleSheetProps {
    pub parameter: Signal<Parameter>,
    pub parameter_idx: usize,
}

/// STUB: placeholder for the sample sheet editor. Shows a static notice and a text input for the
/// path of the sheet. The full editor (table generated from the schema, file picker, pairing
/// helpers, import/export, validation) replaces the notice later.
#[component]
pub fn SampleSheetParameter<P: IsParameterElement + 'static>(
    mut props: SampleSheetProps,
) -> Element {
    match &*props.parameter.read() {
        Parameter::SampleSheet(sheet) => rsx! {
            ParameterElement<P> {
                parameter: props.parameter,
                parameter_idx: props.parameter_idx,
            }
            div {
                class: "border rounded p-3 mb-2 text-muted text-center",
                i { class: "fas fa-table me-2" }
                "Sample sheet editor (not implemented yet)"
            }
            div {
                class: "input-group mb-3",
                span {
                    class: "input-group-text",
                    id: "workflow-integration-configuration-{P::ID_PART}-parameter-{props.parameter_idx}-value",
                    "Sheet path"
                }
                input {
                    r#type: "text",
                    class: "form-control",
                    placeholder: "Path of the sample sheet in the project",
                    aria_label: "Sample sheet path",
                    aria_describedby: "workflow-integration-configuration-{P::ID_PART}-parameter-{props.parameter_idx}-value",
                    value: "{sheet.value().map(|v| v.to_string()).unwrap_or_default()}",
                    oninput: move |e| {
                        let mut param = props.parameter.write();
                        if let Parameter::SampleSheet(sheet) = &mut *param {
                            *sheet.value_mut() = Some(e.value());
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
