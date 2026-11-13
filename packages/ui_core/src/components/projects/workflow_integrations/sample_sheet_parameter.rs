use dioxus::prelude::*;
use macworp_core::workflow_configuration::parameters::{
    parameter::IsParameterWithValue, Parameter,
};

use crate::components::workflow_integrations::parameter::IsParameter as IsParameterElement;

#[derive(Clone, PartialEq, Props)]
pub struct SampleSheetProps {
    pub parameter: Signal<Parameter>,
    pub parameter_idx: usize,
}

/// STUB: placeholder for the sample sheet editor in the start dialog. Shows a static notice and a
/// text input for the path of the sheet; the full editor replaces the notice later.
#[component]
pub fn SampleSheetParameter<P: IsParameterElement + 'static>(
    mut props: SampleSheetProps,
) -> Element {
    match &*props.parameter.read() {
        Parameter::SampleSheet(sheet) => rsx! {
            div {
                class: "border rounded p-3 mb-2 text-muted text-center",
                i { class: "fas fa-table me-2" }
                "Sample sheet editor (not implemented yet)"
            }
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
