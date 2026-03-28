use std::{collections::VecDeque, future::Future, path::PathBuf, rc::Rc};

use dioxus::prelude::*;
use macworp_core::responses::ProjectResponse;
use thiserror::Error;

use crate::{
    client::{Client, IsFileUpload, IsUploadableFile},
    components::{file_browser::IsFileBrowser, ConfirmationDialog, FileBrowser, Spinner},
    configuration::Configuration,
    router::IsRouter,
    storage::IsStorage,
};

/// Trait defining the uplaod behavior
pub trait IsProjectEdit {
    type Error: std::error::Error + std::fmt::Debug + Sync + Send + 'static;
    type File: IsUploadableFile + 'static;
    type FileUpload: IsFileUpload<File = Self::File>;

    /// Prevent dropped file handling by window
    fn prevent_window_file_drop_handling() -> impl Future<Output = Result<(), Self::Error>>;

    /// File drop event
    ///
    /// # Arguments
    /// * `event` - The file drop event
    /// * `target_paht` - The selected path within the project when the event occured
    /// * `queue` - The queue to enqueue the file entries into. Errors will also be enqueued as Err variants.
    #[allow(clippy::type_complexity)]
    fn drop_handler(
        event: Event<DragData>,
        target_path: Rc<PathBuf>,
    ) -> impl Future<Output = Result<Vec<Result<Self::File, Self::Error>>, Self::Error>>;

    /// File drop event
    fn dragover_handler(event: Event<DragData>) -> impl Future<Output = Result<(), Self::Error>>;
}

#[derive(Debug, Error)]
pub enum Error<S: IsStorage> {
    #[error("Config not loaded")]
    ConfigNotLoaded,
    #[error("Failed to fetch projects: {0}")]
    FetchProject(crate::client::Error),
    #[error("Failed to delete projects: {0}")]
    DeleteProject(crate::client::Error),
    #[error("Failed to upload file: {0}")]
    Upload(crate::client::Error),
    #[error("Not logged in")]
    NotLoggedIn,
    #[error("{0}")]
    Storage(S::Error),
    #[error("Unable to reqd upload queue")]
    PoisenedUploadQueue,
}

#[derive(Clone, PartialEq, Props)]
pub struct EditProps {
    /// Project ID to fetch
    pub id: i64,
}

#[component]
pub fn Edit<
    T: IsProjectEdit + 'static,
    F: IsFileBrowser + 'static,
    S: IsStorage + 'static,
    R: IsRouter,
