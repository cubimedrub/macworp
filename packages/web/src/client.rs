use std::{cell::RefCell, rc::Rc};

use dioxus::{
    logger::tracing,
    signals::{Signal, WritableExt},
};
use futures::channel::oneshot;
use macworp_core::http::headers::ContentRangeHeaderValue;
use macworp_ui_core::client::{IsFileUpload, IsUploadableFile};
use reqwest::RequestBuilder;
use thiserror::Error;
use wasm_bindgen_futures::{spawn_local, JsFuture};
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
    #[error("Uplaod was unexpectadly canceled")]
    UploadResultChannel,
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

    async fn partial_upload(
        request_builder: RequestBuilder,
        file_size: u64,
        content_range_start: u64,
        body: Vec<u8>,
    ) -> oneshot::Receiver<Result<u64, Error>> {
        let (result_sender, result_receiver) = oneshot::channel::<Result<u64, Error>>();

        spawn_local(async move {
            let body_len = body.len();
            let result = request_builder
                .header(
                    http::header::CONTENT_RANGE,
                    ContentRangeHeaderValue::new(
                        "bytes",
                        (
                            content_range_start,
                            content_range_start + body_len as u64 - 1,
                        ),
                        Some(file_size),
                    ),
                )
                .body(body)
                .send()
                .await
                .map_err(Error::Request)
                .and_then(|response| {
                    if response.status().is_success() {
                        Ok(body_len as u64)
                    } else {
                        Err(Error::ResponseStatus(response.status()))
                    }
                });

            match result_sender.send(result) {
                Ok(_) => {}
                Err(_err) => tracing::error!(
                    "Upload request could not send results due to closes oneshot channel."
                ),
            }
        });

        result_receiver
    }
}

impl IsFileUpload for BrowserFileUpload {
    type Error = Error;
    type File = UploadableFile;

    async fn upload_file(
        url: &str,
        jwt: &str,
        file: Self::File,
        upload_buffer_size: usize,
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

        let mut buffer: Vec<u8> = Vec::with_capacity(upload_buffer_size); // 10 MiB buffer, TODO: Make adjustable.
        let mut chunk = Vec::new();
        let mut is_stream_exhausted = false;

        let mut partial_upload_result_opt: Option<oneshot::Receiver<Result<u64, Error>>> = None;

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
                // Wait for the previous request to succeed
                if let Some(partial_upload) = partial_upload_result_opt.take() {
                    read_bytes += partial_upload
                        .await
                        .map_err(|_| Error::UploadResultChannel)??;
                    progress.set(read_bytes as f64 / file_size as f64 * 100.0);
                }

                partial_upload_result_opt = Some(
                    Self::partial_upload(
                        request_builder.try_clone().unwrap(),
                        file_size,
                        read_bytes,
                        std::mem::replace(&mut buffer, Vec::with_capacity(upload_buffer_size)), // Move buffer into body and create a new buffer for next file chunks
                    )
                    .await,
                );
            }
        }

        Ok(())
    }
}
