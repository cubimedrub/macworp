use std::{cell::RefCell, rc::Rc};

use dioxus::signals::{Signal, WritableExt};
use futures::channel::oneshot;
use macworp_core::http::headers::ContentRangeHeaderValue;
use macworp_ui_core::client::{IsFileUpload, IsUploadableFile};
use thiserror::Error;
use wasm_bindgen_futures::JsFuture;
use web_sys::{
    js_sys::{Reflect, Uint8Array, Undefined},
    wasm_bindgen::{prelude::Closure, JsCast, JsValue},
    File,
};

use crate::file::UploadableFile;

#[derive(Debug, Error)]
pub enum Error {
    // #[error("Unable to set authorization header")]
    // AuthorizationHeader,
    #[error("Entry to file conversion failed")]
    File,
    // #[error("Form creation failed")]
    // FormCreation,
    // #[error("Adding file to form failed")]
    // FormFile,
    // #[error("Unable to create headers")]
    // HeaderCreation,
    // #[error("Request creation failed")]
    // RequestCreation,
    // #[error("Failed to convert JsValue to response")]
    // Response,
    // #[error("Unable to aquire windows")]
    // Window,
    #[error("Unable to get `done` value of stream read")]
    MissingStreamDone,
    #[error("Unable to get `value` value of stream read")]
    MissingStreamValue,
    #[error("File read failed")]
    Read,
    #[error("Reader conversion failed")]
    Reader,
    #[error("Request failed, {0}")]
    Request(reqwest::Error),
    #[error("Response status indicates failure: {0}")]
    ResponseStatus(reqwest::StatusCode),
}

/// Implementation to upload files using the browser JS API in a streaming manner,
/// to avoid buffering the complete file in memory. Unfortunatly [reqwest] `stream` feature is not available
/// so we need to rely on this aproach. I the future it could be updated to upload files in multple chunks involving
/// additional HTTP requests.
pub struct BrowserFileUpload;

impl BrowserFileUpload {
    /// Creates a callback to get a File-object from a FileSystemFileEntry
    ///
    /// # Arguments
    /// `file_callback_sender` - Sending end of a onshot channel to send the File back into the Rust world.
    ///
    fn create_get_file_callback(
        file_callback_sender: Rc<RefCell<Option<oneshot::Sender<File>>>>,
    ) -> web_sys::wasm_bindgen::prelude::ScopedClosure<
        'static,
        dyn FnMut(File) -> Undefined + 'static,
    > {
        Closure::wrap(Box::new(move |file: File| {
            if let Some(file_callback_sender) = file_callback_sender.borrow_mut().take() {
                // ignore send errors (receiver dropped)
                let _ = file_callback_sender.send(file);
            }
            Undefined::default()
        }) as Box<dyn FnMut(File) -> Undefined>)
    }
}

impl IsFileUpload for BrowserFileUpload {
    type Error = Error;
    type File = UploadableFile;

    async fn upload_file(
        url: &str,
        jwt: &str,
        file: Self::File,
        mut progress: Signal<f64>,
    ) -> Result<(), Self::Error> {
        progress.set(0.0);
        let path = file.upload_path();
        let file = file.into_inner();

        // Create a oneshot channel to receive the callback result
        let (file_callback_sender, file_callback_receiver) = oneshot::channel::<File>();
        let file_callback_sender = Rc::new(RefCell::new(Some(file_callback_sender)));

        let upload_file_callback = Self::create_get_file_callback(file_callback_sender);
        file.file_with_callback(upload_file_callback.as_ref().unchecked_ref());
        let file = file_callback_receiver.await.map_err(|_| Error::File)?;
        upload_file_callback.into_js_value();

        let file_size = file.size() as u64;

        let stream = file.stream();
        let reader = stream.get_reader();
        let reader = reader
            .dyn_into::<web_sys::ReadableStreamDefaultReader>()
            .map_err(|_| Error::Reader)?;

        let client = reqwest::Client::new();
        let request_builder = client
            .post(url)
            .header(http::header::AUTHORIZATION, format!("JWT {jwt}"))
            .query(&[("file_path", path.clone())]);

        let mut read_bytes: u64 = 0;

        let mut buffer: Vec<u8> = Vec::with_capacity(2 * 1024 * 1024); // 10 MiB buffer, TODO: Make adjustable.
        let mut chunk = Vec::new();
        let mut is_stream_exhausted = false;

        loop {
            if is_stream_exhausted && chunk.is_empty() && buffer.is_empty() {
                break;
            }

            if chunk.is_empty() {
                let read_promise = reader.read();
                let read_js_val = JsFuture::from(read_promise)
                    .await
                    .map_err(|_| Error::Read)?;
                let is_reading_done = Reflect::get(&read_js_val, &JsValue::from_str("done"))
                    .map_err(|_| Error::MissingStreamDone)?
                    .as_bool()
                    .unwrap_or(false);
                if is_reading_done {
                    is_stream_exhausted = true;
                }
                let value = Reflect::get(&read_js_val, &JsValue::from_str("value"))
                    .map_err(|_| Error::MissingStreamValue)?;
                // Convert chunk to Vec<u8>
                chunk = Uint8Array::new(&value).to_vec();
            }

            if chunk.len() <= buffer.capacity() - buffer.len() {
                buffer.append(&mut chunk);
            } else {
                buffer.extend(chunk.drain(..buffer.capacity() - buffer.len()));
            }

            if buffer.len() == buffer.capacity() || (is_stream_exhausted && !buffer.is_empty()) {
                let buffer_len = buffer.len();
                let response = request_builder
                    .try_clone()
                    .unwrap()
                    .header(
                        http::header::CONTENT_RANGE,
                        ContentRangeHeaderValue::new(
                            "bytes",
                            (read_bytes, read_bytes + buffer_len as u64 - 1),
                            Some(file_size),
                        ),
                    )
                    .body(buffer)
                    .send()
                    .await
                    .map_err(Error::Request)?;

                if !response.status().is_success() {
                    return Err(Error::ResponseStatus(response.status()));
                }
                read_bytes += buffer_len as u64;
                buffer = Vec::with_capacity(2 * 1024 * 1024); // Clear buffer and create a new one.
            }
            progress.set(read_bytes as f64 / file_size as f64 * 100.0);
        }

        Ok(())
    }
}
