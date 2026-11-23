use std::path::PathBuf;

use dioxus::prelude::*;

use crate::{client::Client, configuration::Configuration, storage::IsStorage};

/// Props of [`ProjectFilePicker`].
#[derive(Clone, PartialEq, Props)]
pub struct ProjectFilePickerProps {
    /// Project whose files are listed
    pub project_id: i64,
    /// Paths shown as selected (project relative)
    pub selected: Vec<PathBuf>,
    /// Whether files can be picked (otherwise they are only listed)
    pub accepts_files: bool,
    /// Whether folders can be picked (they can always be opened)
    pub accepts_folders: bool,
    /// Whether the folder being browsed (the project root at first) can be picked itself, with
    /// an extra entry at the top of the list
    #[props(default = false)]
    pub accepts_current: bool,
    /// Whether a file may be picked, e.g. to grey out names a pattern excludes. Defaults to yes.
    #[props(default = Callback::new(|_| true))]
    pub file_allowed: Callback<PathBuf, bool>,
    /// Whether a folder may be picked. Defaults to yes.
    #[props(default = Callback::new(|_| true))]
    pub folder_allowed: Callback<PathBuf, bool>,
    /// Called with the project relative path when a checkbox is toggled
    pub on_toggle: EventHandler<PathBuf>,
}

/// Inline browser over the files and folders of a project: navigate into folders, tick entries
/// to pick them. The caller owns the selection (`selected`, `on_toggle`) so the same list serves
/// path parameters and sample sheet cells.
#[component]
pub fn ProjectFilePicker<S: IsStorage + 'static>(props: ProjectFilePickerProps) -> Element {
    let storage = use_context::<Signal<S>>();
    let configuration = use_context::<Signal<Configuration>>();
    let mut subpath = use_signal(PathBuf::new);
    let project_id = props.project_id;

    let listing = use_resource(move || async move {
        let jwt = storage.read().jwt().ok().flatten()?;
        let configuration = configuration.read().clone();
        let subpath = subpath.read().clone();
        Some(
            Client::new(
                configuration.base_url().to_string(),
                configuration.timeout(),
            )
            .projects()
            .project(&jwt, project_id, subpath)
            .await,
        )
    });

    let rendered = match &*listing.read() {
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
                if props.accepts_current {
                    {
                        let current = project.current.clone();
                        let is_selected = props.selected.contains(&current);
                        let label = if current.as_os_str().is_empty() {
                            "Project root".to_string()
                        } else {
                            format!("This folder ({}/)", current.display())
                        };
                        rsx! {
                            li {
                                class: "list-group-item d-flex",
                                input {
                                    r#type: "checkbox",
                                    class: "form-check-input me-2",
                                    checked: is_selected,
                                    onchange: move |_| props.on_toggle.call(current.clone()),
                                }
                                span {
                                    class: "flex-fill",
                                    i { class: "fas fa-folder-open me-2" }
                                    "{label}"
                                }
                            }
                        }
                    }
                }
                for folder in project.directories.iter().cloned() {
                    {
                        let path = project.current.join(&folder);
                        let is_selected = props.selected.contains(&path);
                        let navigate_path = path.clone();
                        let allowed = props.folder_allowed.call(path.clone());
                        rsx! {
                            li {
                                class: "list-group-item d-flex",
                                key: "dir-{folder}",
                                if props.accepts_folders {
                                    input {
                                        r#type: "checkbox",
                                        class: "form-check-input me-2",
                                        checked: is_selected,
                                        disabled: !allowed && !is_selected,
                                        title: if allowed { "" } else { "Does not match the allowed folder pattern" },
                                        onchange: move |_| props.on_toggle.call(path.clone()),
                                    }
                                }
                                span {
                                    class: "flex-fill cursor-pointer",
                                    onclick: move |_| subpath.set(navigate_path.clone()),
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
                        let is_selected = props.selected.contains(&path);
                        let allowed = props.file_allowed.call(path.clone());
                        rsx! {
                            li {
                                class: if allowed { "list-group-item d-flex" } else { "list-group-item d-flex text-muted" },
                                key: "file-{file}",
                                if props.accepts_files {
                                    input {
                                        r#type: "checkbox",
                                        class: "form-check-input me-2",
                                        checked: is_selected,
                                        disabled: !allowed && !is_selected,
                                        title: if allowed { "" } else { "Does not match the allowed file pattern" },
                                        onchange: move |_| props.on_toggle.call(path.clone()),
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
        Some(Some(Err(error))) => rsx! {
            div { class: "alert alert-danger", "Failed to list project files: {error}" }
        },
        _ => rsx! { div { class: "text-muted", "Loading..." } },
    };
    rendered
}
