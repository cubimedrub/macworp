use std::{
    io::{self, Write},
    path::{Path, PathBuf},
};

use axum::{
    body::{Body, Bytes},
    http::{HeaderValue, StatusCode, header},
    response::Response,
};
use tokio::sync::mpsc;
use tokio_util::io::ReaderStream;
use zip::{ZipWriter, write::SimpleFileOptions};

use crate::filesystems::{directory_files::DirectoryFiles, errors::Error};

/// Download of a file or folder below an already confined path: a file is streamed as it is, a
/// folder is zipped while it is streamed, so nothing is compressed in advance or buffered on disk.
pub struct Download {
    /// File or folder to send
    path: PathBuf,
    /// Name the browser saves it under, for a folder without the `.zip` extension
    name: String,
    /// Name of a top-level entry of a folder that is left out of its archive, e.g. a folder that
    /// only exists virtually in listings
    skip_top_level: Option<String>,
}

impl Download {
    /// Download of `path`, saved as `name`.
    ///
    /// # Arguments
    /// * `path` - File or folder to send, already confined to its base (see
    ///   [`DirectoryFiles::join_paths`])
    /// * `name` - Name to save it under; a folder gets `.zip` appended
    pub fn new(path: PathBuf, name: String) -> Self {
        Self {
            path,
            name,
            skip_top_level: None,
        }
    }

    /// Leaves the top-level entry `name` out of the archive of a folder.
    ///
    /// # Arguments
    /// * `name` - File name of the entry directly below the downloaded folder
    pub fn skipping_top_level(mut self, name: &str) -> Self {
        self.skip_top_level = Some(name.to_string());
        self
    }

