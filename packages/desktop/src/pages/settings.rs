use std::path::PathBuf;

use dioxus::prelude::*;

use crate::settings::{Folder, Settings};

/// Desktop settings: where the global reference data (genomes, indices, annotation shared by all
/// projects) and the workflow cache (fetched remote workflow repositories) live. The user picks
/// a folder with the native dialog, nothing is typed or edited in files. The choice is saved at
/// once and applies to the next run.
#[component]
pub fn SettingsPage() -> Element {
    rsx! {
        h1 { "Settings" }
        FolderSetting {
            folder: Folder::Reference,
            title: "Global reference data",
            dialog_title: "Folder with the global reference data",
            description: "Genomes, indices and annotation that every project can use in workflow parameters. \
                 Put the files into this folder with your file manager; projects see them under \
                 macworp_global_reference_data. Workflows can read the folder but never change it.",
        }
        FolderSetting {
            folder: Folder::WorkflowCache,
            title: "Workflow cache",
            dialog_title: "Folder for the workflow cache",
            description: "Remote workflows (git repositories, nf-core pipelines) are downloaded here once \
                 and reused by every run, so they also work offline. Each run uses a fixed copy \
                 of the version it started with. Must be a folder the container runtime can read. \
                 Old copies are removed automatically after some time without use.",
        }
    }
}

/// One folder setting: shows the current folder and lets the user choose, reset and open it.
///
/// # Arguments
/// * `folder` - Which folder this edits
/// * `title` - Section heading
/// * `dialog_title` - Title of the native folder dialog
/// * `description` - Explanation shown under the heading
#[component]
fn FolderSetting(
    folder: Folder,
    title: String,
    dialog_title: String,
    description: String,
) -> Element {
    let settings = use_context::<Settings>();
    let mut current = use_signal({
        let settings = settings.clone();
        move || settings.path(folder)
    });
    let mut is_default = use_signal({
        let settings = settings.clone();
        move || settings.is_default(folder)
    });
    // The one place changes are made, so the shown path always matches what is saved
    let apply = {
        let settings = settings.clone();
        move |path: Option<PathBuf>| -> Result<(), crate::settings::Error> {
            settings.set_path(folder, path)?;
            current.set(settings.path(folder));
            is_default.set(settings.is_default(folder));
            Ok(())
        }
    };

    let mut choose = use_action({
        let apply = apply.clone();
        let dialog_title = dialog_title.clone();
        move || {
            let mut apply = apply.clone();
            let dialog_title = dialog_title.clone();
            async move {
                let start = current.read().clone();
                let dialog_title = dialog_title.clone();
                let picked = rfd::AsyncFileDialog::new()
                    .set_title(dialog_title.clone())
                    .set_directory(start)
                    .pick_folder()
                    .await;
                match picked {
                    Some(folder) => apply(Some(folder.path().to_path_buf()))
                        .map_err(dioxus::CapturedError::from_display),
                    // Dialog cancelled
                    None => Ok(()),
                }
            }
        }
    });
    let mut reset = use_action({
        let apply = apply.clone();
        move || {
            let mut apply = apply.clone();
            async move { apply(None).map_err(dioxus::CapturedError::from_display) }
        }
    });
    let mut open = use_action(move || async move {
        opener::open(current.read().clone()).map_err(dioxus::CapturedError::from_display)
    });

    rsx! {
        h2 { class: "h4 mt-4", "{title}" }
        p { class: "text-body-secondary", "{description}" }
        for result in [choose.value().map(|r| r.map(|_| ())), reset.value().map(|r| r.map(|_| ())), open.value().map(|r| r.map(|_| ()))] {
            if let Some(Err(err)) = result {
                div { class: "alert alert-danger", role: "alert", "{err}" }
            }
        }
        div {
            class: "input-group mb-3",
            span { class: "input-group-text", "Folder" }
            input {
                r#type: "text",
                class: "form-control",
                readonly: true,
                aria_label: "{title} folder",
                value: "{current.read().display()}",
            }
            button {
                class: "btn btn-outline-primary",
                r#type: "button",
                onclick: move |_| choose.call(),
                i { class: "fa-solid fa-folder-open me-1" }
                "Choose folder…"
            }
            button {
                class: "btn btn-outline-secondary",
                r#type: "button",
                disabled: *is_default.read(),
                title: "Use the default folder in the application data directory",
                onclick: move |_| reset.call(),
                "Reset to default"
            }
            button {
                class: "btn btn-outline-secondary",
                r#type: "button",
                onclick: move |_| open.call(),
                "Open"
            }
        }
        p {
            class: "form-text",
            "Files already in the old folder are not moved. Runs that are already running keep using the old folder."
        }
    }
}
