use dioxus::prelude::*;
use macworp_core::workflow_configuration::parameters::Parameter;
use strum::IntoEnumIterator;

use crate::{
    components::workflow_integrations::{parameter::IsParameter, *},
    storage::IsStorage,
};

#[derive(Clone, PartialEq, Props)]
pub struct ParameterListProps {
    pub parameters: Vec<Signal<Parameter>>,
    pub on_remove: EventHandler<usize>,
    pub on_add: EventHandler<Parameter>,
}

/// Editable list of parameter definitions plus an "add parameter" row, for dynamic or fixed
/// parameters depending on `P`
#[component]
pub fn ParameterList<P: IsParameter + 'static, S: IsStorage + 'static>(
    props: ParameterListProps,
) -> Element {
    let mut to_add = use_signal(Parameter::default);

    rsx! {
        div {
            class: "row row-cols-1 row-cols-xl-2 g-4 mb-2",
            for (parameter_idx, parameter) in props.parameters.iter().enumerate() {
                div {
                    class: "col",
                    key: "{parameter_idx}",
                    div {
                        class: "card",
                        div {
                            class: "card-body",
                            match &*parameter.read() {
                                Parameter::Float(_) => rsx! {
                                    NumericParameter<P, f64> { parameter: *parameter, parameter_idx }
                                },
                                Parameter::Glob(_) => rsx! {
                                    GlobParameter<P> { parameter: *parameter, parameter_idx }
                                },
                                Parameter::Integer(_) => rsx! {
                                    NumericParameter<P, i64> { parameter: *parameter, parameter_idx }
                                },
                                Parameter::Path(_) => rsx! {
                                    PathParameter<P, S> { parameter: *parameter, parameter_idx }
                                },
                                Parameter::Text(_) => rsx! {
                                    TextParameter<P> { parameter: *parameter, parameter_idx }
                                },
                                Parameter::ValueSelect(_) => rsx! {
                                    ValueSelectParameter<P> { parameter: *parameter, parameter_idx }
                                },
                                Parameter::Separator(_) => rsx! {
                                    SeparatorParameter<P> { parameter: *parameter, parameter_idx }
                                },
                            }
                            button {
                                class: "btn btn-outline-danger btn-sm ms-2",
                                r#type: "button",
                                title: "Remove parameter",
                                onclick: move |event| {
                                    event.prevent_default();
                                    props.on_remove.call(parameter_idx);
                                },
                                i { class: "fas fa-trash" }
                            }
                        }
                    }
                }
            }
        }
        div {
            class: "row",
            div {
                class: "input-group mb-3",
                span {
                    class: "input-group-text",
                    "Add new parameter"
                }
                select {
                    class: "form-select",
                    aria_label: "Paramter type to add to {P::ID_PART} parameters",
                    onchange: move |event| {
                        event.prevent_default();
                        if let Ok(param_type) = event.value().parse::<Parameter>() {
                            to_add.set(param_type);
                        }
                    },
                    for param_type in Parameter::iter() {
                        option {
                            selected: to_add.read().is_same_variant(&param_type),
                            value: "{param_type}",
                            "{param_type}"
                        }
                    }
                }
                button {
                    class: "btn btn-outline-secondary",
                    r#type: "button",
                    onclick: move |event| {
                        event.prevent_default();
                        props.on_add.call(to_add.read().clone());
                    },
                    i { class: "fas fa-plus" }
                }
            }
        }
    }
}
