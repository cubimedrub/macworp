use std::{collections::VecDeque, future::Future, path::PathBuf, rc::Rc};

use dioxus::prelude::*;
use macworp_core::{entities::Role, requests::ParameterValue, responses::ProjectResponse};
use thiserror::Error;

use crate::{
    client::{Client, IsFileUpload, IsUploadableFile, ShareKind},
    components::{
        file_browser::IsFileBrowser, projects::workflow_start_dialog::WorkflowStartDialog,
        runs::RunPanel, sharing::share_dialog::ShareDialog, ConfirmationDialog, FileBrowser,
        Spinner,
    },
    configuration::Configuration,
    router::IsRouter,
    sharing::SharingEnabled,
    storage::IsStorage,
};

/// Trait defining the uplaod behavior
pub trait IsProjectEdit {
    type Error: std::error::Error + std::fmt::Debug + Sync + Send + 'static;
    type File: IsUploadableFile + 'static;
    type FileUpload: IsFileUpload<File = Self::File>;
    type FileBrowser: IsFileBrowser<Storage = Self::Storage> + 'static;
    type Storage: IsStorage + 'static;
    type Router: IsRouter + 'static;

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

    /// Resolves once the content of a local directory changed. Platforms without local
    /// access keep the default, which never resolves.
    fn wait_for_change(_path: PathBuf) -> impl Future<Output = Result<(), Self::Error>> {
        std::future::pending()
    }

    /// Reveals a project directory in the native file explorer. Only called if the backend
    /// reported a local path, i.e. not on web.
    fn open_in_file_explorer(_path: PathBuf) -> impl Future<Output = Result<(), Self::Error>> {
        async { Ok(()) }
    }
}

#[derive(Debug, Error)]
pub enum Error<S: IsStorage> {
    #[error("Config not loaded")]
    ConfigNotLoaded,
    #[error("Failed to fetch projects: {0}")]
    FetchProject(crate::client::Error),
    #[error("Failed to delete projects: {0}")]
    DeleteProject(crate::client::Error),
    #[error("Failed to start workflow: {0}")]
    StartRun(crate::client::Error),
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
pub fn Edit<T: IsProjectEdit + 'static>(props: EditProps) -> Element {
    let storage = use_context::<Signal<T::Storage>>();
    let config = use_context::<Signal<Configuration>>();
    let navigator = navigator();

    let mut subpath: Signal<PathBuf> = use_signal(PathBuf::new);
    let mut is_delete_confirmation_dialog_open = use_signal(|| false);
    let mut is_workflow_start_dialog_open = use_signal(|| false);
    let mut is_share_dialog_open = use_signal(|| false);
    let sharing = SharingEnabled::from_context().is_enabled();
    let mut upload_queue = use_signal(VecDeque::<Result<T::File, T::Error>>::new);
    let mut current_upload: Signal<Option<String>> = use_signal(|| None);
    let mut upload_progress = use_signal(|| 0.0_f64);
    // Counts started runs, tells the run panel to reload
    let mut run_refresh = use_signal(|| 0_u32);

    let mut start_run = use_action(
        move |(workflow_id, parameters): (i64, Vec<ParameterValue>)| async move {
            let jwt = storage
                .read()
                .jwt()
                .map_err(Error::Storage)?
                .ok_or(Error::NotLoggedIn)?;

            let config = config.read().clone();

            let run = Client::new(config.base_url().to_string(), config.timeout())
                .runs()
                .start(&jwt, props.id, workflow_id, String::new(), parameters)
                .await
                .map_err(Error::<T::Storage>::StartRun)?;
            *run_refresh.write() += 1;
            Ok::<_, Error<T::Storage>>(run)
        },
    );

    let mut open_explorer =
        use_action(move |path: PathBuf| async move { T::open_in_file_explorer(path).await });

    let window_file_drop_handling_result = use_resource(T::prevent_window_file_drop_handling);

    let mut project_res: Resource<Result<Signal<ProjectResponse>, Error<T::Storage>>> =
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

    // Reload the listing whenever the shown local directory changes. The resource restarts
    // itself with every reload, which re-arms the watcher.
    let _watcher = use_resource(move || async move {
        let target = match &*project_res.read() {
            Some(Ok(project)) => project
                .read()
                .local_path
                .as_ref()
                .map(|path| path.join(&project.read().current)),
            _ => None,
        };
        let Some(target) = target else { return };
        match T::wait_for_change(target).await {
            Ok(()) => project_res.restart(),
            Err(err) => tracing::warn!("Unable to watch project folder: {err:?}"),
        }
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

        navigator.push(T::Router::project_index_route());

        Ok::<_, Error<T::Storage>>(())
    });