>(
    props: EditProps,
) -> Element {
    let storage = use_context::<Signal<S>>();
    let config = use_context::<Signal<Configuration>>();
    let navigator = navigator();

    let mut subpath: Signal<PathBuf> = use_signal(PathBuf::new);
    let mut is_delete_confirmation_dialog_open = use_signal(|| false);
    let mut upload_queue = use_signal(VecDeque::<Result<T::File, T::Error>>::new);
    let mut current_upload: Signal<Option<String>> = use_signal(|| None);
    let mut upload_progress = use_signal(|| 0.0_f64);

    let window_file_drop_handling_result = use_resource(T::prevent_window_file_drop_handling);

    let project: Resource<Result<Signal<ProjectResponse>, Error<S>>> =
        use_resource(move || async move {
            let jwt = storage
                .read()
                .jwt()
                .map_err(Error::Storage)?
                .ok_or(Error::NotLoggedIn)?;

            let subpath = subpath.read().clone();

            let config = config.read().clone();

            let project = Client::new(config.base_url().to_string(), config.timeout())
                .projects()
                .project(&jwt, props.id, subpath)
                .await
                .map_err(Error::FetchProject)?;

            Ok(Signal::new(project))
        });

    let mut delete_project = use_action(move || async move {
        let jwt = storage
            .read()
            .jwt()
            .map_err(Error::Storage)?
            .ok_or(Error::NotLoggedIn)?;

        let config = config.read().clone();

        Client::new(config.base_url().to_string(), config.timeout())
            .projects()
            .delete(&jwt, props.id)
            .await
            .map_err(Error::DeleteProject)?;

        navigator.push(R::project_index_route());

        Ok::<_, Error<S>>(())
    });

    let mut upload_task: Resource<std::result::Result<(), Error<S>>> =
        use_resource(move || async move {
            tracing::info!("Starting file upload for project {id}", id = props.id);

            let jwt = storage
                .read()
                .jwt()
                .map_err(Error::Storage)?
                .ok_or(Error::NotLoggedIn)?;

            let config = config.read().clone();

            let client = Client::new(config.base_url().to_string(), config.timeout());

            loop {
                let file = match upload_queue.write().pop_front() {
                    Some(Ok(file)) => file,
                    Some(Err(err)) => {
                        tracing::error!("Failed to read file from upload queue: {err:?}");
                        continue;
                    }
                    None => break,
                };

                current_upload.set(Some(format!("{file}")));

                client
                    .projects()
                    .upload_file::<T::FileUpload>(&jwt, props.id, file, upload_progress)
                    .await
                    .map_err(Error::Upload)?;

                current_upload.set(None);
                upload_progress.set(0.0);
            }
            tracing::info!("Upload done!");
            Ok(())
        });

    rsx! {
        div {
            class: "h-min-100",
            ondrop: move |event| async move {
                let current_path = Rc::new(subpath.read().clone());
                match T::drop_handler(event, current_path).await {
                    Ok(uploadable_files) => {
                        upload_queue.write().extend(uploadable_files);
                        upload_task.restart();
                    },
                    Err(err) => tracing::error!("File drop handler error: {err:?}")
                }
            },
            ondragover: move |event| async move {
                let res = T::dragover_handler(event).await;
                if let Err(res) = res {
                    tracing::error!("File dragover handler error: {res:?}");
                }
            },
            ConfirmationDialog {
                is_open: is_delete_confirmation_dialog_open,
                title: "Delete project".to_string(),
                body: "Are you sure you want to delete this project? This action cannot be undone.".to_string(),
                confirmation_button_label: "Delete".to_string(),
                confirmation_callback: Callback::new(move |_| {
                    is_delete_confirmation_dialog_open.set(false);
                    delete_project.call();
                }),
                cancel_callback: Callback::new(move |_| is_delete_confirmation_dialog_open.set(false))
            }
            match &*window_file_drop_handling_result.read() {
                Some(Ok(_)) => rsx! {},
                Some(Err(err)) => rsx! {
                    div {
                        class: "alert alert-danger",
                        role: "alert",
                        "Failed to prevent window file drop handling: {err}"
                    }
                },
                None => rsx! {}
            }
            match &*project.read() {
                Some(Ok(project)) => rsx!(
                    div {
                        class: "logo d-flex flex-row justify-content-between mb-3",
                        h1 {
                            class: "mb-0",
                            "{project.read().name}"
                        }
                        button {
                            id: "collapse-btn",
                            class: "btn btn-sm btn-outline-danger",
                            title: "Collapse sidebar",
                            onclick: move |_| is_delete_confirmation_dialog_open.set(true),
                            i {
                                class: "fa-solid fa-trash me-1"
                            }
                            "Delete"
                        }
                    }
                    match delete_project.value() {
                        Some(Err(err)) => rsx! {
                            div {
                                class: "alert alert-danger",
                                role: "alert",
                                "Failed to delete project: {err}" }
                        },
                        _ => rsx! {}
                    }
                    FileBrowser<F> {
                        project: *project,
                        move_into_folder_callback: Callback::new(move |new_subpath| {
                            subpath.set(new_subpath);
                        })
                    }
                    if !upload_queue.read().is_empty() || !current_upload.read().is_none() {
                        ul {
                            class: "list-group mt-3",
                            if let Some(current_upload) = &*current_upload.read() {
                                li {
                                    class: "list-group-item d-flex align-items-center",
                                    span {
                                        class: "me-3",
                                        "{current_upload}"
                                    }
                                    div {
                                        class: "progress flex-fill",
                                        role: "progressbar",
                                        aria_label: "Basic example",
                                        aria_valuenow: "{upload_progress}",
                                        aria_valuemin: "0",
                                        aria_valuemax: "100",
                                        div {
                                            class: "progress-bar",
                                            style: "width: {upload_progress}%"
                                        }
                                    }
                                }
                            }
                            for file in upload_queue.read().iter() {
                                match file {
                                    Ok(file) => rsx!{
                                        li {
                                            class: "list-group-item d-flex align-items-center",
                                            i {
                                                class: "fa-solid fa-file me-2"
                                            }
                                            "{file}"
                                        }
                                    },
                                    Err(err) => rsx!{
                                        li {
                                            class: "list-group-item d-flex align-items-center list-group-item-danger",
                                            i {
                                                class: "fa-solid fa-triangle-exclamation me-2"
                                            }
                                            "Failed to read file: {err}"
                                        }
                                    }
                                }
                            }
                        }
                    }
                ),
                Some(Err(err)) => rsx!(
                    div {
                        class: "alert alert-danger",
                        "{err}"
                    }
                ),
                None => rsx!(
                    if project.pending() {
                        Spinner {}
                    }
                )
            }
        }
    }
}
