//! Format detection and conversion for text pasted into an editor.
//!
//! Carve and Markdown overlap: `/italic/` and `*bold*` are Carve, while
//! `*italic*` and `**bold**` are Markdown. A pasted fragment therefore cannot
//! be classified with certainty, so detection is deliberately conservative:
//! text without recognizable markup is inserted verbatim, and only unambiguous
//! Markdown syntax is migrated. Ambiguous single delimiters stay Carve.

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
    if contains_markdown_only_syntax(text) {
        return PastedFormat::Markdown;
    }
    if carve_is_structured(text) {
        PastedFormat::Carve
    } else {
        PastedFormat::Plain
    }
}

/// Returns whether the text uses syntax that Markdown and Carve do not share.
///
/// `**`, `__`, `~~`, a `=` setext underline, and a GFM table separator all mean
/// something different (or nothing) in Carve, so they are safe signals. Single
/// `*` and `_` are omitted: `*bold*` is valid Carve and must stay Carve.
fn contains_markdown_only_syntax(text: &str) -> bool {
    if text.contains("**") || text.contains("__") || text.contains("~~") {
        return true;
    }
    let mut previous_content = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            previous_content = false;
            continue;
        }
        if previous_content && is_setext_underline(trimmed) {
            return true;
        }
        if is_markdown_table_separator(trimmed) {
            return true;
        }
        previous_content = true;
    }
    false
}

/// Returns whether a line is a Markdown setext underline (`===`).
fn is_setext_underline(line: &str) -> bool {
    line.chars().all(|character| character == '=')
}

/// Returns whether a line is a GFM table separator row (`| --- | :--: |`).
fn is_markdown_table_separator(line: &str) -> bool {
    line.contains('|')
        && line.contains('-')
        && line
            .chars()
            .all(|character| matches!(character, '|' | '-' | ':' | ' '))
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
