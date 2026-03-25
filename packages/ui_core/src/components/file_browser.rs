use std::fmt::Debug;
use std::path::PathBuf;

use dioxus::prelude::*;
use macworp_core::responses::ProjectResponse;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {}

/// Defines a type of file browser.
///
pub trait IsFileBrowser {
    type Error: std::error::Error + Debug + Sync + Send + 'static;
}

/// Immutable file browser for desktop use where the user is supposed
/// to use the native file explorer
pub struct ImmutableFileBrowser;

impl IsFileBrowser for ImmutableFileBrowser {
    type Error = Error;
}

/// Mutable version of the file browser offers uploads,
/// file/folder deletion and folder creation
pub struct MutableFileBrowser;

impl IsFileBrowser for MutableFileBrowser {
    type Error = Error;
}

#[derive(Clone, PartialEq, Props)]
pub struct FileBrowserProps {
    pub project: Signal<ProjectResponse>,
    pub move_into_folder_callback: Callback<PathBuf, ()>,
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
                    class: "list-group-item cursor-pointer",
                    onclick: move |_| {
                        props.move_into_folder_callback.call(props.project.read().current.join(&folder));
                    },
                    i {
                        class: "fas fa-folder me-2"
                    }
                    "{folder}"
                }
            }
            for file in props.project.read().files.iter() {
                li {
                    class: "list-group-item",
                    i {
                        class: "fas fa-file me-2"
                    }
                    "{file}"
                }
            }
        }
    }
}
