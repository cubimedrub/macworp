use std::path::PathBuf;

use dioxus::prelude::*;
use macworp_core::workflow_configuration::parameters::{
    parameter::IsParameterWithValue, Parameter,
};

use crate::{
    client::Client,
    components::workflow_integrations::parameter::IsParameter as IsParameterElement,
    configuration::Configuration, storage::IsStorage,
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
    let storage = use_context::<Signal<S>>();
    let config = use_context::<Signal<Configuration>>();

    let mut is_picking = use_signal(|| false);
    let mut subpath = use_signal(PathBuf::new);

    let listing = use_resource(move || async move {
        if !*is_picking.read() {
            return None;
        }
        let jwt = storage.read().jwt().ok().flatten()?;
        let config = config.read().clone();
        let subpath = subpath.read().clone();
        Some(
            Client::new(config.base_url().to_string(), config.timeout())
                .projects()
                .project(&jwt, props.project_id, subpath)
                .await,
        )
    });

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
    let mut toggle = move |path: PathBuf| {
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
            match &*listing.read() {
                Some(Some(Ok(project))) => rsx! {
                    ul {
                        class: "list-group mb-3",
                        li {
                            class: "list-group-item cursor-pointer",
                            onclick: {
                                let parent = project.current.parent().map(PathBuf::from).unwrap_or_default();
                                let at_top = project.current.as_os_str().is_empty();
                                move |_| if !at_top { subpath.set(parent.clone()) }
                            },
                            "{project.current.display()}/",
                            i { class: "fas fa-turn-up ms-2" }
                        }
                        for folder in project.directories.iter().cloned() {
                            {
                                let path = project.current.join(&folder);
                                let is_selected = selected.contains(&path);
                                let nav_path = path.clone();
                                let allowed = patterns.accepts_folder(&path);
                                rsx! {
                                    li {
                                        class: "list-group-item d-flex",
                                        key: "dir-{folder}",
                                        if accepts_folders {
                                            input {
                                                r#type: "checkbox",
                                                class: "form-check-input me-2",
                                                checked: is_selected,
                                                disabled: !allowed && !is_selected,
                                                title: if allowed { "" } else { "Does not match the allowed folder pattern" },
                                                onchange: move |_| toggle(path.clone()),
                                            }
                                        }
                                        span {
                                            class: "flex-fill cursor-pointer",
                                            onclick: move |_| subpath.set(nav_path.clone()),
                                            i { class: "fas fa-folder me-2" }
                                            "{folder}"
                                        }
                                    }
                                }
                            }
                        }
                        for file in project.files.iter().cloned() {
                            {
                                let path = project.current.join(&file);
                                let is_selected = selected.contains(&path);
                                let allowed = patterns.accepts_file(&path);
                                rsx! {
                                    li {
                                        class: if allowed { "list-group-item d-flex" } else { "list-group-item d-flex text-muted" },
                                        key: "file-{file}",
                                        if accepts_files {
                                            input {
                                                r#type: "checkbox",
                                                class: "form-check-input me-2",
                                                checked: is_selected,
                                                disabled: !allowed && !is_selected,
                                                title: if allowed { "" } else { "Does not match the allowed file pattern" },
                                                onchange: move |_| toggle(path.clone()),
                                            }
                                        }
                                        span {
                                            class: "flex-fill",
                                            i { class: "fas fa-file me-2" }
                                            "{file}"
                                        }
                                    }
                                }
                            }
                        }
                    }
                },
                Some(Some(Err(err))) => rsx! {
                    div { class: "alert alert-danger", "Failed to list project files: {err}" }
                },
                _ => rsx! { div { class: "text-muted", "Loading..." } },
            }
        }
    }
}
