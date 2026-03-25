use std::{cell::RefCell, path::PathBuf, rc::Rc};

use dioxus::{core::Event, html::DragData, logger::tracing, web::WebEventExt};
use futures::channel::oneshot;
use macworp_ui_core::pages::projects::edit::IsProjectEdit;
use thiserror::Error;
use web_sys::{
    js_sys::Undefined,
    wasm_bindgen::{prelude::Closure, JsCast},
    window, DragEvent, FileSystemDirectoryEntry, FileSystemFileEntry,
};

use crate::{client::BrowserFileUpload, file::UploadableFile};

#[derive(Debug, Error)]
pub enum Error {
    #[error("Failed to aquire document window from browser")]
    Window,
    #[error("Failed to prevent default file drop handling for the window")]
    PreventWindowFileDropHandlingFailed,
    #[error("Unable to read content of directory: {0}")]
    ReadDirecoryEntries(String),
    #[error("Unable to convert to web_sys event")]
    WebEventConversion,
    #[error("Unable to get file entries")]
    GetFileEntries,
    #[error("Unable to convert entry to directory entry for {0}")]
    DirectoryEntryConversion(String),
    #[error("Unable to convert entry to file entry")]
    FileEntryConversion,
    #[error("Callback did not return any entries for directory for {0}")]
    DirectoryReadCallbackNoEntries(String),
}

type DirEntryResult = Result<FileSystemDirectoryEntry, Error>;
type FileEntryResult = Result<FileSystemFileEntry, Error>;

pub struct FileMutableProjectEdit;

impl FileMutableProjectEdit {
    /// Read the given directory enry recursivley, enqueuing all file entries into the queue
    ///
    /// # Arguments
    /// * `entry` - The directory entry to read
    ///
    async fn read_directory(
        entry: web_sys::FileSystemDirectoryEntry,
    ) -> Result<Vec<FileEntryResult>, Error> {
        let (result_sender, result_receiver) =
            oneshot::channel::<(Vec<FileEntryResult>, Vec<DirEntryResult>)>();
        let result_sender = Rc::new(RefCell::new(Some(result_sender)));

        let reader = entry.create_reader();

        let reader_callback = Closure::wrap(Box::new(
            move |entries: Vec<web_sys::FileSystemEntry>| {
                let mut file_entries: Vec<FileEntryResult> = Vec::new();
                let mut dir_entries: Vec<DirEntryResult> = Vec::new();
                for entry in entries {
                    if entry.is_directory() {
                        let dir_entry =
                            entry
                                .dyn_into::<FileSystemDirectoryEntry>()
                                .map_err(|failed_entry| {
                                    Error::DirectoryEntryConversion(failed_entry.full_path())
                                });
                        dir_entries.push(dir_entry);
                    } else {
                        let file_entry = entry
                            .dyn_into::<FileSystemFileEntry>()
                            .map_err(|_| Error::FileEntryConversion);
                        file_entries.push(file_entry);
                    }
                }

                if let Some(result_sender) = result_sender.borrow_mut().take() {
                    // ignore send errors (receiver dropped)
                    match result_sender.send((file_entries, dir_entries)) {
                        Ok(_) => (),
                        Err(_) => tracing::error!("Failed to send file entries from directory read callback, receiver dropped"),
                    }
                }

                Undefined::default()
            },
        )
            as Box<dyn FnMut(Vec<web_sys::FileSystemEntry>) -> Undefined>);
        reader
            .read_entries_with_callback(reader_callback.as_ref().unchecked_ref())
            .map_err(|_| Error::ReadDirecoryEntries(entry.full_path()))?;

        let (mut file_entries, dir_entries) = match result_receiver.await {
            Ok(entries) => entries,
            Err(_) => (
                vec![Err(Error::DirectoryReadCallbackNoEntries(
                    entry.full_path(),
                ))],
                Vec::new(),
            ),
        };
        reader_callback.into_js_value();

        for dir_entry in dir_entries {
            match dir_entry {
                Ok(dir_entry) => {
                    let sub_entries = Box::pin(Self::read_directory(dir_entry)).await?;
                    file_entries.extend(sub_entries);
                }
                Err(err) => file_entries.push(Err(err)),
            }
        }

        Ok(file_entries)
    }
}

impl IsProjectEdit for FileMutableProjectEdit {
    type Error = Error;
    type File = UploadableFile;
    type FileUpload = BrowserFileUpload;

