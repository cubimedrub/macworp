use std::path::PathBuf;

use dioxus::prelude::*;
use macworp_core::workflow_configuration::parameters::{
    parameter::IsParameterWithValue, Parameter,
};

use crate::{
    components::{
        projects::file_picker::ProjectFilePicker,
        workflow_integrations::parameter::IsParameter as IsParameterElement,
    },
    storage::IsStorage,
};

#[derive(Clone, PartialEq, Props)]
pub struct PathProps {
    pub parameter: Signal<Parameter>,
    pub parameter_idx: usize,
    /// Project whose files can be picked
    pub project_id: i64,
}

/// Path parameter in the start dialog: shows the selection and an inline picker over the files
/// and folders of the project, honouring `limit` and the accepted kinds of the parameter.
#[component]
pub fn PathParameter<P: IsParameterElement + 'static, S: IsStorage + 'static>(
    mut props: PathProps,
) -> Element {
    let mut is_picking = use_signal(|| false);

    let Parameter::Path(path_param) = &*props.parameter.read() else {
        return rsx! {
            div {
                class: "alert alert-danger",
                "Unsupported parameter type"
            }
        };
    };
    let limit = path_param.limit().get();
    let selected: Vec<PathBuf> = path_param.value().cloned().unwrap_or_default();
    let accepts_files = path_param.is_excepting_files();
    let accepts_folders = path_param.is_excepting_folders();
    // Greys out entries the parameter's patterns exclude (the backend enforces them as well)
    let patterns = path_param.clone();

    // Toggles `path` in the selection; with limit 1 a new pick replaces the old one
    let toggle = move |path: PathBuf| {
        if let Parameter::Path(path_param) = &mut *props.parameter.write() {
            let limit = path_param.limit().get();
            let mut paths = path_param.value().cloned().unwrap_or_default();
            if let Some(pos) = paths.iter().position(|p| *p == path) {
                paths.remove(pos);
            } else if limit == 1 {
                paths = vec![path];
            } else if paths.len() < limit {
                paths.push(path);
            }
            *path_param.value_mut() = if paths.is_empty() { None } else { Some(paths) };
        }
    };

    rsx! {
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
                placeholder: "No paths selected",
                aria_label: "Selected paths",
                aria_describedby: "workflow-integration-configuration-{P::ID_PART}-parameter-{props.parameter_idx}-value",
                disabled: true,
                value: "{selected.iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join(\", \")}",
            }
            button {
                class: "btn btn-outline-secondary",
                r#type: "button",
                title: "Select from project files",
                onclick: move |_| {
                    let open = !*is_picking.read();
                    is_picking.set(open);
                },
                i { class: "fas fa-folder-open" }
            }
            button {
                class: "btn btn-outline-secondary",
                r#type: "button",
                title: "Clear selection",
                onclick: move |_| {
                    if let Parameter::Path(path_param) = &mut *props.parameter.write() {
                        *path_param.value_mut() = None;
                    }
                },
                i { class: "fas fa-xmark" }
            }
        }
        small {
            class: "form-text text-muted mb-3",
            "{props.parameter.read().desc()}"
            if limit > 1 {
                " ({selected.len()}/{limit} selected)"
            }
        }
        if *is_picking.read() {
            ProjectFilePicker::<S> {
                project_id: props.project_id,
                selected: selected.clone(),
                accepts_files,
                accepts_folders,
                file_allowed: {
                    let patterns = patterns.clone();
                    Callback::new(move |path: PathBuf| patterns.accepts_file(&path))
                },
                folder_allowed: Callback::new(move |path: PathBuf| patterns.accepts_folder(&path)),
                on_toggle: toggle,
            }
        }
    }
}
