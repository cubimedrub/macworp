use std::path::PathBuf;

use dioxus::prelude::*;
use macworp_ui_core::{
    client::Client,
    components::file_browser::{
        CreateFolderProps, DeletePathProps, DownloadPathProps, IsFileBrowser,
    },
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
    /// The browser refused to open the download link
    #[error("Unable to start the download")]
    Navigation,
}

/// Mutable version of the file browser offers uploads,
/// file/folder deletion and folder creation
pub struct FileBrowser;

impl IsFileBrowser for FileBrowser {
    type Storage = Storage;
    type Error = Error;

    const CreateFolder: Component<CreateFolderProps> = |props| {
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

            let client = Client::new(config.base_url().to_string(), config.timeout());
            let files = if props.reference {
                client.reference_files()
            } else {
                client.projects()
            };
            files
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

    const PathDelete: Component<DeletePathProps> = |props| {
        let storage = use_context::<Signal<Self::Storage>>();
        let config = use_context::<Signal<Configuration>>();
        let mut is_deleting_path = use_signal(|| false);

        let mut delete_path_action = use_action(move |project_id: i64, path: PathBuf| async move {
            is_deleting_path.set(true);

            let jwt = storage
                .read()
                .jwt()
                .map_err(Error::Storage)?
                .ok_or(Error::NotLoggedIn)?;

            let config = config.read().clone();

            let client = Client::new(config.base_url().to_string(), config.timeout());
            let files = if props.reference {
                client.reference_files()
            } else {
                client.projects()
            };
            files
                .delete_path(&jwt, project_id, path)
                .await
                .map_err(Error::Request)?;

            is_deleting_path.set(false);

            props.reload_content_callback.call(());

            Ok::<(), Error>(())
        });
        rsx! {
            button {
                class: "btn btn-sm btn-outline-danger",
                r#type: "button",
                id: "path-delete-button",
                onclick: move |event| event.prevent_default(),
                ondoubleclick: move |_| delete_path_action.call(props.project.read().id, props.path.clone()),
                disabled: is_deleting_path,
                i {
                    class: "fas fa-trash me-1",
                }
                "(2x click)"
            }
        }
    };

    const PathDownload: Component<DownloadPathProps> = |props| {
        let storage = use_context::<Signal<Self::Storage>>();
        let config = use_context::<Signal<Configuration>>();

        // The browser follows the download link itself and cannot send the JWT, so a one-time
        // token that is only valid for this request goes into the URL. The response is saved by
        // the browser as it arrives; the page stays where it is.
        let mut download_action =
            use_action(move |id: i64, path: PathBuf, reference: bool| async move {
                let jwt = storage
                    .read()
                    .jwt()
                    .map_err(Error::Storage)?
                    .ok_or(Error::NotLoggedIn)?;
                let config = config.read().clone();

                let client = Client::new(config.base_url().to_string(), config.timeout());
                let token = client.users().one_time_token(&jwt).await?;
                let files = if reference {
                    client.reference_files()
                } else {
                    client.projects()
                };
                let url = files.download_url(id, &path, &token)?;
                web_sys::window()
                    .ok_or(Error::Navigation)?
                    .location()
                    .set_href(&url)
                    .map_err(|_| Error::Navigation)?;
                Ok::<(), Error>(())
            });

        // A button group takes only buttons, so a failure shows on the button itself
        let failure = download_action
            .value()
            .and_then(|result| result.err().map(|error| error.to_string()));
        rsx! {
            button {
                class: if failure.is_some() { "btn btn-sm btn-outline-danger" } else { "btn btn-sm btn-outline-secondary" },
                r#type: "button",
                title: failure.unwrap_or_else(|| "Download (folders as zip)".to_string()),
                onclick: move |_| download_action.call(props.project.read().id, props.path.clone(), props.reference),
                i {
                    class: "fas fa-download",
                }
            }
        }
    };
}
