//! Bounded native file reads and ordered managed imports.
use super::{AppMsg, AppRuntime, UiError, display_error};
use crate::mvu::{EditorMsg, ImportFileSource, ImportTarget, StoredMedia};
use carver_sdk::LibraryBackend;
use gtk::gio::{self, prelude::*};

const MAX_FILE_BYTES: usize = 50 * 1024 * 1024;

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
        let mut stored = Vec::new();
        for file in files {
            let bytes = read_bounded_file(&gio::File::for_uri(&file.uri)).await?;
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
                .store_asset_async(note_id, file.extension, bytes)
                .await
                .map_err(display_error)?;
            stored.push(StoredMedia {
                path,
                label: file.label,
                image: file.image,
            });
        }
        Ok(stored)
    }
}

async fn read_bounded_file(file: &gio::File) -> Result<Vec<u8>, UiError> {
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
    if u64::try_from(info.size()).unwrap_or(u64::MAX) > MAX_FILE_BYTES as u64 {
        return Err(UiError::new("Files must be 50 MB or smaller"));
    }
    let stream = file
        .read_future(glib::Priority::DEFAULT)
        .await
        .map_err(display_error)?;
    read_bounded_stream(&stream, MAX_FILE_BYTES).await
}

async fn read_bounded_stream(
    stream: &impl IsA<gio::InputStream>,
    limit: usize,
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
            return Err(UiError::new("Files must be 50 MB or smaller"));
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
