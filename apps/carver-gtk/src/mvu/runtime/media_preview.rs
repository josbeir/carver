//! SDK reads, isolated preview copies, and downloads for managed media.

use std::{io::Write, os::unix::fs::PermissionsExt, path::PathBuf};

use super::super::safe_media_filename;
use super::{AppMsg, AppRuntime, LibraryBackend, UiError, display_error, gio};
use gettextrs::gettext;
use gtk::gio::prelude::FileExt;

#[derive(Debug, thiserror::Error)]
enum PreviewCopyError {
    #[error("Could not prepare the preview: {0}")]
    Io(#[from] std::io::Error),
}

enum PreparedPreview {
    Cached(PathBuf),
    New {
        note_id: carver_sdk::NoteId,
        asset_path: String,
        directory: tempfile::TempDir,
        preview_path: PathBuf,
    },
}

impl<B: LibraryBackend> AppRuntime<B> {
    pub(super) fn prepare_media_preview(
        &self,
        session: super::super::EditorSessionId,
        note_id: carver_sdk::NoteId,
        path: String,
        label: String,
    ) {
        let client = self.inner.client.clone();
        let runtime = self.clone();
        glib::spawn_future_local(async move {
            let cached = cached_preview_path(&runtime.inner.preview_copies, note_id, &path);
            let result = if let Some(cached) = cached {
                Ok(PreparedPreview::Cached(cached))
            } else {
                let asset_path = path.clone();
                let copy_path = asset_path.clone();
                match client.note_asset_bytes_async(note_id, path).await {
                    Ok(Some(bytes)) => {
                        gio::spawn_blocking(move || prepare_copy(&bytes, &copy_path, &label))
                            .await
                            .map_err(|_| UiError::new("Could not prepare the file preview."))
                            .and_then(|result| result.map_err(display_error))
                            .map(|(directory, preview_path)| PreparedPreview::New {
                                note_id,
                                asset_path,
                                directory,
                                preview_path,
                            })
                    }
                    Ok(None) => Err(UiError::new(gettext("This file is no longer available."))),
                    Err(error) => Err(display_error(error)),
                }
            };
            if runtime
                .model()
                .editor
                .as_ref()
                .is_none_or(|document| document.session != session)
            {
                return;
            }
            let result = result.map(|preview| match preview {
                PreparedPreview::Cached(path) => path,
                PreparedPreview::New {
                    note_id,
                    asset_path,
                    directory,
                    preview_path,
                } => retain_preview_copy(
                    &runtime.inner.preview_copies,
                    note_id,
                    asset_path,
                    directory,
                    preview_path,
                ),
            });
            runtime.dispatch(AppMsg::Editor(
                super::super::EditorMsg::MediaPreviewPrepared { session, result },
            ));
        });
    }

    /// Writes one managed asset's bytes to the URI chosen in the native save dialog.
    pub(super) fn write_media_download(
        &self,
        session: super::super::EditorSessionId,
        note_id: carver_sdk::NoteId,
        path: String,
        target_uri: String,
    ) {
        let client = self.inner.client.clone();
        let runtime = self.clone();
        glib::spawn_future_local(async move {
            match client.note_asset_bytes_async(note_id, path).await {
                Ok(Some(bytes)) => {
                    let file = gtk::gio::File::for_uri(&target_uri);
                    let bytes = glib::Bytes::from_owned(bytes);
                    let runtime = runtime.clone();
                    file.replace_contents_bytes_async(
                        &bytes,
                        None,
                        false,
                        gtk::gio::FileCreateFlags::REPLACE_DESTINATION,
                        None::<&gtk::gio::Cancellable>,
                        move |result| {
                            runtime.finish_media_download(
                                session,
                                result.map(|_| ()).map_err(display_error),
                            );
                        },
                    );
                }
                Ok(None) => runtime.finish_media_download(
                    session,
                    Err(UiError::new(gettext("This file is no longer available."))),
                ),
                Err(error) => runtime.finish_media_download(session, Err(display_error(error))),
            }
        });
    }

    /// Reports a finished download only while its requesting editor is still active.
    fn finish_media_download(
        &self,
        session: super::super::EditorSessionId,
        result: Result<(), UiError>,
    ) {
        if self
            .model()
            .editor
            .as_ref()
            .is_none_or(|document| document.session != session)
        {
            return;
        }
        self.dispatch(AppMsg::Editor(
            super::super::EditorMsg::MediaDownloadFinished { session, result },
        ));
    }
}

fn cached_preview_path(
    copies: &std::cell::RefCell<super::PreviewCopies>,
    note_id: carver_sdk::NoteId,
    path: &str,
) -> Option<PathBuf> {
    copies
        .borrow()
        .get(&(note_id, path.to_owned()))
        .map(|(_, preview_path)| preview_path.clone())
}

fn retain_preview_copy(
    copies: &std::cell::RefCell<super::PreviewCopies>,
    note_id: carver_sdk::NoteId,
    asset_path: String,
    directory: tempfile::TempDir,
    preview_path: PathBuf,
) -> PathBuf {
    copies
        .borrow_mut()
        .insert((note_id, asset_path), (directory, preview_path.clone()));
    preview_path
}

fn prepare_copy(
    bytes: &[u8],
    path: &str,
    label: &str,
) -> Result<(tempfile::TempDir, PathBuf), PreviewCopyError> {
    let directory = tempfile::Builder::new()
        .prefix("carver-preview-")
        .tempdir()?;
    let path = directory.path().join(safe_media_filename(path, label));
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)?;
    file.write_all(bytes)?;
    file.set_permissions(std::fs::Permissions::from_mode(0o400))?;
    Ok((directory, path))
}

#[cfg(test)]
mod tests;
