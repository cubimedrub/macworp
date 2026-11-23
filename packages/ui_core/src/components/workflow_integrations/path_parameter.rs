use std::num::NonZeroUsize;

use dioxus::prelude::*;
use macworp_core::workflow_configuration::parameters::Parameter;

use crate::{
    components::workflow_integrations::parameter::{
        IsParameter as IsParameterElement, Parameter as ParameterElement,
    },
    storage::IsStorage,
};

#[derive(Clone, PartialEq, Props)]
pub struct PathProps {
    pub parameter: Signal<Parameter>,
    pub parameter_idx: usize,
}

#[component]
pub fn PathParameter<P: IsParameterElement + 'static, S: IsStorage + 'static>(
    mut props: PathProps,
) -> Element {
    match &*props.parameter.read() {
        Parameter::Path(path_param) => rsx! {
            ParameterElement<P> {
                parameter: props.parameter,
                parameter_idx: props.parameter_idx,
            }
            div {
                class: "input-group mb-3",
                span {
                    class: "input-group-text",
                    id: "workflow-integration-configuration-{P::ID_PART}-parameter-{props.parameter_idx}-value",
                    "Limit"
                }
                input {
                    r#type: "number",
                    class: "form-control",
                    placeholder: "Number of selectable files/folders",
                    aria_label: "Number of selectable files/folders",
                    aria_describedby: "workflow-integration-configuration-{P::ID_PART}-parameter-{props.parameter_idx}-limit",
                    min: "1",
                    value: "{path_param.limit()}",
                    oninput: move |evt| {
                        let mut param = props.parameter.write();
                        if let Parameter::Path(path_param) = &mut *param {
                            *path_param.limit_mut() = evt.value().parse::<NonZeroUsize>().unwrap_or(NonZeroUsize::new(1).unwrap());
                        }
                    }
                }
            }
            div {
                class: "form-check form-switch",
                label {
                    class: "form-check-label",
                    for: "workflow-integration-configuration-{P::ID_PART}-parameter-{props.parameter_idx}-is-excepting-files",
                    "Is excepting files"
                }
                input {
                    r#type: "checkbox",
                    class: "form-check-input",
                    id: "workflow-integration-configuration-{P::ID_PART}-parameter-{props.parameter_idx}-is-excepting-files",
                    checked: "{ path_param.is_excepting_files() }",
                    oninput: move |evt| {
                        let mut param = props.parameter.write();
                        if let Parameter::Path(path_param) = &mut *param {
                            *path_param.is_excepting_files_mut() = evt.checked();
                        }
                    }
                }
            }
            div {
                class: "form-check form-switch",
                label {
                    class: "form-check-label",
                    for: "workflow-integration-configuration-{P::ID_PART}-parameter-{props.parameter_idx}-is-excepting-volders",
                    "Is excepting folders"
                }
                input {
                    r#type: "checkbox",
                    class: "form-check-input",
                    id: "workflow-integration-configuration-{P::ID_PART}-parameter-{props.parameter_idx}-is-excepting-volders",
                    checked: "{ path_param.is_excepting_folders() }",
                    oninput: move |evt| {
                        let mut param = props.parameter.write();
                        if let Parameter::Path(path_param) = &mut *param {
                            *path_param.is_excepting_folders_mut() = evt.checked();
                        }
                    }
                }
            }
            div {
                class: "input-group mt-2",
                span { class: "input-group-text", "File pattern" }
                input {
                    r#type: "text",
                    class: "form-control",
                    placeholder: r"Regular expression for the whole path, e.g. .*\.(fastq|fq)(\.gz)?",
                    aria_label: "Regular expression selectable files must match",
                    id: "workflow-integration-configuration-{P::ID_PART}-parameter-{props.parameter_idx}-file-pattern",
                    value: "{path_param.file_pattern().unwrap_or_default()}",
                    oninput: move |evt| {
                        let mut param = props.parameter.write();
                        if let Parameter::Path(path_param) = &mut *param {
                            path_param.set_file_pattern(&evt.value());
                        }
                    }
                }
            }
            div {
                class: "input-group mt-2",
                span { class: "input-group-text", "Folder pattern" }
                input {
                    r#type: "text",
                    class: "form-control",
                    placeholder: "Regular expression for the whole path, e.g. data/.*",
                    aria_label: "Regular expression selectable folders must match",
                    id: "workflow-integration-configuration-{P::ID_PART}-parameter-{props.parameter_idx}-folder-pattern",
                    value: "{path_param.folder_pattern().unwrap_or_default()}",
                    oninput: move |evt| {
                        let mut param = props.parameter.write();
                        if let Parameter::Path(path_param) = &mut *param {
                            path_param.set_folder_pattern(&evt.value());
                        }
                    }
                }
            }
            if let Err(message) = path_param.validate_patterns() {
                div { class: "text-danger small mb-2", "{message}" }
            }
            div {
                class: "input-group mb-3",
                span {
                    class: "input-group-text",
                    id: "workflow-integration-configuration-{P::ID_PART}-parameter-{props.parameter_idx}-value",
                    "Value"
                }
                if path_param.limit().get() > 1 {
                    textarea {
                        class: "form-control",
                        placeholder: "Default paths (one per line)",
                        aria_label: "Default paths (one per line)",
                        aria_describedby: "workflow-integration-configuration-{P::ID_PART}-parameter-{props.parameter_idx}-value",
                        value: "{path_param.value_as_string()}",
                        oninput: move |evt| {
                            let mut param = props.parameter.write();
                            if let Parameter::Path(path_param) = &mut *param {
                                path_param.value_from_string(evt.value())
                            }
                        }
                    }
                } else {
                    input {
                        r#type: "text",
                        class: "form-control",
                        placeholder: "Default path",
                        aria_label: "Default path",
                        aria_describedby: "workflow-integration-configuration-{P::ID_PART}-parameter-{props.parameter_idx}-value",
                        value: "{path_param.value_as_string()}",
                        oninput: move |evt| {
                            let mut param = props.parameter.write();
                            if let Parameter::Path(path_param) = &mut *param {
                                path_param.value_from_string(evt.value())
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
