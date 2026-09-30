//! Safe filenames for managed assets handed to external applications.

use std::path::Path;

/// Builds a filesystem-safe filename for a managed asset, preferring its authored label.
///
/// The asset's original extension is preserved and the byte length is bounded so the result
/// stays within a single filesystem component. The preview copy and the native save dialog
/// share this so both suggest the same name.
pub(crate) fn safe_media_filename(path: &str, label: &str) -> String {
    const MAX_FILENAME_BYTES: usize = 255;
    let name = label.trim().trim_matches([' ', '.']);
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
    let mut filename = carver_export::filename::sanitized_component(
        stem,
        MAX_FILENAME_BYTES.saturating_sub(suffix_bytes),
    );
    if filename.is_empty() {
        filename = String::from("Attachment");
    }
    filename.push_str(suffix.as_deref().unwrap_or_default());
    filename
}

#[cfg(test)]
mod tests;
