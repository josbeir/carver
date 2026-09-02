//! Portable rich-document primitives for native Carve editors.

#![forbid(unsafe_code)]

use thiserror::Error;

/// A document that can be represented safely by the first native WYSIWYG surface.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RichDocument {
    /// Ordered editable blocks.
    pub blocks: Vec<RichBlock>,
}

/// A supported block-level rich document element.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RichBlock {
    /// Ordinary paragraph content.
    Paragraph(Vec<RichInline>),
    /// A heading with its level and content.
    Heading {
        /// Heading level from one through six.
        level: u8,
        /// Heading content.
        content: Vec<RichInline>,
    },
    /// A bullet, numbered, or task item represented as editable text.
    ListItem {
        /// Visible list marker semantics.
        marker: ListMarker,
        /// Item content.
        content: Vec<RichInline>,
    },
    /// A fenced code block.
    CodeBlock(String),
    /// A quoted paragraph.
    Quote(Vec<RichInline>),
    /// A thematic separator.
    Rule,
}

/// A supported list marker.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ListMarker {
    /// Unordered bullet item.
    Bullet,
    /// Ordered list item.
    Ordered,
    /// Unchecked task item.
    TaskUnchecked,
    /// Checked task item.
    TaskChecked,
}

/// A supported inline rich document element.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RichInline {
    /// Plain text.
    Text(String),
    /// Bold content.
    Bold(Vec<RichInline>),
    /// Italic content.
    Italic(Vec<RichInline>),
    /// Struck-through content.
    Strike(Vec<RichInline>),
    /// Inline code.
    Code(String),
    /// Link text and destination.
    Link {
        /// Visible linked text.
        text: String,
        /// Link destination.
        destination: String,
    },
    /// A managed local image.
    Image {
        /// Alternative text retained in Carve.
        alt: String,
        /// Managed relative asset path.
        path: String,
    },
}

/// A source feature not yet supported by the native rich editor.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UnsupportedFeature {
    /// Table syntax.
    Table,
    /// Container div syntax.
    Div,
    /// Footnote syntax.
    Footnote,
    /// Definition-list syntax.
    DefinitionList,
    /// External image source.
    RemoteImage,
}

/// A rich conversion error that protects source fidelity.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum RichDocumentError {
    /// The source uses a feature without a lossless native rich representation.
    #[error("unsupported rich-text feature: {0:?}")]
    Unsupported(UnsupportedFeature),
}

/// Parses the supported Carve subset into a portable rich document.
///
/// # Errors
///
/// Returns an error rather than simplifying source when it contains unsupported structure.
pub fn parse_carve(source: &str) -> Result<RichDocument, RichDocumentError> {
    if source
        .lines()
        .any(|line| line.trim_start().starts_with('|'))
    {
        return Err(RichDocumentError::Unsupported(UnsupportedFeature::Table));
    }
    if source
        .lines()
        .any(|line| line.trim_start().starts_with(":::"))
    {
        return Err(RichDocumentError::Unsupported(UnsupportedFeature::Div));
    }
    if source.contains("[^") {
        return Err(RichDocumentError::Unsupported(UnsupportedFeature::Footnote));
    }
    let mut blocks = Vec::new();
    for line in source.lines() {
        let trimmed = line.trim_start();
        if trimmed == "---" {
            blocks.push(RichBlock::Rule);
        } else if let Some((level, content)) = heading(trimmed) {
            blocks.push(RichBlock::Heading {
                level,
                content: inline(content)?,
            });
        } else if let Some((marker, content)) = list_item(trimmed) {
            blocks.push(RichBlock::ListItem {
                marker,
                content: inline(content)?,
            });
        } else if let Some(content) = trimmed.strip_prefix("> ") {
            blocks.push(RichBlock::Quote(inline(content)?));
        } else if !line.is_empty() {
            blocks.push(RichBlock::Paragraph(inline(line)?));
        }
    }
    Ok(RichDocument { blocks })
}

/// Serializes a supported rich document to canonical Carve syntax.
#[must_use]
pub fn serialize_carve(document: &RichDocument) -> String {
    document
        .blocks
        .iter()
        .map(block_source)
        .collect::<Vec<_>>()
        .join("\n")
}

fn heading(line: &str) -> Option<(u8, &str)> {
    let count = line
        .chars()
        .take_while(|character| *character == '#')
        .count();
    let level = u8::try_from(count).ok()?;
    (count > 0 && line.as_bytes().get(count) == Some(&b' ')).then(|| (level, &line[count + 1..]))
}

