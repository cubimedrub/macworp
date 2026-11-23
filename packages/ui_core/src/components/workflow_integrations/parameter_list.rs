use dioxus::prelude::*;
use macworp_core::workflow_configuration::parameters::Parameter;
use strum::IntoEnumIterator;

use crate::{
    components::workflow_integrations::{parameter::IsParameter, *},
    parameter_arrangement::{self, Direction, ParameterMove},
    storage::IsStorage,
};

#[derive(Clone, PartialEq, Props)]
pub struct ParameterListProps {
    pub parameters: Vec<Signal<Parameter>>,
    pub on_remove: EventHandler<usize>,
    pub on_add: EventHandler<Parameter>,
    /// Reorder or move-to-other-list requested by the user, applied by the owner of the lists
    pub on_move: EventHandler<ParameterMove>,
}

/// Editable list of parameter definitions plus an "add parameter" row, for dynamic or fixed
/// parameters depending on `P`
#[component]
pub fn ParameterList<P: IsParameter + 'static, S: IsStorage + 'static>(
    props: ParameterListProps,
) -> Element {
    let mut to_add = use_signal(Parameter::default);
    // Index of the card being dragged by its handle
    let mut dragged = use_signal(|| None::<usize>);
    let other_list = if P::ID_PART == "dynamic" {
        "fixed"
    } else {
        "dynamic"
    };
    let last = props.parameters.len().saturating_sub(1);
    // Groups (by their header parameter) that are folded; keyed by signal so reordering keeps it
    let mut collapsed = use_signal(Vec::<Signal<Parameter>>::new);
    let sections = parameter_arrangement::sections(&props.parameters, |parameter| {
        matches!(*parameter.read(), Parameter::Group(_))
    });

    rsx! {
        for section in sections {
            {
                let header = matches!(&*props.parameters[section.start].read(), Parameter::Group(_))
                    .then(|| props.parameters[section.start]);
                let is_collapsed = header.is_some_and(|header| collapsed.read().contains(&header));
                let cards_start = if header.is_some() { section.start + 1 } else { section.start };
                let count = section.end - cards_start;
                rsx! {
                    div {
                        class: if header.is_some() { "card border-primary mb-4" } else { "mb-2" },
                        key: "{section.start}",
                        if let Some(header) = header {
                            div {
                                class: "card-header d-flex align-items-center",
                                ondragover: move |event| {
                                    if dragged.read().is_some() {
                                        event.prevent_default();
                                    }
                                },
                                ondrop: move |event| {
                                    event.prevent_default();
                                    if let Some(from) = dragged.write().take() {
                                        props.on_move.call(ParameterMove::Drop { from, target: section.start });
                                    }
                                },
                                span {
                                    class: "me-2 text-muted",
                                    style: "cursor: grab",
                                    draggable: "true",
                                    title: "Drag to move the group with its parameters",
                                    ondragstart: move |_| dragged.set(Some(section.start)),
                                    ondragend: move |_| dragged.set(None),
                                    i { class: "fas fa-grip-vertical" }
                                }
                                button {
                                    class: "btn btn-sm btn-link text-decoration-none me-2",
                                    r#type: "button",
                                    title: if is_collapsed { "Expand group" } else { "Collapse group" },
                                    onclick: move |event| {
                                        event.prevent_default();
                                        let mut folded = collapsed.write();
                                        if let Some(position) = folded.iter().position(|other| *other == header) {
                                            folded.remove(position);
                                        } else {
                                            folded.push(header);
                                        }
                                    },
                                    i { class: if is_collapsed { "fas fa-chevron-right" } else { "fas fa-chevron-down" } }
                                }
                                div {
                                    class: "flex-grow-1 me-2",
                                    GroupParameter<P> { parameter: header, parameter_idx: section.start }
                                }
                                if is_collapsed {
                                    span { class: "badge text-bg-secondary me-2", "{count} parameter(s)" }
                                }
                                button {
                                    class: "btn btn-outline-secondary btn-sm me-1",
                                    r#type: "button",
                                    title: "Move group up",
                                    disabled: section.start == 0,
                                    onclick: move |event| {
                                        event.prevent_default();
                                        props.on_move.call(ParameterMove::Step(section.start, Direction::Up));
                                    },
                                    i { class: "fas fa-arrow-up" }
                                }
                                button {
                                    class: "btn btn-outline-secondary btn-sm me-1",
                                    r#type: "button",
                                    title: "Move group down",
                                    disabled: section.end > last,
                                    onclick: move |event| {
                                        event.prevent_default();
                                        props.on_move.call(ParameterMove::Step(section.start, Direction::Down));
                                    },
                                    i { class: "fas fa-arrow-down" }
                                }
                                button {
                                    class: "btn btn-outline-secondary btn-sm me-1",
                                    r#type: "button",
                                    title: "Move the group with its parameters to the {other_list} parameters",
                                    onclick: move |event| {
                                        event.prevent_default();
                                        props.on_move.call(ParameterMove::ToOtherList(section.start));
                                    },
                                    i { class: "fas fa-right-left me-1" }
                                    "{other_list}"
                                }
                                button {
                                    class: "btn btn-outline-danger btn-sm",
                                    r#type: "button",
                                    title: "Remove the group header (its parameters are kept)",
                                    onclick: move |event| {
                                        event.prevent_default();
                                        props.on_remove.call(section.start);
                                    },
                                    i { class: "fas fa-trash" }
                                }
                            }
                        }
                        if !is_collapsed {
                            div {
                                class: if header.is_some() { "card-body" } else { "" },
                                div {
                                    class: "row row-cols-1 row-cols-xl-2 g-4",
                                    for parameter_idx in cards_start..section.end {
                                        {
                                            let parameter = props.parameters[parameter_idx];
                                            rsx! {
                                                div {
                                                    class: "col",
                                                    key: "{parameter_idx}",
                                                    ondragover: move |event| {
                                                        if dragged.read().is_some() {
                                                            event.prevent_default();
                                                        }
                                                    },
                                                    ondrop: move |event| {
                                                        event.prevent_default();
                                                        if let Some(from) = dragged.write().take() {
                                                            props.on_move.call(ParameterMove::Drop { from, target: parameter_idx });
                                                        }
                                                    },
                                                    div {
                                                        class: "card",
                                                        div {
                                                            class: "card-header d-flex align-items-center py-1",
                                                            span {
                                                                class: "me-2 text-muted",
                                                                style: "cursor: grab",
                                                                draggable: "true",
                                                                title: "Drag to reorder",
                                                                ondragstart: move |_| dragged.set(Some(parameter_idx)),
                                                                ondragend: move |_| dragged.set(None),
                                                                i { class: "fas fa-grip-vertical" }
                                                            }
                                                            span { class: "me-auto" }
                                                            button {
                                                                class: "btn btn-outline-secondary btn-sm me-1",
                                                                r#type: "button",
                                                                title: "Move up",
                                                                disabled: parameter_idx == 0,
                                                                onclick: move |event| {
                                                                    event.prevent_default();
                                                                    props.on_move.call(ParameterMove::Step(parameter_idx, Direction::Up));
                                                                },
                                                                i { class: "fas fa-arrow-up" }
                                                            }
                                                            button {
                                                                class: "btn btn-outline-secondary btn-sm me-1",
                                                                r#type: "button",
                                                                title: "Move down",
                                                                disabled: parameter_idx == last,
                                                                onclick: move |event| {
                                                                    event.prevent_default();
                                                                    props.on_move.call(ParameterMove::Step(parameter_idx, Direction::Down));
                                                                },
                                                                i { class: "fas fa-arrow-down" }
                                                            }
                                                            button {
                                                                class: "btn btn-outline-secondary btn-sm",
                                                                r#type: "button",
                                                                title: "Move to the {other_list} parameters",
                                                                onclick: move |event| {
                                                                    event.prevent_default();
                                                                    props.on_move.call(ParameterMove::ToOtherList(parameter_idx));
                                                                },
                                                                i { class: "fas fa-right-left me-1" }
                                                                "{other_list}"
                                                            }
                                                        }
                                                        div {
                                                            class: "card-body",
                                                            match &*parameter.read() {
                                                                Parameter::Float(_) => rsx! {
                                                                    NumericParameter<P, f64> { parameter, parameter_idx }
                                                                },
                                                                Parameter::Glob(_) => rsx! {
                                                                    GlobParameter<P> { parameter, parameter_idx }
                                                                },
                                                                Parameter::Integer(_) => rsx! {
                                                                    NumericParameter<P, i64> { parameter, parameter_idx }
                                                                },
                                                                Parameter::Path(_) => rsx! {
                                                                    PathParameter<P, S> { parameter, parameter_idx }
                                                                },
                                                                Parameter::Text(_) => rsx! {
                                                                    TextParameter<P> { parameter, parameter_idx }
                                                                },
                                                                Parameter::ValueSelect(_) => rsx! {
                                                                    ValueSelectParameter<P> { parameter, parameter_idx }
                                                                },
                                                                Parameter::SampleSheet(_) => rsx! {
                                                                    SampleSheetParameter<P> { parameter, parameter_idx }
                                                                },
                                                                Parameter::Group(_) => rsx! {
                                                                    GroupParameter<P> { parameter, parameter_idx }
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
                                    }
                                }
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