    async fn prevent_window_file_drop_handling() -> Result<(), Self::Error> {
        let (sync_sender, sync_receiver) = oneshot::channel::<()>();
        let sync_sender = Rc::new(RefCell::new(Some(sync_sender)));

        // Drop event handler for window
        let on_drop = Closure::wrap(Box::new(move |event: web_sys::DragEvent| {
            // Get data transfer list if prsent
            let dt = match event.data_transfer() {
                Some(dt) => dt,
                None => return Undefined::default(),
            };
            // Get items
            let data_transfer_item_list = dt.items();
            // Iterate through items in an ugly but accessible way
            let is_file_present = (0..data_transfer_item_list.length()).any(|i| {
                // Check if any item ist a file
                data_transfer_item_list
                    .get(i)
                    .map(|item| item.kind() == "file")
                    .unwrap_or(false)
            });

            // Prevent event if file is present
            if is_file_present {
                event.prevent_default();
            }
            if let Some(sync_sender) = sync_sender.borrow_mut().take() {
                // ignore send errors (receiver dropped)
                let _ = sync_sender.send(());
            }
            Undefined::default()
        }) as Box<dyn FnMut(DragEvent) -> Undefined>);

        let window = window().ok_or(Error::Window)?;

        window
            .add_event_listener_with_callback("drop", on_drop.as_ref().unchecked_ref())
            .map_err(|_| Error::PreventWindowFileDropHandlingFailed)?;

        window
            .add_event_listener_with_callback("dragover", on_drop.as_ref().unchecked_ref())
            .map_err(|_| Error::PreventWindowFileDropHandlingFailed)?;

        // we can saftly ignore any error as this is only a sync point
        let _ = sync_receiver.await;

        on_drop.into_js_value();

        Ok(())
    }

    async fn drop_handler(
        event: Event<DragData>,
        target_path: Rc<PathBuf>,
    ) -> Result<Vec<Result<Self::File, Self::Error>>, Self::Error> {
        // Convert to web_sys::DragEvent, as dioxus not let us play with the full file API to travrse directories
        let drag_event: web_sys::DragEvent = match event.try_as_web_event() {
            Some(event) => event,
            None => return Err(Error::WebEventConversion),
        };
        let data_transfer = match drag_event.data_transfer() {
            Some(data_transfer) => data_transfer,
            None => return Ok(Vec::new()),
        };
        let items = data_transfer.items();
        if items.length() > 0 {
            event.prevent_default();
        } else {
            return Ok(Vec::new());
        }

        let mut file_entries: Vec<Result<FileSystemFileEntry, Self::Error>> = Vec::new();

        for i in 0..items.length() {
            let item = match items.get(i) {
                Some(item) => item,
                None => continue,
            };
            let entry = match item.webkit_get_as_entry() {
                Ok(Some(entry)) => entry,
                Ok(None) => continue,
                Err(_) => {
                    file_entries.push(Err(Error::GetFileEntries));
                    continue;
                }
            };

            if entry.is_directory() {
                // Convert to directory entry and read it recursively
                let dir_entry: web_sys::FileSystemDirectoryEntry = match entry.dyn_into() {
                    Ok(dir_entry) => dir_entry,
                    Err(failed_entry) => {
                        file_entries.push(Err(Error::DirectoryEntryConversion(
                            failed_entry.full_path(),
                        )));
                        continue;
                    }
                };
                match Self::read_directory(dir_entry).await {
                    Ok(new_file_entries) => file_entries.extend(new_file_entries),
                    Err(err) => {
                        file_entries.push(Err(err));
                        continue;
                    }
                }
            } else {
                let file_entry = entry
                    .dyn_into::<web_sys::FileSystemFileEntry>()
                    .map_err(|_| Error::FileEntryConversion);
                file_entries.push(file_entry);
            }
        }

        Ok(file_entries
            .into_iter()
            .map(|file_entry_result| {
                file_entry_result
                    .map(|file_entry| UploadableFile::new(file_entry, target_path.clone()))
            })
            .collect())
    }

    async fn dragover_handler(event: Event<DragData>) -> Result<(), Self::Error> {
        if !event.data_transfer().files().is_empty() {
            event.prevent_default();
            event.data_transfer().set_drop_effect("copy");
        }
        Ok(())
    }
}
