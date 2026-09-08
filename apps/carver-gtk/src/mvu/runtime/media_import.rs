//! Bounded native file reads and ordered managed imports.
use super::{AppMsg, AppRuntime, UiError, display_error};
use crate::mvu::{EditorMsg, ImportFileSource, ImportTarget, StoredMedia};
use carver_sdk::LibraryBackend;
use gtk::gio::{self, prelude::*};

const MAX_FILE_BYTES: usize = 50 * 1024 * 1024;
const MAX_BATCH_BYTES: usize = 200 * 1024 * 1024;
const FILE_SIZE_ERROR: &str = "Files must be 50 MB or smaller";
const BATCH_SIZE_ERROR: &str = "Selected files must total 200 MB or smaller";

impl<B: LibraryBackend> AppRuntime<B> {
    pub(super) fn import_editor_files(
        &self,
        target: ImportTarget,
        note_id: carver_sdk::NoteId,
        files: Vec<ImportFileSource>,
    ) {
        let runtime = self.clone();
        glib::spawn_future_local(async move {
            let result = runtime
                .store_native_files(target.session, note_id, files)
                .await;
            runtime.dispatch(AppMsg::Editor(EditorMsg::ImportFilesStored {
                target,
                result,
            }));
        });
    }

    async fn store_native_files(
        &self,
        session: crate::mvu::EditorSessionId,
        note_id: carver_sdk::NoteId,
        files: Vec<ImportFileSource>,
    ) -> Result<Vec<StoredMedia>, UiError> {
        let files = stage_native_files(files).await?;
        let mut stored = Vec::with_capacity(files.len());
        for file in files {
            if !self
                .inner
                .model
                .borrow()
                .editor
                .as_ref()
                .is_some_and(|document| document.session == session)
            {
                return Ok(Vec::new());
            }
            let path = self
                .inner
                .client
                .store_asset_async(note_id, file.source.extension, file.bytes)
                .await
                .map_err(display_error)?;
            stored.push(StoredMedia {
                path,
                label: file.source.label,
                image: file.source.image,
            });
        }
        Ok(stored)
    }
}

/// A validated native file kept in memory until the complete selection is ready to store.
struct StagedFile {
    source: ImportFileSource,
    bytes: Vec<u8>,
}

async fn stage_native_files(files: Vec<ImportFileSource>) -> Result<Vec<StagedFile>, UiError> {
    stage_native_files_with_limit(files, MAX_BATCH_BYTES).await
}

async fn stage_native_files_with_limit(
    files: Vec<ImportFileSource>,
    batch_limit: usize,
) -> Result<Vec<StagedFile>, UiError> {
    let mut staged = Vec::with_capacity(files.len());
    let mut staged_bytes = 0_usize;
    for source in files {
        let remaining = remaining_batch_capacity(staged_bytes, batch_limit)?;
        let limit = MAX_FILE_BYTES.min(remaining);
        let bytes = read_bounded_file_with_limit(
            &gio::File::for_uri(&source.uri),
            limit,
            if limit == MAX_FILE_BYTES {
                FILE_SIZE_ERROR
            } else {
                BATCH_SIZE_ERROR
            },
        )
        .await?;
        staged_bytes += bytes.len();
        staged.push(StagedFile { source, bytes });
    }
    Ok(staged)
}

fn remaining_batch_capacity(staged_bytes: usize, batch_limit: usize) -> Result<usize, UiError> {
    batch_limit
        .checked_sub(staged_bytes)
        .filter(|remaining| *remaining > 0)
        .ok_or_else(|| UiError::new(BATCH_SIZE_ERROR))
}

#[cfg(test)]
async fn read_bounded_file(file: &gio::File) -> Result<Vec<u8>, UiError> {
    read_bounded_file_with_limit(file, MAX_FILE_BYTES, FILE_SIZE_ERROR).await
}

async fn read_bounded_file_with_limit(
    file: &gio::File,
    limit: usize,
    limit_error: &str,
) -> Result<Vec<u8>, UiError> {
    let info = file
        .query_info_future(
            "standard::type,standard::size",
            gio::FileQueryInfoFlags::NONE,
            glib::Priority::DEFAULT,
        )
        .await
        .map_err(display_error)?;
    if info.file_type() != gio::FileType::Regular {
        return Err(UiError::new("Only regular files can be added"));
    }
    if u64::try_from(info.size()).unwrap_or(u64::MAX) > limit as u64 {
        return Err(UiError::new(limit_error));
    }
    let stream = file
        .read_future(glib::Priority::DEFAULT)
        .await
        .map_err(display_error)?;
    read_bounded_stream_with_limit(&stream, limit, limit_error).await
}

#[cfg(test)]
async fn read_bounded_stream(
    stream: &impl IsA<gio::InputStream>,
    limit: usize,
) -> Result<Vec<u8>, UiError> {
    read_bounded_stream_with_limit(stream, limit, FILE_SIZE_ERROR).await
}

async fn read_bounded_stream_with_limit(
    stream: &impl IsA<gio::InputStream>,
    limit: usize,
    limit_error: &str,
) -> Result<Vec<u8>, UiError> {
    let mut bytes = Vec::new();
    loop {
        let chunk = stream
            .read_bytes_future(
                (limit + 1 - bytes.len()).min(64 * 1024),
                glib::Priority::DEFAULT,
            )
            .await
            .map_err(display_error)?;
        if chunk.is_empty() {
            return Ok(bytes);
        }
        bytes.extend_from_slice(chunk.as_ref());
        if bytes.len() > limit {
            return Err(UiError::new(limit_error));
        }
    }
}

/// Decode the source once, retaining only a small aspect-preserving PNG thumbnail.
pub(super) fn thumbnail(bytes: &[u8]) -> Option<Vec<u8>> {
    use gtk::gdk_pixbuf::prelude::*;
    let loader = gtk::gdk_pixbuf::PixbufLoader::new();
    loader.connect_size_prepared(|loader, width, height| {
        let largest = width.max(height).max(1);
        if largest > 96 {
            loader.set_size(
                i32::try_from(((i64::from(width) * 96) / i64::from(largest)).max(1)).unwrap_or(96),
                i32::try_from(((i64::from(height) * 96) / i64::from(largest)).max(1)).unwrap_or(96),
            );
        }
    });
    let written = loader.write(bytes);
    let closed = loader.close();
    written.ok()?;
    closed.ok()?;
    loader.pixbuf()?.save_to_bufferv("png", &[]).ok()
}

#[cfg(test)]
mod tests;
