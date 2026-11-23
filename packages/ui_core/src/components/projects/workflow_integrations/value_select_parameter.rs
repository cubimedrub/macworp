use dioxus::prelude::*;
use macworp_core::workflow_configuration::parameters::{
    parameter::IsParameterWithValue, Parameter,
};

use crate::components::workflow_integrations::parameter::IsParameter as IsParameterElement;

#[derive(Clone, PartialEq, Props)]
pub struct ValueSelectProps {
    pub parameter: Signal<Parameter>,
    pub parameter_idx: usize,
}

/// Choice of one option (dropdown) or several (checkboxes) in the start dialog
#[component]
pub fn ValueSelectParameter<P: IsParameterElement + 'static>(
    mut props: ValueSelectProps,
) -> Element {
    match &*props.parameter.read() {
        Parameter::ValueSelect(select) => rsx! {
            div {
                class: "input-group",
                span {
                    class: "input-group-text",
                    id: "workflow-integration-configuration-{P::ID_PART}-parameter-{props.parameter_idx}-value",
                    "{props.parameter.read().label()}"
                }
                if select.is_multiselect() {
                    div {
                        class: "form-control",
                        for option in select.options().iter() {
                            {
                                let value = option.value.clone();
                                rsx! {
                                    div {
                                        class: "form-check",
                                        key: "{value}",
                                        input {
                                            r#type: "checkbox",
                                            class: "form-check-input",
                                            checked: select.value().is_some_and(|v| v.contains(&value)),
                                            onchange: move |evt| {
                                                if let Parameter::ValueSelect(select) = &mut *props.parameter.write() {
                                                    let mut values = select.value().cloned().unwrap_or_default();
                                                    values.retain(|v| *v != value);
                                                    if evt.checked() {
                                                        values.push(value.clone());
                                                    }
                                                    *select.value_mut() = if values.is_empty() { None } else { Some(values) };
                                                }
                                            }
                                        }
                                        label { class: "form-check-label", "{option.label}" }
                                    }
                                }
                            }
                        }
                    }
                } else {
                    select {
                        class: "form-select",
                        aria_label: "{props.parameter.read().label()}",
                        aria_describedby: "workflow-integration-configuration-{P::ID_PART}-parameter-{props.parameter_idx}-value",
                        onchange: move |evt| {
                            if let Parameter::ValueSelect(select) = &mut *props.parameter.write() {
                                let value = evt.value();
                                *select.value_mut() = if value.is_empty() { None } else { Some(vec![value]) };
                            }
                        },
                        option {
                            value: "",
                            disabled: true,
                            selected: select.value().is_none(),
                            "Select..."
                        }
                        for option in select.options().iter() {
                            option {
                                value: "{option.value}",
                                selected: select.value().is_some_and(|v| v.contains(&option.value)),
                                "{option.label}"
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
