use dioxus::prelude::*;
use macworp_core::workflow_configuration::parameters::{
    parameter::IsParameterWithValue, value_select::SelectOption, Parameter,
};

use crate::components::workflow_integrations::parameter::{
    IsParameter as IsParameterElement, Parameter as ParameterElement,
};

#[derive(Clone, PartialEq, Props)]
pub struct ValueSelectProps {
    pub parameter: Signal<Parameter>,
    pub parameter_idx: usize,
}

#[component]
pub fn ValueSelectParameter<P: IsParameterElement + 'static>(
    mut props: ValueSelectProps,
) -> Element {
    match &*props.parameter.read() {
        Parameter::ValueSelect(select) => rsx! {
            ParameterElement<P> {
                parameter: props.parameter,
                parameter_idx: props.parameter_idx,
            }
            div {
                class: "form-check form-switch",
                label {
                    class: "form-check-label",
                    for: "workflow-integration-configuration-{P::ID_PART}-parameter-{props.parameter_idx}-is-multiselect",
                    "Allow multiple selections"
                }
                input {
                    r#type: "checkbox",
                    class: "form-check-input",
                    id: "workflow-integration-configuration-{P::ID_PART}-parameter-{props.parameter_idx}-is-multiselect",
                    checked: "{select.is_multiselect()}",
                    oninput: move |evt| {
                        if let Parameter::ValueSelect(select) = &mut *props.parameter.write() {
                            *select.is_multiselect_mut() = evt.checked();
                            if !evt.checked() {
                                if let Some(v) = select.value_mut() {
                                    v.truncate(1);
                                }
                            }
                        }
                    }
                }
            }
            p { class: "mt-2 mb-1", "Options" }
            for (option_idx, option) in select.options().iter().enumerate() {
                div {
                    class: "input-group mb-2",
                    key: "{option_idx}",
                    input {
                        r#type: "text",
                        class: "form-control",
                        placeholder: "Label",
                        aria_label: "Option label",
                        value: "{option.label}",
                        oninput: move |evt| {
                            if let Parameter::ValueSelect(select) = &mut *props.parameter.write() {
                                select.options_mut()[option_idx].label = evt.value();
                            }
                        }
                    }
                    input {
                        r#type: "text",
                        class: "form-control",
                        placeholder: "Value passed to the workflow",
                        aria_label: "Option value",
                        value: "{option.value}",
                        oninput: move |evt| {
                            if let Parameter::ValueSelect(select) = &mut *props.parameter.write() {
                                let old = std::mem::replace(
                                    &mut select.options_mut()[option_idx].value,
                                    evt.value(),
                                );
                                // keep the default in sync with the renamed option
                                let new = select.options()[option_idx].value.clone();
                                if let Some(default) = select.value_mut() {
                                    for v in default.iter_mut().filter(|v| **v == old) {
                                        *v = new.clone();
                                    }
                                }
                            }
                        }
                    }
                    button {
                        class: "btn btn-outline-danger",
                        r#type: "button",
                        title: "Remove option",
                        onclick: move |evt| {
                            evt.prevent_default();
                            if let Parameter::ValueSelect(select) = &mut *props.parameter.write() {
                                let removed = select.options_mut().remove(option_idx);
                                if let Some(default) = select.value_mut() {
                                    default.retain(|v| *v != removed.value);
                                    if default.is_empty() {
                                        *select.value_mut() = None;
                                    }
                                }
                            }
                        },
                        i { class: "fas fa-trash" }
                    }
                }
            }
            button {
                class: "btn btn-outline-secondary btn-sm mb-3",
                r#type: "button",
                onclick: move |evt| {
                    evt.prevent_default();
                    if let Parameter::ValueSelect(select) = &mut *props.parameter.write() {
                        select.options_mut().push(SelectOption::default());
                    }
                },
                i { class: "fas fa-plus me-1" }
                "Add option"
            }
            if select.is_multiselect() {
                p { class: "mt-2 mb-1", "Default" }
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
                                    oninput: move |evt| {
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
            } else {
                div {
                    class: "input-group mb-3",
                    span {
                        class: "input-group-text",
                        id: "workflow-integration-configuration-{P::ID_PART}-parameter-{props.parameter_idx}-value",
                        "Default"
                    }
                    select {
                        class: "form-select",
                        aria_label: "Default selection",
                        aria_describedby: "workflow-integration-configuration-{P::ID_PART}-parameter-{props.parameter_idx}-value",
                        onchange: move |evt| {
                            if let Parameter::ValueSelect(select) = &mut *props.parameter.write() {
                                let value = evt.value();
                                *select.value_mut() = if value.is_empty() { None } else { Some(vec![value]) };
                            }
                        },
                        option {
                            value: "",
                            selected: select.value().is_none(),
                            "(none)"
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
        },
        _ => rsx! {
            div {
                class: "alert alert-danger",
                "Unsupported parameter type"
            }
        },
    }
}
