//! Portable rich clipboard content for complete Carver notes.

use std::{fs, path::Path};

use super::preview::{managed_asset_filename, mime_type};
use base64::{Engine as _, engine::general_purpose::STANDARD};

const MAX_EMBEDDED_IMAGE_BYTES: usize = 5 * 1024 * 1024;
const MAX_TOTAL_EMBEDDED_IMAGE_BYTES: usize = 15 * 1024 * 1024;

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

/// Builds portable rich and plain-text clipboard content from canonical Carve source.
pub(crate) fn clipboard_document(source: &str, assets_dir: Option<&Path>) -> ClipboardDocument {
    let (html, omitted_images) = embed_managed_images(&carve::to_html(source), assets_dir);
    ClipboardDocument {
        html,
        plain_text: carve::to_plain_text(source),
        omitted_images,
    }
}

/// Publishes a complete note as HTML with a plain-text fallback.
///
/// # Errors
///
/// Returns an error if GTK cannot claim the system clipboard.
pub(crate) fn publish_note(
    clipboard: &gtk::gdk::Clipboard,
    source: &str,
    assets_dir: Option<&Path>,
) -> Result<ClipboardDocument, glib::BoolError> {
    let document = clipboard_document(source, assets_dir);
    let html = gtk::gdk::ContentProvider::for_bytes(
        "text/html",
        &glib::Bytes::from(document.html.as_bytes()),
    );
    let plain_text = gtk::gdk::ContentProvider::for_bytes(
        "text/plain;charset=utf-8",
        &glib::Bytes::from(document.plain_text.as_bytes()),
    );
    let content = gtk::gdk::ContentProvider::new_union(&[html, plain_text]);
    clipboard.set_content(Some(&content))?;
    Ok(document)
}

fn embed_managed_images(html: &str, assets_dir: Option<&Path>) -> (String, usize) {
    let mut remaining = html;
    let mut embedded_bytes = 0_usize;
    let mut omitted_images = 0_usize;
    let mut output = String::with_capacity(html.len());

    while let Some(image_start) = remaining.find("<img ") {
        let (before_image, image_and_rest) = remaining.split_at(image_start);
        output.push_str(before_image);
        let Some(image_end) = image_and_rest.find('>') else {
            output.push_str(image_and_rest);
            return (output, omitted_images);
        };
        let (image, rest) = image_and_rest.split_at(image_end + 1);
        match image_source(image) {
            ImageSource::Managed(source) => {
                if let Some(data_uri) =
                    embedded_image_data_uri(source, assets_dir, &mut embedded_bytes)
                {
                    output.push_str(&image.replacen(
                        &format!("src=\"{source}\""),
                        &format!("src=\"{data_uri}\""),
                        1,
                    ));
                } else {
                    output.push_str(&omitted_image_text(image));
                    omitted_images += 1;
                }
            }
            ImageSource::InvalidManaged => {
                output.push_str(&omitted_image_text(image));
                omitted_images += 1;
            }
            ImageSource::External => output.push_str(image),
        }
        remaining = rest;
    }
    output.push_str(remaining);
    (output, omitted_images)
}

enum ImageSource<'a> {
    External,
    Managed(&'a str),
    InvalidManaged,
}

fn image_source(image: &str) -> ImageSource<'_> {
    let Some(source) = attribute(image, "src") else {
        return ImageSource::External;
    };
    if !source.starts_with("assets/") {
        return ImageSource::External;
    }
    if managed_asset_filename(source).is_some() {
        ImageSource::Managed(source)
    } else {
        ImageSource::InvalidManaged
    }
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

fn omitted_image_text(image: &str) -> String {
    let alt = attribute(image, "alt").filter(|alt| !alt.is_empty());
    match alt {
        Some(alt) => format!("<span>[Image: {alt}]</span>"),
        None => String::from("<span>[Image omitted]</span>"),
    }
}

fn attribute<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let prefix = format!("{name}=\"");
    let value = tag.split_once(&prefix)?.1;
    value.split_once('"').map(|(value, _)| value)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn clipboard_document_should_embed_a_small_managed_image()
    -> Result<(), Box<dyn std::error::Error>> {
        let directory = tempfile::tempdir()?;
        fs::write(directory.path().join("example.png"), [1_u8, 2, 3])?;

        let document = clipboard_document("![Diagram](assets/example.png)", Some(directory.path()));

        assert!(document.html.contains("src=\"data:image/png;base64,AQID\""));
        assert_eq!(document.plain_text.trim(), "Diagram");
        assert_eq!(document.omitted_images, 0);
        Ok(())
    }

    #[test]
    fn clipboard_document_should_preserve_external_images() {
        let document = clipboard_document("![Logo](https://example.test/logo.png)", None);

        assert!(
            document
                .html
                .contains("src=\"https://example.test/logo.png\"")
        );
        assert_eq!(document.omitted_images, 0);
    }

    #[test]
    fn clipboard_document_should_replace_missing_managed_images_with_alt_text() {
        let document = clipboard_document("![Diagram](assets/missing.png)", None);

        assert!(document.html.contains("[Image: Diagram]"));
        assert_eq!(document.omitted_images, 1);
    }

    #[test]
    fn clipboard_document_should_replace_invalid_managed_asset_paths_with_alt_text() {
        let document = clipboard_document("![Private](assets/../library.sqlite3)", None);

        assert!(document.html.contains("[Image: Private]"));
        assert_eq!(document.omitted_images, 1);
    }

    #[test]
    fn clipboard_document_should_omit_managed_images_over_the_per_image_limit()
    -> Result<(), Box<dyn std::error::Error>> {
        let directory = tempfile::tempdir()?;
        fs::write(
            directory.path().join("large.png"),
            vec![0_u8; MAX_EMBEDDED_IMAGE_BYTES + 1],
        )?;

        let document = clipboard_document("![Large](assets/large.png)", Some(directory.path()));

        assert!(document.html.contains("[Image: Large]"));
        assert_eq!(document.omitted_images, 1);
        Ok(())
    }

    #[test]
    fn clipboard_document_should_limit_total_embedded_image_bytes()
    -> Result<(), Box<dyn std::error::Error>> {
        let directory = tempfile::tempdir()?;
        let image = vec![0_u8; MAX_EMBEDDED_IMAGE_BYTES];
        for index in 1..=4 {
            fs::write(directory.path().join(format!("{index}.png")), &image)?;
        }

        let document = clipboard_document(
            "![One](assets/1.png) ![Two](assets/2.png) ![Three](assets/3.png) ![Four](assets/4.png)",
            Some(directory.path()),
        );

        assert_eq!(document.html.matches("data:image/png;base64,").count(), 3);
        assert!(document.html.contains("[Image: Four]"));
        assert_eq!(document.omitted_images, 1);
        Ok(())
    }
}
