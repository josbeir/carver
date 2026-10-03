//! Internal note-to-note link extraction from canonical Carve source.
//!
//! Carver links notes with a stable URI destination, `carver:note/<uuid>`, on an
//! ordinary Carve link. Reading the resolved [`carve::Link::href`] rather than a
//! literal spelling means inline, full-reference, collapsed, and shortcut
//! reference forms all resolve to the same target; unresolved references carry
//! no href and heading references resolve to an in-document `#slug`, so neither
//! is collected.

use std::collections::BTreeMap;
use std::ops::Range;

use carve::{BlockNode, FigureTarget, InlineNode, Options, parse_with_options};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{NoteId, NoteSummary};

/// The URI prefix Carver uses for internal note links.
pub const NOTE_LINK_SCHEME: &str = "carver:note/";

/// One internal note reference found in canonical source.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NoteLinkRef {
    /// Referenced note.
    pub target: NoteId,
    /// Visible link label as authored.
    pub label: String,
    /// Unicode code-point range of the authored link markup.
    pub range: Range<usize>,
}

/// A note's outgoing links and the notes that link back to it.
///
/// Both directions are filtered to active notes by the storage layer.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct NoteLinks {
    /// Notes referenced by the current note.
    pub outgoing: Vec<NoteSummary>,
    /// Notes that reference the current note.
    pub backlinks: Vec<NoteSummary>,
}

impl NoteLinks {
    /// Returns whether the note has neither outgoing links nor backlinks.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.outgoing.is_empty() && self.backlinks.is_empty()
    }
}

/// Builds the canonical destination for an internal note link.
#[must_use]
pub fn note_link_destination(note_id: NoteId) -> String {
    format!("{NOTE_LINK_SCHEME}{}", note_id.as_uuid())
}

/// Parses a link destination into a note identifier.
///
/// Returns `None` for external links, malformed identifiers, and destinations
/// that carry an unsupported suffix such as a heading anchor.
#[must_use]
pub fn parse_note_link_destination(destination: &str) -> Option<NoteId> {
    let value = destination.strip_prefix(NOTE_LINK_SCHEME)?;
    Uuid::parse_str(value).ok().map(NoteId::from_uuid)
}

/// Collects the unique internal note references in canonical Carve source.
///
/// References keep their first-occurrence order, label, and source range.
#[must_use]
pub fn extract_note_links(source: &str) -> Vec<NoteLinkRef> {
    let document = parse_with_options(source, &Options::default().with_positions(true));
    let mut links = Vec::new();
    collect_blocks(&document.children, &mut links);
    for blocks in document.footnote_defs.values() {
        collect_blocks(blocks, &mut links);
    }
    let mut seen = std::collections::BTreeSet::new();
    links
        .into_iter()
        .filter(|link| seen.insert(link.target))
        .collect()
}

/// Collects the unique note identifiers referenced by canonical Carve source.
#[must_use]
pub fn extract_note_link_targets(source: &str) -> Vec<NoteId> {
    extract_note_links(source)
        .into_iter()
        .map(|link| link.target)
        .collect()
}

/// Replaces internal note-link destinations with portable targets for export.
///
/// Only the destination text of each internal link is rewritten, so a literal `carver:note/...`
/// elsewhere (for example inside a code block) is left untouched. Every extracted target is
/// substituted with the value the caller resolved for it, so an export never carries the `carver:`
/// scheme. Targets absent from `replacements` fall back to their bare identifier, which stays
/// portable but is not a `carver:` URI.
#[must_use]
pub fn rewrite_note_link_destinations(
    source: &str,
    replacements: &BTreeMap<NoteId, String>,
) -> String {
    let spans = note_link_destination_spans(source);
    if spans.is_empty() {
        return source.to_owned();
    }
    let mut result = String::with_capacity(source.len());
    let mut cursor = 0;
    for (range, target) in spans {
        if range.start < cursor || range.end > source.len() {
            continue;
        }
        result.push_str(&source[cursor..range.start]);
        let replacement = replacements
            .get(&target)
            .cloned()
            .unwrap_or_else(|| target.as_uuid().to_string());
        result.push_str(&replacement);
        cursor = range.end;
    }
    result.push_str(&source[cursor..]);
    result
}

