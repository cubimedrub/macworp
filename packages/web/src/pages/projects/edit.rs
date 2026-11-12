use std::{cell::RefCell, path::PathBuf, rc::Rc};

use dioxus::{core::Event, html::DragData, web::WebEventExt};
use futures::channel::oneshot;
use macworp_ui_core::pages::projects::edit::IsProjectEdit;
use thiserror::Error;
use web_sys::{
    js_sys::Undefined,
    wasm_bindgen::{prelude::Closure, JsCast},
    window, DragEvent, FileSystemDirectoryEntry, FileSystemFileEntry,
};

use crate::{
    client::BrowserFileUpload, components::file_browser::FileBrowser, file::UploadableFile,
    router::Router, storage::Storage,
};

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
    #[error("Callback did not return any entries for directory for {0}")]
    DirectoryReadCallbackNoEntries(String),
}

type FileEntryResult = Result<FileSystemFileEntry, Error>;

pub struct FileMutableProjectEdit;

impl FileMutableProjectEdit {
    /// Whether a drag carries files. While dragging, Safari exposes neither `files` nor
    /// `items`, only the `types` list (containing `"Files"`); Chromium exposes item kinds.
    ///
    /// # Arguments
    /// * `data_transfer` - Data transfer of the drag event to inspect
    ///
    fn carries_files(data_transfer: &web_sys::DataTransfer) -> bool {
        let has_files_type = data_transfer
            .types()
            .iter()
            .any(|value| value.as_string().is_some_and(|value| value == "Files"));
        let items = data_transfer.items();
        has_files_type
            || (0..items.length()).any(|i| items.get(i).is_some_and(|item| item.kind() == "file"))
    }

    /// Read one batch of entries from a directory reader. Browsers return a directory in batches
    /// (Chromium at most 100 entries per call), an empty batch marks the end.
    ///
    /// # Arguments
    /// * `reader` - The directory reader to read the next batch from
    /// * `path` - Full path of the directory, only used for error reporting
    ///
    async fn read_batch(
        reader: &web_sys::FileSystemDirectoryReader,
        path: &str,
    ) -> Result<Vec<web_sys::FileSystemEntry>, Error> {
        let (result_sender, result_receiver) = oneshot::channel::<Vec<web_sys::FileSystemEntry>>();
        let result_sender = Rc::new(RefCell::new(Some(result_sender)));

        let reader_callback =
            Closure::wrap(Box::new(move |entries: Vec<web_sys::FileSystemEntry>| {
                if let Some(result_sender) = result_sender.borrow_mut().take() {
                    // ignore send errors (receiver dropped)
                    let _ = result_sender.send(entries);
                }
                Undefined::default()
            })
                as Box<dyn FnMut(Vec<web_sys::FileSystemEntry>) -> Undefined>);
        reader
            .read_entries_with_callback(reader_callback.as_ref().unchecked_ref())
            .map_err(|_| Error::ReadDirecoryEntries(path.to_string()))?;

        let entries = result_receiver
            .await
            .map_err(|_| Error::DirectoryReadCallbackNoEntries(path.to_string()))?;
        reader_callback.into_js_value();
        Ok(entries)
    }

    /// Read the given directory entry recursively and return all file entries found.
    ///
    /// # Arguments
    /// * `entry` - The directory entry to read
    ///
    async fn read_directory(
        entry: web_sys::FileSystemDirectoryEntry,
    ) -> Result<Vec<FileEntryResult>, Error> {
        let path = entry.full_path();
        let reader = entry.create_reader();

        let mut file_entries: Vec<FileEntryResult> = Vec::new();
        let mut dir_entries: Vec<FileSystemDirectoryEntry> = Vec::new();
        loop {
            let batch = Self::read_batch(&reader, &path).await?;
            if batch.is_empty() {
                break;
            }
            for entry in batch {
                // Decide on the entry flags and not on `instanceof`: Chromium and Safari hand out
                // entry objects for which the checked `dyn_into` cast fails.
                if entry.is_directory() {
                    dir_entries.push(entry.unchecked_into::<FileSystemDirectoryEntry>());
                } else {
                    file_entries.push(Ok(entry.unchecked_into::<FileSystemFileEntry>()));
                }
            }
        }

        for dir_entry in dir_entries {
            let sub_entries = Box::pin(Self::read_directory(dir_entry)).await?;
            file_entries.extend(sub_entries);
        }

        Ok(file_entries)
    }
}

impl IsProjectEdit for FileMutableProjectEdit {
    type Error = Error;
    type File = UploadableFile;
    type FileUpload = BrowserFileUpload;
    type FileBrowser = FileBrowser;
    type Storage = Storage;
    type Router = Router;

    async fn prevent_window_file_drop_handling() -> Result<(), Self::Error> {
        // Register the window listeners only once, the component may mount several times.
        thread_local! {
            static REGISTERED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
        }
        if REGISTERED.replace(true) {
            return Ok(());
        }

        let (sync_sender, sync_receiver) = oneshot::channel::<()>();
        let sync_sender = Rc::new(RefCell::new(Some(sync_sender)));

        // Drop event handler for window
        let on_drop = Closure::wrap(Box::new(move |event: web_sys::DragEvent| {
            // Get data transfer list if prsent
            let dt = match event.data_transfer() {
                Some(dt) => dt,
                None => return Undefined::default(),
            };
            let is_file_present = FileMutableProjectEdit::carries_files(&dt);

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

        // The listeners live as long as the page, so leak the closure right away. Dropping it
        // (e.g. when this future is cancelled before the first event) would make the still
        // registered listeners throw "closure invoked after being dropped".
        on_drop.into_js_value();

        // we can saftly ignore any error as this is only a sync point
        let _ = sync_receiver.await;

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

        // `DataTransferItemList` is only valid while the event is dispatched in Chromium and
        // Safari: resolve all entries before the first `.await`.
        let mut top_level_entries: Vec<web_sys::FileSystemEntry> = Vec::new();
        let mut file_entries: Vec<Result<FileSystemFileEntry, Self::Error>> = Vec::new();
        for i in 0..items.length() {
            let item = match items.get(i) {
                Some(item) => item,
                None => continue,
            };
            if item.kind() != "file" {
                continue;
            }
            match item.webkit_get_as_entry() {
                Ok(Some(entry)) => top_level_entries.push(entry),
                Ok(None) => continue,
                Err(_) => file_entries.push(Err(Error::GetFileEntries)),
            }
        }

        for entry in top_level_entries {
            if entry.is_directory() {
                let dir_entry = entry.unchecked_into::<FileSystemDirectoryEntry>();
                match Self::read_directory(dir_entry).await {
                    Ok(new_file_entries) => file_entries.extend(new_file_entries),
                    Err(err) => file_entries.push(Err(err)),
                }
            } else {
                file_entries.push(Ok(entry.unchecked_into::<FileSystemFileEntry>()));
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
        // `files()` is empty while dragging in Chromium and Safari, only the items (kind) are
        // readable, so check those.
        let drag_event: web_sys::DragEvent =
            event.try_as_web_event().ok_or(Error::WebEventConversion)?;
        let has_file = drag_event
            .data_transfer()
            .is_some_and(|data_transfer| Self::carries_files(&data_transfer));
        if has_file {
            event.prevent_default();
            event.data_transfer().set_drop_effect("copy");
        }
        Ok(())
    }
}
