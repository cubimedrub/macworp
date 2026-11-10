use std::path::PathBuf;

use dioxus::prelude::*;

use crate::settings::Settings;

/// Desktop settings: where the global reference data (genomes, indices, annotation shared by all
/// projects) lives. The user picks a folder with the native dialog, nothing is typed or edited
/// in files. The choice is saved at once and applies to the next run.
#[component]
pub fn SettingsPage() -> Element {
    let settings = use_context::<Settings>();
    let mut current = use_signal({
        let settings = settings.clone();
        move || settings.reference_directory().get()
    });
    let mut is_default = use_signal({
        let settings = settings.clone();
        move || settings.is_default()
    });
    // The one place changes are made, so the shown path always matches what is saved
    let apply = {
        let settings = settings.clone();
        move |path: Option<PathBuf>| -> Result<(), crate::settings::Error> {
            settings.set_reference_path(path)?;
            current.set(settings.reference_directory().get());
            is_default.set(settings.is_default());
            Ok(())
        }
    };

    let mut choose = use_action({
        let apply = apply.clone();
        move || {
            let mut apply = apply.clone();
            async move {
                let start = current.read().clone();
                let picked = rfd::AsyncFileDialog::new()
                    .set_title("Folder with the global reference data")
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
        h1 { "Settings" }
        h2 { class: "h4 mt-4", "Global reference data" }
        p {
            class: "text-body-secondary",
            "Genomes, indices and annotation that every project can use in workflow parameters. \
             Put the files into this folder with your file manager; projects see them under \
             macworp_global_reference_data. Workflows can read the folder but never change it."
        }
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
                aria_label: "Global reference data folder",
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