    /// Builds the response: the file with its length, or the streamed archive of the folder.
    ///
    /// Fails with [`Error::FileNotFound`] if the path does not exist or is neither a file nor a
    /// folder (symlinks included, they are never followed).
    pub async fn into_response(self) -> Result<Response, Error> {
        let metadata = match tokio::fs::symlink_metadata(&self.path).await {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Err(Error::FileNotFound(self.path));
            }
            Err(error) => return Err(error.into()),
        };
        if metadata.is_file() {
            let file = tokio::fs::File::open(&self.path).await?;
            let body = Body::from_stream(ReaderStream::new(file));
            Ok(Self::response(
                "application/octet-stream",
                &self.name,
                Some(metadata.len()),
                body,
            ))
        } else if metadata.is_dir() {
            let name = format!("{}.zip", self.name);
            let body = Body::from_stream(futures_util::stream::unfold(
                self.start_archive(),
                |mut receiver| async move { receiver.recv().await.map(|chunk| (chunk, receiver)) },
            ));
            Ok(Self::response("application/zip", &name, None, body))
        } else {
            Err(Error::FileNotFound(self.path))
        }
    }

    /// Starts the thread that writes the archive and returns the receiving end of its output. The
    /// channel is small, so the thread waits for a slow client instead of reading ahead; it stops
    /// as soon as the client is gone.
    fn start_archive(self) -> mpsc::Receiver<Result<Bytes, io::Error>> {
        let (sender, receiver) = mpsc::channel(Self::CHANNEL_CHUNKS);
        tokio::task::spawn_blocking(move || {
            let mut writer = ChannelWriter::new(sender.clone());
            let result = self.write_archive(&mut writer);
            if let Err(error) = result {
                // Aborts the response: the client must not mistake a cut archive for a complete one
                let _ = sender.blocking_send(Err(error));
            }
        });
        receiver
    }

    /// Writes the zip archive of the folder into `writer`.
    ///
    /// # Arguments
    /// * `writer` - Sink of the archive bytes
    fn write_archive(&self, writer: &mut ChannelWriter) -> io::Result<()> {
        let mut archive = ZipWriter::new_stream(&mut *writer);
        // Sizes are unknown up front, so entries may need the zip64 extension
        let options = SimpleFileOptions::default().large_file(true);
        self.add_folder(
            &mut archive,
            &self.path,
            Path::new(&self.name),
            true,
            options,
        )?;
        archive.finish()?;
        writer.flush()
    }

    /// Adds the folder `folder` and everything below it, in name order, as entries below `prefix`.
    /// Symlinks and unfinished uploads are left out.
    ///
    /// # Arguments
    /// * `archive` - The archive to add to
    /// * `folder` - Folder on disk
    /// * `prefix` - Path of the folder inside the archive
    /// * `top_level` - Whether `folder` is the downloaded folder itself, the only one whose
    ///   `skip_top_level` entry is left out
    /// * `options` - Entry options
    fn add_folder<W: Write>(
        &self,
        archive: &mut ZipWriter<zip::write::StreamWriter<W>>,
        folder: &Path,
        prefix: &Path,
        top_level: bool,
        options: SimpleFileOptions,
    ) -> io::Result<()> {
        archive.add_directory(Self::entry_name(prefix, true), options)?;
        let mut entries = std::fs::read_dir(folder)?.collect::<Result<Vec<_>, _>>()?;
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            let file_name = entry.file_name();
            let file_name_text = file_name.to_string_lossy();
            if DirectoryFiles::is_unfinished_name(&file_name_text)
                || (top_level && self.skip_top_level.as_deref() == Some(&*file_name_text))
            {
                continue;
            }
            let file_type = entry.file_type()?;
            let entry_path = prefix.join(&file_name);
            if file_type.is_dir() {
                self.add_folder(archive, &entry.path(), &entry_path, false, options)?;
            } else if file_type.is_file() {
                archive.start_file(Self::entry_name(&entry_path, false), options)?;
                io::copy(&mut std::fs::File::open(entry.path())?, archive)?;
            }
        }
        Ok(())
    }

    /// Entry name in the archive: `/` separated, folders end with `/`.
    ///
    /// # Arguments
    /// * `path` - Path of the entry inside the archive
    /// * `folder` - Whether the entry is a folder
    fn entry_name(path: &Path, folder: bool) -> String {
        let mut name = path
            .components()
            .map(|component| component.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/");
        if folder {
            name.push('/');
        }
        name
    }

    /// Chunks the archive thread may have waiting before it blocks.
    const CHANNEL_CHUNKS: usize = 4;

    /// Response with the headers every download carries.
    ///
    /// # Arguments
    /// * `content_type` - Media type of the body
    /// * `name` - Name the browser saves the body under
    /// * `length` - Length of the body in bytes, if known up front
    /// * `body` - The body
    fn response(
        content_type: &'static str,
        name: &str,
        length: Option<u64>,
        body: Body,
    ) -> Response {
        let mut response = Response::new(body);
        *response.status_mut() = StatusCode::OK;
        let headers = response.headers_mut();
        headers.insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
        headers.insert(
            header::CONTENT_DISPOSITION,
            HeaderValue::from_str(&Self::content_disposition(name))
                .unwrap_or_else(|_| HeaderValue::from_static("attachment")),
        );
        headers.insert(
            header::X_CONTENT_TYPE_OPTIONS,
            HeaderValue::from_static("nosniff"),
        );
        headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
        if let Some(length) = length {
            headers.insert(header::CONTENT_LENGTH, HeaderValue::from(length));
        }
        response
    }

    /// `Content-Disposition` value that makes the browser save the body as `name`: an ASCII
    /// fallback and the exact name percent-encoded as UTF-8 (RFC 6266).
    ///
    /// # Arguments
    /// * `name` - File name to save under
    fn content_disposition(name: &str) -> String {
        let fallback: String = name
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        let encoded: String = name
            .bytes()
            .map(|byte| {
                if byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_') {
                    (byte as char).to_string()
                } else {
                    format!("%{byte:02X}")
                }
            })
            .collect();
        format!("attachment; filename=\"{fallback}\"; filename*=UTF-8''{encoded}")
    }
}

/// [`Write`] sink that hands what is written to an async channel in chunks, for use on a blocking
/// thread. Fails with `BrokenPipe` once the receiver is gone, which stops the writing thread.
struct ChannelWriter {
    /// Where finished chunks go
    sender: mpsc::Sender<Result<Bytes, io::Error>>,
    /// Bytes not yet sent
    buffer: Vec<u8>,
}

impl ChannelWriter {
    /// Bytes collected before a chunk is sent
    const CHUNK_BYTES: usize = 64 * 1024;

    /// Writer sending to `sender`.
    ///
    /// # Arguments
    /// * `sender` - Channel the chunks are sent to
    fn new(sender: mpsc::Sender<Result<Bytes, io::Error>>) -> Self {
        Self {
            sender,
            buffer: Vec::with_capacity(Self::CHUNK_BYTES),
        }
    }
}

impl Write for ChannelWriter {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        self.buffer.extend_from_slice(data);
        if self.buffer.len() >= Self::CHUNK_BYTES {
            self.flush()?;
        }
        Ok(data.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        if self.buffer.is_empty() {
            return Ok(());
        }
        let chunk = Bytes::from(std::mem::take(&mut self.buffer));
        self.sender
            .blocking_send(Ok(chunk))
            .map_err(|_| io::ErrorKind::BrokenPipe.into())
    }
}