fn list_item(line: &str) -> Option<(ListMarker, &str)> {
    if let Some(content) = line.strip_prefix("- [ ] ") {
        return Some((ListMarker::TaskUnchecked, content));
    }
    if let Some(content) = line.strip_prefix("- [x] ") {
        return Some((ListMarker::TaskChecked, content));
    }
    if let Some(content) = line.strip_prefix("- ") {
        return Some((ListMarker::Bullet, content));
    }
    let digits = line.chars().take_while(char::is_ascii_digit).count();
    (digits > 0 && line[digits..].starts_with(". "))
        .then(|| (ListMarker::Ordered, &line[digits + 2..]))
}

fn inline(source: &str) -> Result<Vec<RichInline>, RichDocumentError> {
    if source.contains("![") && source.contains("](http") {
        return Err(RichDocumentError::Unsupported(
            UnsupportedFeature::RemoteImage,
        ));
    }
    let mut result = Vec::new();
    let mut remainder = source;
    while !remainder.is_empty() {
        if let Some((before, alt, path, after)) = image(remainder) {
            push_text(&mut result, before);
            result.push(RichInline::Image {
                alt: alt.to_owned(),
                path: path.to_owned(),
            });
            remainder = after;
        } else if let Some((before, content, after)) = delimited(remainder, "*") {
            push_text(&mut result, before);
            result.push(RichInline::Bold(inline(content)?));
            remainder = after;
        } else if let Some((before, content, after)) = delimited(remainder, "/") {
            push_text(&mut result, before);
            result.push(RichInline::Italic(inline(content)?));
            remainder = after;
        } else {
            push_text(&mut result, remainder);
            break;
        }
    }
    Ok(result)
}

fn image(source: &str) -> Option<(&str, &str, &str, &str)> {
    let start = source.find("![")?;
    let rest = &source[start + 2..];
    let close_alt = rest.find("](")?;
    let close_path = rest[close_alt + 2..].find(')')?;
    let path_start = close_alt + 2;
    let path_end = path_start + close_path;
    Some((
        &source[..start],
        &rest[..close_alt],
        &rest[path_start..path_end],
        &rest[path_end + 1..],
    ))
}

fn delimited<'a>(source: &'a str, delimiter: &str) -> Option<(&'a str, &'a str, &'a str)> {
    let start = source.find(delimiter)?;
    let rest = &source[start + delimiter.len()..];
    let end = rest.find(delimiter)?;
    Some((
        &source[..start],
        &rest[..end],
        &rest[end + delimiter.len()..],
    ))
}

fn push_text(target: &mut Vec<RichInline>, text: &str) {
    if !text.is_empty() {
        target.push(RichInline::Text(text.to_owned()));
    }
}

fn block_source(block: &RichBlock) -> String {
    match block {
        RichBlock::Paragraph(content) => inline_source(content),
        RichBlock::Heading { level, content } => {
            format!("{} {}", "#".repeat((*level).into()), inline_source(content))
        }
        RichBlock::ListItem { marker, content } => format!(
            "{}{}",
            match marker {
                ListMarker::Bullet => "- ",
                ListMarker::Ordered => "1. ",
                ListMarker::TaskUnchecked => "- [ ] ",
                ListMarker::TaskChecked => "- [x] ",
            },
            inline_source(content)
        ),
        RichBlock::CodeBlock(content) => format!("```\n{content}\n```"),
        RichBlock::Quote(content) => format!("> {}", inline_source(content)),
        RichBlock::Rule => "---".to_owned(),
    }
}

fn inline_source(content: &[RichInline]) -> String {
    content
        .iter()
        .map(|node| match node {
            RichInline::Text(text) => text.clone(),
            RichInline::Bold(nodes) => format!("*{}*", inline_source(nodes)),
            RichInline::Italic(nodes) => format!("/{} /", inline_source(nodes)).replace(" /", "/"),
            RichInline::Strike(nodes) => format!("~{}~", inline_source(nodes)),
            RichInline::Code(text) => format!("`{text}`"),
            RichInline::Link { text, destination } => format!("[{text}]({destination})"),
            RichInline::Image { alt, path } => format!("![{alt}]({path})"),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_and_serializes_visible_bold_and_images() {
        let document = parse_carve("# Plan\n*bold* ![diagram](assets/diagram.png)")
            .unwrap_or_else(|error| panic!("parse failed: {error}"));
        assert!(serialize_carve(&document).contains("![diagram](assets/diagram.png)"));
    }
    #[test]
    fn protects_tables_from_lossy_rich_editing() {
        assert_eq!(
            parse_carve("| A | B |"),
            Err(RichDocumentError::Unsupported(UnsupportedFeature::Table))
        );
    }
}