/// Byte spans of the destination text for every internal note link, in source order.
fn note_link_destination_spans(source: &str) -> Vec<(Range<usize>, NoteId)> {
    let document = parse_with_options(source, &Options::default().with_positions(true));
    let mut links = Vec::new();
    collect_blocks(&document.children, &mut links);
    for blocks in document.footnote_defs.values() {
        collect_blocks(blocks, &mut links);
    }
    let mut definitions = Vec::new();
    collect_reference_definitions(&document.children, &mut definitions);
    for blocks in document.footnote_defs.values() {
        collect_reference_definitions(blocks, &mut definitions);
    }

    let mut spans = Vec::new();
    for link in links {
        if let Some(range) = find_destination(source, link.range, link.target) {
            spans.push((range, link.target));
        }
    }
    for (range, target) in definitions {
        if let Some(found) = find_destination(source, range, target) {
            spans.push((found, target));
        }
    }
    spans.sort_by_key(|(range, _)| range.start);
    spans
}

/// Locates the authored `carver:note/...` destination for `target` inside one node's source slice.
///
/// The span is recovered from the scheme and validated by parsing rather than by rebuilding the
/// canonical spelling, because `Uuid` also accepts uppercase and un-hyphenated forms.
fn find_destination(source: &str, range: Range<usize>, target: NoteId) -> Option<Range<usize>> {
    let range = byte_range(source, range)?;
    let slice = source.get(range.clone())?;
    let bytes = slice.as_bytes();
    let mut search = 0;
    while let Some(offset) = slice.get(search..)?.find(NOTE_LINK_SCHEME) {
        let start = search + offset;
        search = start + NOTE_LINK_SCHEME.len();
        // Only a destination follows `(`, `<`, `:`, or whitespace; a label occurrence is skipped.
        if start != 0 && !matches!(bytes[start - 1], b'(' | b'<' | b':' | b' ' | b'\t') {
            continue;
        }
        let end = slice[search..]
            .char_indices()
            .take_while(|(_, character)| character.is_ascii_hexdigit() || *character == '-')
            .map(|(index, character)| index + character.len_utf8())
            .last()
            .unwrap_or(0);
        if parse_note_link_destination(&slice[start..search + end]) == Some(target) {
            return Some(range.start + start..range.start + search + end);
        }
    }
    None
}

/// Converts a Carve `Pos` codepoint range into a byte range of `source`.
///
/// The parser records columns and offsets in Unicode codepoints, so they must be converted
/// before slicing UTF-8 text; otherwise a multibyte character before a link shifts the range
/// into the middle of a sequence and the destination is missed.
fn byte_range(source: &str, range: Range<usize>) -> Option<Range<usize>> {
    Some(byte_offset(source, range.start)?..byte_offset(source, range.end)?)
}

fn byte_offset(source: &str, codepoints: usize) -> Option<usize> {
    if codepoints == 0 {
        return Some(0);
    }
    source
        .char_indices()
        .nth(codepoints)
        .map(|(index, _)| index)
        .or_else(|| (source.chars().count() == codepoints).then_some(source.len()))
}

fn collect_reference_definitions(blocks: &[BlockNode], out: &mut Vec<(Range<usize>, NoteId)>) {
    for block in blocks {
        collect_reference_definition(block, out);
    }
}

