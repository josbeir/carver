//! UI-neutral conversion and packaging for Carver note exports.

#![forbid(unsafe_code)]

use std::{
    collections::BTreeSet,
    io::{Cursor, Write},
    path::{Component, Path},
};

use carve::{CheckedRenderOptions, to_markdown_with_report};
use thiserror::Error;
use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

/// A directly writable note export format.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExportFormat {
    /// Canonical Carve source.
    Carve,
    /// Markdown converted by Carve's native renderer.
    Markdown,
}

impl ExportFormat {
    /// Returns the standard filename extension for the format.
    #[must_use]
    pub const fn extension(self) -> &'static str {
        match self {
            Self::Carve => "crv",
            Self::Markdown => "md",
        }
    }
}

/// One managed asset available for a portable archive.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ManagedAsset {
    /// Source-relative asset location, such as `assets/photo.png`.
    pub path: String,
    /// Original managed asset bytes.
    pub bytes: Vec<u8>,
}

/// A non-fatal condition that requires user acknowledgement before writing.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExportWarning {
    /// Carve conversion omitted constructs unsupported by Markdown.
    MarkdownLoss {
        /// Number of omitted constructs.
        count: usize,
    },
    /// A portable archive could not include one referenced managed image.
    MissingManagedAsset {
        /// Source-relative managed asset location.
        path: String,
    },
}

impl std::fmt::Display for ExportWarning {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MarkdownLoss { count } => write!(
                formatter,
                "Markdown cannot represent {count} Carve construct{} exactly.",
                if *count == 1 { "" } else { "s" }
            ),
            Self::MissingManagedAsset { path } => {
                write!(
                    formatter,
                    "The managed image `{path}` could not be included."
                )
            }
        }
    }
}

/// Prepared file bytes and any conditions that require user confirmation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExportArtifact {
    /// Complete bytes ready to write to the selected destination.
    pub bytes: Vec<u8>,
    /// File extension selected for the artifact.
    pub extension: &'static str,
    /// Warnings collected while converting or packaging.
    pub warnings: Vec<ExportWarning>,
}

