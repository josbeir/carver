//! Portable rich clipboard content for complete Carver notes.

use std::{fs, path::Path};

use lol_html::{errors::RewritingError, html_content::ContentType};
use thiserror::Error;

use super::html::rewrite_managed_images;
use super::preview::{managed_asset_filename, mime_type};
use base64::{Engine as _, engine::general_purpose::STANDARD};

const MAX_EMBEDDED_IMAGE_BYTES: usize = 5 * 1024 * 1024;
const MAX_TOTAL_EMBEDDED_IMAGE_BYTES: usize = 15 * 1024 * 1024;
pub(crate) const CARVER_CLIPBOARD_MIME: &str = "application/x-carver-source";

/// Rendered clipboard representations for a complete note.
#[derive(Debug, Eq, PartialEq)]
pub(crate) struct ClipboardDocument {
    /// HTML fragment for rich destinations.
    pub(crate) html: String,
    /// Plain-text fallback for destinations without HTML support.
    pub(crate) plain_text: String,
    /// Number of managed images excluded because they were unavailable or exceeded a limit.
    pub(crate) omitted_images: usize,
}

/// Failures preparing or publishing portable clipboard content.
#[derive(Debug, Error)]
pub(crate) enum ClipboardError {
    /// The HTML rewriter could not complete the document.
    #[error("Could not prepare clipboard HTML: {0}")]
    Html(#[from] RewritingError),
    /// GTK could not publish the prepared content.
    #[error("Could not claim the clipboard: {0}")]
    Publish(#[from] glib::BoolError),
}

/// Builds portable rich and plain-text clipboard content from canonical Carve source.
///
/// # Errors
///
/// Returns an error when HTML rewriting cannot complete.
pub(crate) fn clipboard_document(
    source: &str,
    assets_dir: Option<&Path>,
) -> Result<ClipboardDocument, ClipboardError> {
    let (html, omitted_images) = embed_managed_images(&carve::to_html(source), assets_dir)?;
    Ok(ClipboardDocument {
        html,
        plain_text: carve::to_plain_text(source),
        omitted_images,
    })
}

/// Publishes a complete note as HTML with a plain-text fallback.
///
/// # Errors
///
/// Returns an error if HTML rewriting fails or GTK cannot claim the system clipboard.
pub(crate) fn publish_note(
    clipboard: &gtk::gdk::Clipboard,
    source: &str,
    assets_dir: Option<&Path>,
) -> Result<ClipboardDocument, ClipboardError> {
    let document = clipboard_document(source, assets_dir)?;
    let html = gtk::gdk::ContentProvider::for_bytes(
        "text/html",
        &glib::Bytes::from(document.html.as_bytes()),
    );
    let plain_text = gtk::gdk::ContentProvider::for_bytes(
        "text/plain;charset=utf-8",
        &glib::Bytes::from(document.plain_text.as_bytes()),
    );
    let canonical_source = gtk::gdk::ContentProvider::for_bytes(
        CARVER_CLIPBOARD_MIME,
        &glib::Bytes::from(source.as_bytes()),
    );
    let content = gtk::gdk::ContentProvider::new_union(&[html, plain_text, canonical_source]);
    clipboard.set_content(Some(&content))?;
    Ok(document)
}

fn embed_managed_images(
    html: &str,
    assets_dir: Option<&Path>,
) -> Result<(String, usize), RewritingError> {
    let mut embedded_bytes = 0_usize;
    let mut omitted_images = 0_usize;
    let output = rewrite_managed_images(html, |image, source| {
        if let Some(data_uri) = embedded_image_data_uri(source, assets_dir, &mut embedded_bytes) {
            image.set_attribute("src", &data_uri)?;
        } else {
            let alt = image.get_attribute("alt").filter(|alt| !alt.is_empty());
            let text = alt.map_or_else(
                || String::from("[Image omitted]"),
                |alt| format!("[Image: {}]", html_escape::decode_html_entities(&alt)),
            );
            image.before("<span>", ContentType::Html);
            image.replace(&text, ContentType::Text);
            image.after("</span>", ContentType::Html);
            omitted_images += 1;
        }
        Ok(())
    })?;
    Ok((output, omitted_images))
}

fn embedded_image_data_uri(
    source: &str,
    assets_dir: Option<&Path>,
    embedded_bytes: &mut usize,
) -> Option<String> {
    let filename = managed_asset_filename(source)?;
    let directory = assets_dir?;
    let path = directory.join(filename);
    let metadata = fs::metadata(&path).ok()?;
    let byte_len = usize::try_from(metadata.len()).ok()?;
    if byte_len > MAX_EMBEDDED_IMAGE_BYTES
        || embedded_bytes.saturating_add(byte_len) > MAX_TOTAL_EMBEDDED_IMAGE_BYTES
    {
        return None;
    }
    let bytes = fs::read(path).ok()?;
    if bytes.len() != byte_len {
        return None;
    }
    *embedded_bytes += byte_len;
    Some(format!(
        "data:{};base64,{}",
        mime_type(filename),
        STANDARD.encode(bytes)
    ))
}

#[cfg(test)]
mod tests;
