use std::fmt::Debug;
use std::path::PathBuf;

use dioxus::prelude::*;
use macworp_core::{entities::Role, paths::REFERENCE_PREFIX, responses::ProjectResponse};

use crate::storage::IsStorage;

#[derive(Clone, PartialEq, Props)]
pub struct CreateFolderProps {
    pub project: Signal<ProjectResponse>,
    /// The folder belongs to the global reference data, not to a project
    #[props(default)]
    pub reference: bool,
    pub reload_content_callback: Callback<(), ()>,
}

#[derive(Clone, PartialEq, Props)]
pub struct DeletePathProps {
    pub project: Signal<ProjectResponse>,
    /// The path belongs to the global reference data, not to a project
    #[props(default)]
    pub reference: bool,
    pub path: PathBuf,
    pub reload_content_callback: Callback<(), ()>,
}

/// Defines a type of file browser.
///
pub trait IsFileBrowser {
    type Storage: IsStorage;
    type Error: std::error::Error + Debug + Sync + Send + 'static;

    #[allow(non_upper_case_globals)]
    const CreateFolder: Component<CreateFolderProps>;

    #[allow(non_upper_case_globals)]
    const PathDelete: Component<DeletePathProps>;
}

#[derive(Clone, PartialEq, Props)]
pub struct FileBrowserProps {
    pub project: Signal<ProjectResponse>,
    /// The listing is the global reference data, not a project
    #[props(default)]
    pub reference: bool,
    /// Callback to call once moved into a folder, which should trigger a content reload
    pub move_into_folder_callback: Callback<PathBuf, ()>,
    /// Callback for reloding file contente, once a folder was created
    pub reload_content_callback: Callback<(), ()>,
}

/// Lists the folders and files of the current path. At the top level of a project the virtual
/// reference data folder is always shown first, marked with the DNA icon, described, and without
/// a delete button.
#[component]
pub fn FileBrowser<F: IsFileBrowser>(props: FileBrowserProps) -> Element {
    // Callback for goint one level up if browser is not already at top level
    let go_up_callback = move |_| {
        if !props.project.read().current.as_os_str().is_empty() {
            props.move_into_folder_callback.call(
                props
                    .project
                    .read()
                    .current
                    .parent()
                    .map(PathBuf::from)
                    .unwrap_or_default(),
            );
        }
    };

    // Viewers may only browse: creating and deleting need at least the editor role
    // The virtual reference folder inside a project is read-only for everybody
    let can_edit = !props.project.read().read_only
        && props
            .project
            .read()
            .my_role
            .is_some_and(|role| role >= Role::Editor);

    // The virtual reference folder is shown at the top level of a project, always first, with its
    // own icon and without a delete button
    let has_reference_folder = !props.reference
        && props.project.read().current.as_os_str().is_empty()
        && props
            .project
            .read()
            .directories
            .iter()
            .any(|folder| folder == REFERENCE_PREFIX);

    rsx! {
        if can_edit {
            F::CreateFolder{
                project: props.project,
                reference: props.reference,
                reload_content_callback: props.reload_content_callback,
            }
        }
        ul {
            class: "list-group",
            li {
                class: "list-group-item cursor-pointer",
                onclick: go_up_callback,
                "{props.project.read().current.display()}/",
                i {
                    class: "fas fa-turn-up ms-2"
                }
            }
            if has_reference_folder {
                li {
                    class: "list-group-item cursor-pointer d-flex",
                    span {
                        class: "d-flex align-items-center flex-fill",
                        onclick: move |_| {
                            props.move_into_folder_callback.call(PathBuf::from(REFERENCE_PREFIX));
                        },
                        i {
                            class: "fa-solid fa-dna me-2"
                        }
                        "{REFERENCE_PREFIX}"
                        small {
                            class: "text-muted ms-3",
                            "Shared reference data, e.g. genomes, proteomes, annotations, ... (read-only, managed by administrator)"
                        }
                    }
                }
            }
            for folder in props.project.read().directories.iter().filter(|folder| !(has_reference_folder && folder.as_str() == REFERENCE_PREFIX)).cloned() {
                li {
                    class: "list-group-item cursor-pointer d-flex",
                    span {
                        class: "d-flex align-items-center flex-fill",
                        onclick: move |_| {
                            props.move_into_folder_callback.call(props.project.read().current.join(&folder));
                        },
                        i {
                            class: "fas fa-folder me-2"
                        }
                        "{folder}"
                    }
                    if can_edit {
                        F::PathDelete {
                            project: props.project,
                            reference: props.reference,
                            path: props.project.read().current.join(&folder),
                            reload_content_callback: props.reload_content_callback,
                        }
                    }

                }
            }
            for file in props.project.read().files.iter() {
                li {
                    class: "list-group-item d-flex",
                    span {
                        class: "d-flex align-items-center flex-fill",
                        i {
                            class: "fas fa-file me-2"
                        }
                        "{file}"
                    }
                    if can_edit {
                        F::PathDelete {
                            project: props.project,
                            reference: props.reference,
                            path: props.project.read().current.join(file),
                            reload_content_callback: props.reload_content_callback,
                        }
                    }
                }
            }
        }
    }
}
