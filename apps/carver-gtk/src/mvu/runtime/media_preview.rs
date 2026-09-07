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
            let result = match client.note_asset_bytes_async(note_id, path.clone()).await {
                Ok(Some(bytes)) => gio::spawn_blocking(move || prepare_copy(&bytes, &path, &label))
                    .await
                    .map_err(|_| UiError::new("Could not prepare the file preview."))
                    .and_then(|result| result.map_err(display_error)),
                Ok(None) => Err(UiError::new("This file is no longer available.")),
                Err(error) => Err(display_error(error)),
            };
            if runtime
                .model()
                .editor
                .as_ref()
                .is_none_or(|document| document.session != session)
            {
                return;
            }
            let result = result.map(|(directory, path)| {
                runtime.inner.preview_copies.borrow_mut().push(directory);
                path
            });
            runtime.dispatch(AppMsg::Editor(
                super::super::EditorMsg::MediaPreviewPrepared { session, result },
            ));
        });
    }
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
    let name: String = label
        .chars()
        .take(100)
        .map(|character| {
            if character.is_alphanumeric() || matches!(character, ' ' | '-' | '_' | '.') {
                character
            } else {
                '_'
            }
        })
        .collect();
    let mut name = name.trim_matches([' ', '.']).to_owned();
    if name.is_empty() {
        name = String::from("Attachment");
    }
    if let Some(extension) = Path::new(path).extension().and_then(|value| value.to_str())
        && !name.ends_with(&format!(".{extension}"))
    {
        name.push('.');
        name.push_str(extension);
    }
    name
}

#[cfg(test)]
mod tests;
