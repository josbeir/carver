//! SDK reads and isolated temporary copies for external media viewers.

use std::{
    io::Write,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
};

use super::{AppMsg, AppRuntime, LibraryBackend, UiError, display_error, gio};

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
                    Ok(None) => Err(UiError::new("This file is no longer available.")),
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
    let path = directory.path().join(preview_filename(path, label));
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)?;
    file.write_all(bytes)?;
    file.set_permissions(std::fs::Permissions::from_mode(0o400))?;
    Ok((directory, path))
}

fn preview_filename(path: &str, label: &str) -> String {
    const MAX_FILENAME_BYTES: usize = 255;
    let name: String = label
        .chars()
        .map(|character| {
            if character.is_alphanumeric() || matches!(character, ' ' | '-' | '_' | '.') {
                character
            } else {
                '_'
            }
        })
        .collect();
    let name = name.trim_matches([' ', '.']);
    let suffix = Path::new(path)
        .extension()
        .and_then(|value| value.to_str())
        .map(|extension| format!(".{extension}"));
    let stem = suffix
        .as_deref()
        .and_then(|suffix| name.strip_suffix(suffix))
        .unwrap_or(name)
        .trim_matches([' ', '.']);
    let suffix_bytes = suffix.as_ref().map_or(0, String::len);
    let mut filename = truncate_utf8(stem, MAX_FILENAME_BYTES.saturating_sub(suffix_bytes));
    if filename.is_empty() {
        filename = String::from("Attachment");
    }
    filename.push_str(suffix.as_deref().unwrap_or_default());
    filename
}

/// Truncates on a UTF-8 boundary so preview filenames fit one filesystem component.
fn truncate_utf8(value: &str, max_bytes: usize) -> String {
    value
        .chars()
        .scan(0, |bytes, character| {
            let character_bytes = character.len_utf8();
            (*bytes + character_bytes <= max_bytes).then(|| {
                *bytes += character_bytes;
                character
            })
        })
        .collect()
}

#[cfg(test)]
mod tests;
