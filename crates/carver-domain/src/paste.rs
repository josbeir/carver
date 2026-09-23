//! Format detection and conversion for text pasted into an editor.
//!
//! Carve and Markdown overlap, so automatic detection gives canonical Carve
//! source priority. Markdown conversion is available when the caller explicitly
//! requests it.

use carve::{BlockNode, Document, InlineNode, Options, parse_with_options};

use crate::{DocumentImportFormat, import_document};

/// The perceived format of pasted text.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PastedFormat {
    /// Unstructured text that must be inserted verbatim.
    Plain,
    /// Canonical Carve markup.
    Carve,
    /// CommonMark-compatible Markdown converted through Carve's migration.
    Markdown,
}

/// The caller's preferred interpretation of pasted text.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PasteIntent {
    /// Detect the format from the text itself.
    Auto,
    /// Treat the text as Carve markup.
    Carve,
    /// Treat the text as Markdown and migrate it.
    Markdown,
}

/// Canonical Carve source produced from a paste and the format used for it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PastedDocument {
    /// Canonical Carve source suitable for persistence or rich projection.
    pub source: String,
    /// Interpretation applied to the pasted text.
    pub format: PastedFormat,
}

/// Upper bound on text parsed for detection.
///
/// Larger pastes are inserted verbatim so a single paste cannot block the UI
/// thread on an adversarial document.
const MAX_CLASSIFIED_LENGTH: usize = 256 * 1024;

/// Converts pasted text into canonical Carve, detecting its format when requested.
#[must_use]
pub fn import_pasted_text(text: &str, intent: PasteIntent) -> PastedDocument {
    let format = match intent {
        PasteIntent::Carve => PastedFormat::Carve,
        PasteIntent::Markdown => PastedFormat::Markdown,
        PasteIntent::Auto => detect_pasted_format(text),
    };
    match format {
        PastedFormat::Plain => PastedDocument {
            source: text.to_owned(),
            format,
        },
        PastedFormat::Carve => PastedDocument {
            source: import_document(text, DocumentImportFormat::Carve),
            format,
        },
        PastedFormat::Markdown => PastedDocument {
            source: import_document(text, DocumentImportFormat::Markdown),
            format,
        },
    }
}

/// Classifies pasted text without converting it.
#[must_use]
pub fn detect_pasted_format(text: &str) -> PastedFormat {
    if text.trim().is_empty() || text.len() > MAX_CLASSIFIED_LENGTH {
        return PastedFormat::Plain;
    }
    if carve_is_structured(text) {
        PastedFormat::Carve
    } else {
        PastedFormat::Plain
    }
}

/// Returns whether Carve parsing turns the text into real markup.
fn carve_is_structured(text: &str) -> bool {
    let document = parse_with_options(text, &Options::default());
    document_is_structured(&document)
}

fn document_is_structured(document: &Document) -> bool {
    if !document.frontmatter.is_empty() || document.frontmatter_raw.is_some() {
        return true;
    }
    if document.children.len() > 1 {
        return true;
    }
    document.children.iter().any(block_is_structured)
}

fn block_is_structured(block: &BlockNode) -> bool {
    match block {
        BlockNode::Paragraph(paragraph) => {
            paragraph.attrs.is_some() || paragraph.children.iter().any(inline_is_structured)
        }
        _ => true,
    }
}

fn inline_is_structured(inline: &InlineNode) -> bool {
    !matches!(
        inline,
        InlineNode::Text(_)
            | InlineNode::EscapedText(_)
            | InlineNode::SmartPunctuation(_)
            | InlineNode::SoftBreak(_)
            | InlineNode::HardBreak(_)
    )
}

#[cfg(test)]
mod tests;