// Mirrors the block containers walked by `collect_block` so reference-style note links have their
// definition destination rewritten too; definitions render nothing and are otherwise inert.
fn collect_reference_definition(block: &BlockNode, out: &mut Vec<(Range<usize>, NoteId)>) {
    match block {
        BlockNode::LinkReferenceDefinition(definition) => {
            if let Some(pos) = &definition.pos
                && let Some(target) = parse_note_link_destination(&definition.href)
            {
                out.push((pos.start_offset..pos.end_offset, target));
            }
        }
        BlockNode::List(node) => {
            for item in &node.items {
                collect_reference_definitions(&item.children, out);
            }
        }
        BlockNode::BlockQuote(node) => collect_reference_definitions(&node.children, out),
        BlockNode::Admonition(node) => collect_reference_definitions(&node.children, out),
        BlockNode::Div(node) => collect_reference_definitions(&node.children, out),
        BlockNode::Section(node) => collect_reference_definitions(&node.children, out),
        BlockNode::Directive(node) => collect_reference_definitions(&node.children, out),
        BlockNode::BlockExtension(node) => {
            collect_reference_definitions(node.fallback_slice(), out);
        }
        BlockNode::LineBlock(node) => collect_reference_definitions(&node.children, out),
        BlockNode::DefinitionList(node) => {
            for item in &node.items {
                for definition in &item.definitions {
                    collect_reference_definitions(&definition.children, out);
                }
            }
        }
        BlockNode::Figure(node) => {
            if let FigureTarget::BlockQuote(quote) = &*node.target {
                collect_reference_definitions(&quote.children, out);
            }
        }
        BlockNode::FigureGroup(node) => collect_reference_definitions(&node.children, out),
        BlockNode::Heading(_)
        | BlockNode::Paragraph(_)
        | BlockNode::Table(_)
        | BlockNode::CodeBlock(_)
        | BlockNode::AbbreviationDef(_)
        | BlockNode::CitationDefinition(_)
        | BlockNode::RawBlock(_)
        | BlockNode::Comment(_)
        | BlockNode::ExtensionCarrier(_)
        | BlockNode::BlockImage(_)
        | BlockNode::ThematicBreak(_) => {}
    }
}

fn collect_blocks(blocks: &[BlockNode], out: &mut Vec<NoteLinkRef>) {
    for block in blocks {
        collect_block(block, out);
    }
}

// CONTEXT: Listing the inline-bearing containers explicitly keeps the walk
// auditable and lets new Carve block variants stay inert until reviewed.
fn collect_block(block: &BlockNode, out: &mut Vec<NoteLinkRef>) {
    match block {
        BlockNode::Heading(node) => collect_inlines(&node.children, out),
        BlockNode::Paragraph(node) => collect_inlines(&node.children, out),
        BlockNode::List(node) => {
            for item in &node.items {
                collect_blocks(&item.children, out);
            }
        }
        BlockNode::BlockQuote(node) => collect_blocks(&node.children, out),
        BlockNode::Table(node) => {
            collect_optional_inlines(node.caption.as_ref(), out);
            collect_optional_inlines(node.short_caption.as_ref(), out);
            for row in &node.rows {
                for cell in &row.cells {
                    collect_inlines(&cell.children, out);
                }
            }
        }
        BlockNode::Admonition(node) => {
            collect_optional_inlines(node.title.as_ref(), out);
            collect_blocks(&node.children, out);
        }
        BlockNode::Div(node) => collect_blocks(&node.children, out),
        BlockNode::Section(node) => collect_blocks(&node.children, out),
        BlockNode::Directive(node) => {
            collect_optional_inlines(node.title.as_ref(), out);
            collect_blocks(&node.children, out);
        }
        BlockNode::BlockExtension(node) => collect_blocks(node.fallback_slice(), out),
        BlockNode::LineBlock(node) => collect_blocks(&node.children, out),
        BlockNode::DefinitionList(node) => {
            for item in &node.items {
                for term in &item.terms {
                    collect_inlines(&term.children, out);
                }
                for definition in &item.definitions {
                    collect_blocks(&definition.children, out);
                }
            }
        }
        BlockNode::Figure(node) => {
            collect_inlines(&node.caption, out);
            collect_optional_inlines(node.short_caption.as_ref(), out);
            collect_figure_target(&node.target, out);
        }
        BlockNode::FigureGroup(node) => {
            collect_blocks(&node.children, out);
            collect_optional_inlines(node.caption.as_ref(), out);
        }
        BlockNode::CodeBlock(_)
        | BlockNode::AbbreviationDef(_)
        | BlockNode::LinkReferenceDefinition(_)
        | BlockNode::CitationDefinition(_)
        | BlockNode::RawBlock(_)
        | BlockNode::Comment(_)
        | BlockNode::ExtensionCarrier(_)
        | BlockNode::BlockImage(_)
        | BlockNode::ThematicBreak(_) => {}
    }
}

fn collect_figure_target(target: &FigureTarget, out: &mut Vec<NoteLinkRef>) {
    match target {
        FigureTarget::BlockQuote(node) => collect_blocks(&node.children, out),
        FigureTarget::Table(node) => {
            for row in &node.rows {
                for cell in &row.cells {
                    collect_inlines(&cell.children, out);
                }
            }
        }
        FigureTarget::Paragraph(node) => collect_inlines(&node.children, out),
        FigureTarget::Image(_) | FigureTarget::CodeBlock(_) => {}
    }
}

