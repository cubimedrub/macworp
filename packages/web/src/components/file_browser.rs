use dioxus::prelude::*;
use macworp_ui_core::{
    client::Client,
    components::file_browser::{CreateFolderProps, IsFileBrowser},
    configuration::Configuration,
    storage::IsStorage,
};
use thiserror::Error;

use crate::storage::Storage;

#[derive(Debug, Error)]
pub enum Error {
    #[error("{0}")]
    Storage(<Storage as IsStorage>::Error),
    #[error("Not logged in")]
    NotLoggedIn,
    #[error("{0}")]
    Request(#[from] macworp_ui_core::client::Error),
    #[error("Folder name cannot be empty")]
    EmptyFolderName,
}

/// Mutable version of the file browser offers uploads,
/// file/folder deletion and folder creation
pub struct FileBrowser;

impl IsFileBrowser for FileBrowser {
    type Storage = Storage;
    type Error = Error;

    const CreateFolder: Component<CreateFolderProps> = |props: CreateFolderProps| {
        let storage = use_context::<Signal<Self::Storage>>();
        let config = use_context::<Signal<Configuration>>();
        let mut folder_name = use_signal(String::new);
        let mut is_creating_folder = use_signal(|| false);

        let mut create_folder_action = use_action(move || async move {
            is_creating_folder.set(true);
            let folder_path = if !folder_name.is_empty() {
                props
                    .project
                    .read()
                    .current
                    .join(folder_name.read().as_str())
            } else {
                is_creating_folder.set(false);
                return Err(Error::EmptyFolderName);
            };

            let jwt = storage
                .read()
                .jwt()
                .map_err(Error::Storage)?
                .ok_or(Error::NotLoggedIn)?;

            let config = config.read().clone();
            let id = props.project.read().id;

            Client::new(config.base_url().to_string(), config.timeout())
                .projects()
                .create_folder(&jwt, id, folder_path)
                .await
                .map_err(Error::Request)?;

            is_creating_folder.set(false);

            folder_name.set(String::new());
            props.reload_content_callback.call(());

            Ok(())
        });

        rsx! {
            div {
                class: "row",
                div {
                    class: "col col-md-6 offset-md-6 col-lg-4 offset-lg-8 col-xl-2 offset-xl-10 d-flex justify-content-end",
                    div {
                        class: "input-group mb-3",
                        input {
                            r#type: "text",
                            class: "form-control",
                            placeholder: "Name for new folder",
                            aria_label: "Name for new folder",
                            aria_describedby: "folder-create-button",
                            disabled: is_creating_folder,
                            onchange: move |evt| folder_name.set(evt.value()),
                            onkeyup: move |event| {
                                if event.code() == Code::Enter {
                                    create_folder_action.call();
                                }
                            },
                            value: "{folder_name}"

                        }
                        button {
                            class: "btn btn-outline-secondary",
                            r#type: "button",
                            id: "folder-create-button",
                            onclick: move |_| create_folder_action.call(),
                            "create"
                        }
                    }
                }
            }
        }
    };
}
