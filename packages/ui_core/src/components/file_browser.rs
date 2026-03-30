use std::fmt::Debug;
use std::path::PathBuf;

use dioxus::prelude::*;
use macworp_core::responses::ProjectResponse;

use crate::storage::IsStorage;

#[derive(Clone, PartialEq, Props)]
pub struct CreateFolderProps {
    pub project: Signal<ProjectResponse>,
    pub reload_content_callback: Callback<(), ()>,
}

#[derive(Clone, PartialEq, Props)]
pub struct DeletePathProps {
    pub project: Signal<ProjectResponse>,
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
    /// Callback to call once moved into a folder, which should trigger a content reload
    pub move_into_folder_callback: Callback<PathBuf, ()>,
    /// Callback for reloding file contente, once a folder was created
    pub reload_content_callback: Callback<(), ()>,
}

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

    rsx! {
        F::CreateFolder{
            project: props.project,
            reload_content_callback: props.reload_content_callback,
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
            for folder in props.project.read().directories.iter().cloned() {
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
                    F::PathDelete {
                        project: props.project,
                        path: props.project.read().current.join(&folder),
                        reload_content_callback: props.reload_content_callback,
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
                    F::PathDelete {
                        project: props.project,
                        path: props.project.read().current.join(file),
                        reload_content_callback: props.reload_content_callback,
                    }
                }
            }
        }
    }
}