    let mut upload_task: Resource<std::result::Result<(), Error<T::Storage>>> =
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

                let file_destination = file.parent().map(PathBuf::from);

                client
                    .projects()
                    .upload_file::<T::FileUpload>(
                        &jwt,
                        props.id,
                        file,
                        config.upload_buffer_size(),
                        upload_progress,
                    )
                    .await
                    .map_err(Error::Upload)?;

                // Refresh content if we're still in the same path
                if let Some(destination) = file_destination.as_ref() {
                    if destination == &*subpath.read() {
                        project_res.restart();
                    }
                }

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
                // Dropped files are uploads, which need at least the editor role
                let can_edit = project_res
                    .read()
                    .as_ref()
                    .and_then(|project| project.as_ref().ok())
                    .is_some_and(|project| project.read().my_role.is_some_and(|role| role >= Role::Editor));
                if !can_edit {
                    return;
                }
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
            WorkflowStartDialog::<T::Storage> {
                project_id: props.id,
                is_open: is_workflow_start_dialog_open,
                cancel_callback: Callback::new(move |_| is_workflow_start_dialog_open.set(false)),
                start_callback: Callback::new(move |(workflow_id, parameters): (i64, Vec<ParameterValue>)| {
                    is_workflow_start_dialog_open.set(false);
                    start_run.call((workflow_id, parameters));
                })
            }
            if sharing {
                ShareDialog::<T::Storage> {
                    kind: ShareKind::Project,
                    resource_id: props.id,
                    is_open: is_share_dialog_open,
                    cancel_callback: Callback::new(move |_| is_share_dialog_open.set(false)),
                }
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
            match &*project_res.read() {
                Some(Ok(project)) => rsx!(
                    div {
                        class: "logo d-flex flex-row justify-content-between mb-3",
                        h1 {
                            class: "mb-0",
                            "{project.read().name}"
                        }
                        div {
                            class: "d-flex flex-row",
                            if project.read().my_role.is_some_and(|role| role >= Role::Editor) {
                                button {
                                    class: "btn btn-sm btn-outline-primary me-2",
                                    title: "Start workflow",
                                    onclick: move |_| is_workflow_start_dialog_open.set(true),
                                    i {
                                        class: "fa-solid fa-play me-1"
                                    }
                                    "Start workflow"
                                }
                            }
                            button {
                                class: "btn btn-sm btn-outline-secondary me-2",
                                title: "Reload file list",
                                onclick: move |_| project_res.restart(),
                                i {
                                    class: "fa-solid fa-rotate me-1"
                                }
                                "Reload"
                            }
                            if let Some(target) = project.read().local_path.as_ref().map(|path| path.join(&project.read().current)) {
                                button {
                                    class: "btn btn-sm btn-outline-secondary me-2",
                                    title: "Open in file explorer",
                                    onclick: move |_| {
                                        open_explorer.call(target.clone());
                                    },
                                    i {
                                        class: "fa-solid fa-folder-open me-1"
                                    }
                                    "Open folder"
                                }
                            }
                            if project.read().my_role == Some(Role::Owner) {
                                if sharing {
                                    button {
                                        id: "share-btn",
                                        class: "btn btn-sm btn-outline-secondary me-2",
                                        title: "Share project",
                                        onclick: move |_| is_share_dialog_open.set(true),
                                        i {
                                            class: "fa-solid fa-share-nodes me-1"
                                        }
                                        "Share"
                                    }
                                }
                                button {
                                    id: "delete-btn",
                                    class: "btn btn-sm btn-outline-danger",
                                    title: "Delete project",
                                    onclick: move |_| is_delete_confirmation_dialog_open.set(true),
                                    i {
                                        class: "fa-solid fa-trash me-1"
                                    }
                                    "Delete"
                                }
                            }
                        }
                    }
                    match start_run.value() {
                        Some(Ok(_)) => rsx! {},
                        Some(Err(err)) => rsx! {
                            div {
                                class: "alert alert-danger",
                                role: "alert",
                                "{err}"
                            }
                        },
                        None => rsx! {}
                    }
                    match open_explorer.value() {
                        Some(Err(err)) => rsx! {
                            div {
                                class: "alert alert-danger",
                                role: "alert",
                                "Failed to open file explorer: {err}"
                            }
                        },
                        _ => rsx! {}
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
                    RunPanel::<T::Storage> {
                        project_id: props.id,
                        refresh: run_refresh,
                    }
                    FileBrowser<T::FileBrowser> {
                        project: *project,
                        move_into_folder_callback: Callback::new(move |new_subpath| {
                            subpath.set(new_subpath);
                        }),
                        reload_content_callback: Callback::new(move |_| {
                            project_res.restart();
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
                    Spinner {}
                )
            }
        }
    }
}