fn collect_optional_inlines(nodes: Option<&Vec<InlineNode>>, out: &mut Vec<NoteLinkRef>) {
    if let Some(nodes) = nodes {
        collect_inlines(nodes, out);
    }
}

fn collect_inlines(nodes: &[InlineNode], out: &mut Vec<NoteLinkRef>) {
    for node in nodes {
        match node {
            InlineNode::Link(link) => {
                if let Some(target) = parse_note_link_destination(&link.href) {
                    out.push(NoteLinkRef {
                        target,
                        label: inline_text(&link.children),
                        range: link
                            .pos
                            .as_ref()
                            .map_or(0..0, |pos| pos.start_offset..pos.end_offset),
                    });
                }
                collect_inlines(&link.children, out);
            }
            InlineNode::Emphasis(node) => collect_inlines(&node.children, out),
            InlineNode::Ruby(node) => {
                for pair in &node.pairs {
                    collect_inlines(&pair.base, out);
                    collect_inlines(&pair.annotation, out);
                }
            }
            InlineNode::Span(node) => collect_inlines(&node.children, out),
            InlineNode::Extension(node) => collect_inlines(&node.children, out),
            InlineNode::CriticInsert(node) => collect_inlines(&node.children, out),
            InlineNode::CriticDelete(node) => collect_inlines(&node.children, out),
            InlineNode::CriticSubstitute(node) => {
                collect_inlines(&node.old, out);
                collect_inlines(&node.new, out);
            }
            InlineNode::Footnote(node) => {
                collect_optional_inlines(node.inline.as_ref(), out);
            }
            InlineNode::Text(_)
            | InlineNode::EscapedText(_)
            | InlineNode::SmartPunctuation(_)
            | InlineNode::Code(_)
            | InlineNode::Image(_)
            | InlineNode::Math(_)
            | InlineNode::RawInline(_)
            | InlineNode::LiteralInline(_)
            | InlineNode::Symbol(_)
            | InlineNode::AutoLink(_)
            | InlineNode::CrossRef(_)
            | InlineNode::CaptionNumber(_)
            | InlineNode::Mention(_)
            | InlineNode::Tag(_)
            | InlineNode::CitationGroup(_)
            | InlineNode::Abbreviation(_)
            | InlineNode::NonBreakingSpace(_)
            | InlineNode::SoftBreak(_)
            | InlineNode::HardBreak(_)
            | InlineNode::CriticComment(_)
            | InlineNode::Comment(_) => {}
        }
    }
}

fn inline_text(nodes: &[InlineNode]) -> String {
    nodes
        .iter()
        .map(|node| match node {
            InlineNode::Text(node) => node.value.clone(),
            InlineNode::EscapedText(node) => node.value.clone(),
            InlineNode::SmartPunctuation(node) => {
                node.glyph.as_ref().unwrap_or(&node.value).clone()
            }
            InlineNode::Emphasis(node) => inline_text(&node.children),
            InlineNode::Ruby(node) => inline_text(&node.flattened()),
            InlineNode::NonBreakingSpace(_) => String::from("\u{a0}"),
            InlineNode::Span(node) => inline_text(&node.children),
            InlineNode::Link(node) => inline_text(&node.children),
            InlineNode::Image(node) => node.alt.clone(),
            InlineNode::Extension(node) => inline_text(&node.children),
            InlineNode::CriticInsert(node) => inline_text(&node.children),
            InlineNode::CriticDelete(node) => inline_text(&node.children),
            InlineNode::CriticSubstitute(node) => inline_text(&node.new),
            InlineNode::Code(node) => node.value.clone(),
            InlineNode::AutoLink(node) => node.text.clone(),
            InlineNode::Math(node) => node.content.clone(),
            InlineNode::LiteralInline(node) => node.content.clone(),
            InlineNode::Abbreviation(node) => node.abbr.clone(),
            InlineNode::SoftBreak(_) | InlineNode::HardBreak(_) => String::from(" "),
            InlineNode::Footnote(node) => node
                .inline
                .as_ref()
                .map_or_else(String::new, |children| inline_text(children)),
            _ => String::new(),
        })
        .collect()
}

#[cfg(test)]
mod tests;