/// Failures while converting or packaging an export.
#[derive(Debug, Error)]
pub enum ExportError {
    /// The Markdown renderer failed before producing an artifact.
    #[error("Could not convert the note to Markdown: {0}")]
    Markdown(#[from] carve::RenderLossError),
    /// A ZIP archive could not be completed.
    #[error("Could not create the portable archive: {0}")]
    Archive(#[from] zip::result::ZipError),
    /// An archive filename was invalid.
    #[error("The export filename is invalid")]
    InvalidFilename,
    /// Writing the ZIP stream failed.
    #[error("Could not write the portable archive: {0}")]
    Io(#[from] std::io::Error),
}

/// Returns the safe managed asset paths referenced by a Carve document.
#[must_use]
pub fn managed_asset_paths(source: &str) -> Vec<String> {
    let html = carve::to_html(source);
    let mut paths = BTreeSet::new();
    let mut remaining = html.as_str();
    while let Some(image_start) = remaining.find("<img ") {
        let image_and_rest = &remaining[image_start..];
        let Some(image_end) = image_and_rest.find('>') else {
            break;
        };
        let image = &image_and_rest[..=image_end];
        if let Some(path) = attribute(image, "src").filter(|path| is_managed_asset_path(path)) {
            paths.insert(path.to_owned());
        }
        remaining = &image_and_rest[image_end + 1..];
    }
    paths.into_iter().collect()
}

/// Prepares direct document bytes or a ZIP archive containing managed assets.
///
/// When `include_assets` is false, managed paths remain unchanged in the document.
/// When it is true, the archive contains the document at its root and available assets in their
/// original `assets/` paths.
///
/// # Errors
///
/// Returns an error when Markdown conversion or archive construction fails.
pub fn prepare_export(
    source: &str,
    document_stem: &str,
    format: ExportFormat,
    include_assets: bool,
    assets: &[ManagedAsset],
) -> Result<ExportArtifact, ExportError> {
    let (document, mut warnings) = document_bytes(source, format)?;
    if !include_assets {
        return Ok(ExportArtifact {
            bytes: document,
            extension: format.extension(),
            warnings,
        });
    }

    let document_name = archive_document_name(document_stem, format)?;
    let expected = managed_asset_paths(source);
    let available: std::collections::BTreeMap<_, _> = assets
        .iter()
        .filter(|asset| is_managed_asset_path(&asset.path))
        .map(|asset| (asset.path.as_str(), asset.bytes.as_slice()))
        .collect();
    let archive = Cursor::new(Vec::new());
    let mut writer = ZipWriter::new(archive);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    writer.start_file(document_name, options)?;
    writer.write_all(&document)?;
    for path in expected {
        let Some(bytes) = available.get(path.as_str()) else {
            warnings.push(ExportWarning::MissingManagedAsset { path });
            continue;
        };
        writer.start_file(path, options)?;
        writer.write_all(bytes)?;
    }
    let bytes = writer.finish()?.into_inner();
    Ok(ExportArtifact {
        bytes,
        extension: "zip",
        warnings,
    })
}

fn document_bytes(
    source: &str,
    format: ExportFormat,
) -> Result<(Vec<u8>, Vec<ExportWarning>), ExportError> {
    match format {
        ExportFormat::Carve => Ok((source.as_bytes().to_vec(), Vec::new())),
        ExportFormat::Markdown => {
            let result = to_markdown_with_report(source, CheckedRenderOptions::default())?;
            let warnings = (result.total_losses > 0)
                .then_some(ExportWarning::MarkdownLoss {
                    count: result.total_losses,
                })
                .into_iter()
                .collect();
            Ok((result.value.into_bytes(), warnings))
        }
    }
}

fn archive_document_name(document_stem: &str, format: ExportFormat) -> Result<String, ExportError> {
    let stem = sanitized_filename_stem(document_stem);
    if stem.is_empty() {
        return Err(ExportError::InvalidFilename);
    }
    Ok(format!("{stem}.{}", format.extension()))
}

/// Returns a safe desktop filename stem with a readable fallback.
#[must_use]
pub fn sanitized_filename_stem(title: &str) -> String {
    let value = title
        .trim()
        .chars()
        .map(|character| match character {
            '/' | '\\' | ':' | '\0' => '-',
            character if character.is_control() => ' ',
            character => character,
        })
        .collect::<String>();
    let value = value.trim_matches([' ', '.']).trim();
    if value.is_empty() {
        String::from("Untitled Note")
    } else {
        value.chars().take(120).collect()
    }
}

fn is_managed_asset_path(path: &str) -> bool {
    let Some(relative) = path.strip_prefix("assets/") else {
        return false;
    };
    !relative.is_empty()
        && Path::new(relative)
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
}

fn attribute<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let prefix = format!("{name}=\"");
    let value = tag.split_once(&prefix)?.1;
    value.split_once('"').map(|(value, _)| value)
}

#[cfg(test)]
mod tests {
    use std::io::{Cursor, Read};

    use super::*;

    #[test]
    fn carve_export_should_preserve_the_current_source() -> Result<(), ExportError> {
        let artifact = prepare_export("# Draft\n\nText", "Draft", ExportFormat::Carve, false, &[])?;

        assert_eq!(artifact.bytes, b"# Draft\n\nText");
        assert_eq!(artifact.extension, "crv");
        Ok(())
    }

    #[test]
    fn markdown_export_should_use_the_native_carve_codec() -> Result<(), ExportError> {
        let artifact = prepare_export(
            "# Draft\n\n/Emphasis/",
            "Draft",
            ExportFormat::Markdown,
            false,
            &[],
        )?;

        assert_eq!(artifact.extension, "md");
        assert!(String::from_utf8_lossy(&artifact.bytes).contains("# Draft"));
        assert!(artifact.warnings.is_empty());
        Ok(())
    }

    #[test]
    fn markdown_export_should_warn_for_dropped_target_specific_raw_content()
    -> Result<(), ExportError> {
        let artifact = prepare_export(
            "```=latex\n\\textbf{only LaTeX}\n```",
            "Draft",
            ExportFormat::Markdown,
            false,
            &[],
        )?;

        assert!(matches!(
            artifact.warnings.as_slice(),
            [ExportWarning::MarkdownLoss { count }] if *count > 0
        ));
        Ok(())
    }

    #[test]
    fn export_warnings_should_explain_singular_plural_and_missing_asset_cases() {
        assert_eq!(
            ExportWarning::MarkdownLoss { count: 1 }.to_string(),
            "Markdown cannot represent 1 Carve construct exactly."
        );
        assert_eq!(
            ExportWarning::MarkdownLoss { count: 2 }.to_string(),
            "Markdown cannot represent 2 Carve constructs exactly."
        );
        assert_eq!(
            ExportWarning::MissingManagedAsset {
                path: "assets/missing.png".to_owned()
            }
            .to_string(),
            "The managed image `assets/missing.png` could not be included."
        );
    }

    #[test]
    fn portable_export_should_include_referenced_managed_assets()
    -> Result<(), Box<dyn std::error::Error>> {
        let artifact = prepare_export(
            "![Diagram](assets/diagram.png)",
            "Diagram",
            ExportFormat::Carve,
            true,
            &[ManagedAsset {
                path: String::from("assets/diagram.png"),
                bytes: vec![1, 2, 3],
            }],
        )?;
        let mut archive = zip::ZipArchive::new(Cursor::new(artifact.bytes))?;
        let mut image = Vec::new();
        archive
            .by_name("assets/diagram.png")?
            .read_to_end(&mut image)?;

        assert_eq!(image, [1, 2, 3]);
        Ok(())
    }

    #[test]
    fn portable_export_should_warn_when_a_managed_asset_is_missing() -> Result<(), ExportError> {
        let artifact = prepare_export(
            "![Diagram](assets/missing.png)",
            "Draft",
            ExportFormat::Carve,
            true,
            &[],
        )?;

        assert_eq!(
            artifact.warnings,
            vec![ExportWarning::MissingManagedAsset {
                path: String::from("assets/missing.png")
            }]
        );
        Ok(())
    }

    #[test]
    fn managed_asset_paths_should_exclude_external_and_traversal_paths() {
        let paths = managed_asset_paths(
            "![A](assets/photo.png) ![B](https://example.test/a.png) ![C](assets/../private.png)",
        );

        assert_eq!(paths, vec![String::from("assets/photo.png")]);
    }

    #[test]
    fn filename_stem_should_replace_unsafe_path_characters() {
        assert_eq!(
            sanitized_filename_stem(" /Client: brief/ "),
            "-Client- brief-"
        );
        assert_eq!(sanitized_filename_stem(". \n\t"), "Untitled Note");
    }
}
