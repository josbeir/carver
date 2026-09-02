//! Portable rich-document primitives for native Carve editors.

#![forbid(unsafe_code)]

use carve::{BlockNode, Document, InlineNode, parse};
use thiserror::Error;

/// The editor surface that can represent a Carve document without data loss.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EditorProjection {
    /// A document supported by the native editable GTK editor.
    Native(RichDocument),
    /// A full-fidelity rendered preview is required; source remains editable.
    RenderedOnly,
}

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
    /// A fenced code block, retaining its optional language identifier.
    CodeBlock {
        /// Optional language identifier following the opening fence.
        language: Option<String>,
        /// Literal code content without its fences.
        content: String,
    },
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
    /// Underlined content.
    Underline(Vec<RichInline>),
    /// Highlighted content.
    Highlight(Vec<RichInline>),
    /// Superscript content.
    Superscript(Vec<RichInline>),
    /// Subscript content.
    Subscript(Vec<RichInline>),
    /// Inserted content.
    Inserted(Vec<RichInline>),
    /// Deleted content.
    Deleted(Vec<RichInline>),
    /// Inline code.
    Code(String),
    /// Link text and destination.
    Link {
        /// Visible linked text.
        text: String,
        /// Link destination.
        destination: String,
    },
    /// Text carrying a Carve attribute list.
    Attribute {
        /// Visible text.
        text: String,
        /// Attribute list contents without its surrounding braces.
        attributes: String,
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
    // `str::lines` deliberately omits empty and trailing lines. Those lines are
    // editable paragraph breaks, so the native representation must retain them.
    let mut lines = source.split('\n').peekable();
    while let Some(line) = lines.next() {
        let trimmed = line.trim_start();
        if let Some(language) = trimmed.strip_prefix("```") {
            let language = (!language.trim().is_empty()).then(|| language.trim().to_owned());
            let mut content = Vec::new();
            let mut closed = false;
            for code_line in lines.by_ref() {
                if code_line.trim() == "```" {
                    closed = true;
                    break;
                }
                content.push(code_line);
            }
            if closed {
                blocks.push(RichBlock::CodeBlock {
                    language,
                    content: content.join("\n"),
                });
            } else {
                blocks.push(RichBlock::Paragraph(inline(line)?));
                blocks.extend(
                    content
                        .into_iter()
                        .map(inline)
                        .collect::<Result<Vec<_>, _>>()?
                        .into_iter()
                        .map(RichBlock::Paragraph),
                );
            }
        } else if trimmed == "---" {
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
        } else {
            blocks.push(RichBlock::Paragraph(inline(line)?));
        }
    }
    Ok(RichDocument { blocks })
}

/// Classifies Carve source for the native editor using the canonical Carve AST.
///
/// The native editor deliberately declines constructs whose structure it cannot
/// serialize faithfully; callers use the full rendered projection for them.
#[must_use]
pub fn editor_projection(source: &str) -> EditorProjection {
    let document = parse(source);
    if native_document(&document) {
        parse_carve(source).map_or(EditorProjection::RenderedOnly, EditorProjection::Native)
    } else {
        EditorProjection::RenderedOnly
    }
}

/// Renders complete Carve source to HTML for a read-only frontend surface.
#[must_use]
pub fn render_html(source: &str) -> String {
    carve::to_html(source)
}

fn native_document(document: &Document) -> bool {
    document.frontmatter.is_empty()
        && document.footnote_defs.is_empty()
        && document.children.iter().all(native_block)
}

fn native_block(block: &BlockNode) -> bool {
    match block {
        // The canonical parser materializes a generated id on ordinary headings,
        // so heading attributes alone cannot distinguish authored extra syntax.
        BlockNode::Heading(heading) => native_inlines(&heading.children),
        BlockNode::Paragraph(paragraph) => {
            paragraph.attrs.is_none() && native_inlines(&paragraph.children)
        }
        BlockNode::CodeBlock(code) => {
            code.attrs.is_none() && code.title.is_none() && code.label.is_none()
        }
        BlockNode::List(list) => {
            list.attrs.is_none()
                && list.items.iter().all(|item| {
                    item.attrs.is_none()
                        && item.children.len() == 1
                        && item.children.iter().all(native_block)
                })
        }
        BlockNode::BlockQuote(quote) => {
            quote.attrs.is_none() && !quote.fenced && quote.children.iter().all(native_block)
        }
        // The canonical parser promotes a line containing only an image into a
        // block image. The native projection already preserves that source as
        // an image paragraph, so managed standalone images remain editable.
        BlockNode::BlockImage(image) => {
            image.attrs.is_none()
                && image.title.is_none()
                && image.ref_label.is_none()
                && image.raw_ref.is_none()
                && image.src.starts_with("assets/")
        }
        BlockNode::ThematicBreak(rule) => rule.attrs.is_none(),
        _ => false,
    }
}

fn native_inlines(nodes: &[InlineNode]) -> bool {
    nodes.iter().all(|node| match node {
        InlineNode::Text(_)
        | InlineNode::EscapedText(_)
        | InlineNode::SmartPunctuation(_)
        | InlineNode::SoftBreak(_) => true,
        InlineNode::Emphasis(emphasis) => {
            emphasis.attrs.is_none() && native_inlines(&emphasis.children)
        }
        InlineNode::Code(code) => code.attrs.is_none(),
        InlineNode::Link(link) => {
            link.attrs.is_none()
                && link.title.is_none()
                && link.ref_label.is_none()
                && link.raw_ref.is_none()
                && native_inlines(&link.children)
        }
        InlineNode::Image(image) => {
            image.attrs.is_none()
                && image.title.is_none()
                && image.ref_label.is_none()
                && image.raw_ref.is_none()
                && image.src.starts_with("assets/")
        }
        InlineNode::CriticInsert(change) => {
            change.attrs.is_none() && native_inlines(&change.children)
        }
        InlineNode::CriticDelete(change) => {
            change.attrs.is_none() && native_inlines(&change.children)
        }
        _ => false,
    })
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
        let Some(position) = next_inline_start(remainder) else {
            push_text(&mut result, remainder);
            break;
        };
        push_text(&mut result, &remainder[..position]);
        remainder = &remainder[position..];
        if let Some((node, after)) = inline_at_start(remainder)? {
            result.push(node);
            remainder = after;
        } else if let Some(character) = remainder.chars().next() {
            push_text(&mut result, &remainder[..character.len_utf8()]);
            remainder = &remainder[character.len_utf8()..];
        }
    }
    Ok(result)
}

fn next_inline_start(source: &str) -> Option<usize> {
    [
        "![", "[", "`", "~", "*", "/", "_", "=", "{+", "{-", "{^", "{,",
    ]
    .iter()
    .filter_map(|marker| source.find(marker))
    .min()
}

fn inline_at_start(source: &str) -> Result<Option<(RichInline, &str)>, RichDocumentError> {
    if let Some((alt, path, after)) = image_at_start(source) {
        return Ok(Some((
            RichInline::Image {
                alt: alt.to_owned(),
                path: path.to_owned(),
            },
            after,
        )));
    }
    if let Some((text, destination, after)) = link_at_start(source) {
        return Ok(Some((
            RichInline::Link {
                text: text.to_owned(),
                destination: destination.to_owned(),
            },
            after,
        )));
    }
    if let Some((text, attributes, after)) = attribute_at_start(source) {
        return Ok(Some((
            RichInline::Attribute {
                text: text.to_owned(),
                attributes: attributes.to_owned(),
            },
            after,
        )));
    }
    if let Some((content, after)) = delimited_at_start(source, "`") {
        return Ok(Some((RichInline::Code(content.to_owned()), after)));
    }
    for (opening, closing, wrap) in [
        (
            "{-",
            "-}",
            RichInline::Deleted as fn(Vec<RichInline>) -> RichInline,
        ),
        ("{+", "+}", RichInline::Inserted),
        ("{^", "^}", RichInline::Superscript),
        ("{,", ",}", RichInline::Subscript),
        ("~", "~", RichInline::Strike),
        ("*", "*", RichInline::Bold),
        ("/", "/", RichInline::Italic),
        ("_", "_", RichInline::Underline),
        ("=", "=", RichInline::Highlight),
    ] {
        if let Some((content, after)) = delimited_at_start_with_closing(source, opening, closing) {
            return Ok(Some((wrap(inline(content)?), after)));
        }
    }
    Ok(None)
}

fn image_at_start(source: &str) -> Option<(&str, &str, &str)> {
    let rest = source.strip_prefix("![")?;
    let close_alt = rest.find("](")?;
    let close_path = rest[close_alt + 2..].find(')')?;
    let path_start = close_alt + 2;
    let path_end = path_start + close_path;
    Some((
        &rest[..close_alt],
        &rest[path_start..path_end],
        &rest[path_end + 1..],
    ))
}

fn link_at_start(source: &str) -> Option<(&str, &str, &str)> {
    let rest = source.strip_prefix('[')?;
    let close_text = rest.find("](")?;
    let close_destination = rest[close_text + 2..].find(')')?;
    let destination_start = close_text + 2;
    let destination_end = destination_start + close_destination;
    Some((
        &rest[..close_text],
        &rest[destination_start..destination_end],
        &rest[destination_end + 1..],
    ))
}

fn attribute_at_start(source: &str) -> Option<(&str, &str, &str)> {
    let rest = source.strip_prefix('[')?;
    let text_end = rest.find("]{")?;
    let attributes = &rest[text_end + 2..];
    let attributes_end = attributes.find('}')?;
    Some((
        &rest[..text_end],
        &attributes[..attributes_end],
        &attributes[attributes_end + 1..],
    ))
}

fn delimited_at_start<'a>(source: &'a str, delimiter: &str) -> Option<(&'a str, &'a str)> {
    delimited_at_start_with_closing(source, delimiter, delimiter)
}

fn delimited_at_start_with_closing<'a>(
    source: &'a str,
    opening: &str,
    closing: &str,
) -> Option<(&'a str, &'a str)> {
    let rest = source.strip_prefix(opening)?;
    let end = rest.find(closing)?;
    Some((&rest[..end], &rest[end + closing.len()..]))
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
        RichBlock::CodeBlock { language, content } => format!(
            "```{}\n{content}\n```",
            language.as_deref().unwrap_or_default()
        ),
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
            RichInline::Underline(nodes) => format!("_{}_", inline_source(nodes)),
            RichInline::Highlight(nodes) => format!("={}=", inline_source(nodes)),
            RichInline::Inserted(nodes) => format!("{{+{}+}}", inline_source(nodes)),
            RichInline::Superscript(nodes) => format!("{{^{}^}}", inline_source(nodes)),
            RichInline::Subscript(nodes) => format!("{{,{},}}", inline_source(nodes)),
            RichInline::Deleted(nodes) => format!("{{-{}-}}", inline_source(nodes)),
            RichInline::Code(text) => format!("`{text}`"),
            RichInline::Link { text, destination } => format!("[{text}]({destination})"),
            RichInline::Attribute { text, attributes } => format!("[{text}]{{{attributes}}}"),
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
    fn parses_headings_and_all_common_list_markers() {
        let document = parse_carve("# Title\n- bullet\n1. numbered\n- [x] done")
            .unwrap_or_else(|error| panic!("parse failed: {error}"));
        assert_eq!(document.blocks.len(), 4);
        assert_eq!(
            serialize_carve(&document),
            "# Title\n- bullet\n1. numbered\n- [x] done"
        );
    }

    #[test]
    fn parses_common_inline_formatting() {
        let document = parse_carve("`code` ~strike~ [link](https://example.test)")
            .unwrap_or_else(|error| panic!("parse failed: {error}"));
        assert_eq!(
            serialize_carve(&document),
            "`code` ~strike~ [link](https://example.test)"
        );
    }

    #[test]
    fn parses_the_documented_wysiwyg_subset_without_losing_source() {
        let source = "# Carve WYSIWYG Demo\n\nThis is a *WYSIWYG editor* that outputs /Carve markup/.\n\n## Inline marks\n\n- *Strong* → `*text*`\n- /Emphasis/ → `/text/`\n- _Underline_ → `_text_`\n- =Highlight= → `=text=`\n- ~Strike~ → `~text~`\n- {+Inserted+} → `{+text+}`\n- {-Deleted-} → `{-text-}`\n- [HTML]{abbr=\"HyperText Markup Language\"} → `[HTML]{abbr=\"...\"}`\n\n## Task list\n\n- [x] Task lists round-trip to `- [x]`\n- [ ] Toggle the checkbox and watch the source\n\n> Edit the content and watch the Carve source below.\n\n```php\necho \"Hello, Carve!\";\n```";
        let document = parse_carve(source).unwrap_or_else(|error| panic!("parse failed: {error}"));
        assert_eq!(serialize_carve(&document), source);
        assert!(matches!(
            document.blocks.last(),
            Some(RichBlock::CodeBlock { language: Some(language), .. }) if language == "php"
        ));
    }

    #[test]
    fn preserves_empty_and_trailing_paragraph_breaks() {
        let source = "First paragraph\n\nSecond paragraph\n";
        let document = parse_carve(source).unwrap_or_else(|error| panic!("parse failed: {error}"));
        assert_eq!(serialize_carve(&document), source);
    }

    #[test]
    fn protects_tables_from_lossy_rich_editing() {
        assert_eq!(
            parse_carve("| A | B |"),
            Err(RichDocumentError::Unsupported(UnsupportedFeature::Table))
        );
    }

    #[test]
    fn canonical_ast_routes_advanced_carve_to_the_full_renderer() {
        let source = "# Heading\n\n|= Name |= Value |\n| One | Two |\n\n::: note\nKeep this\n:::\n";
        assert_eq!(editor_projection(source), EditorProjection::RenderedOnly);
        let html = render_html(source);
        assert!(html.contains("<table"));
        assert!(html.contains("Keep this"));
    }

    #[test]
    fn canonical_ast_keeps_the_everyday_editor_subset_native() {
        let source = "## Plan\n\n- [x] *Bold* /italic/ _under_ =highlight= {^sup^} {,sub,}\n\n```rust\nlet ok = true;\n```";
        assert!(native_document(&parse(source)));
        assert!(parse_carve(source).is_ok());
        let projection = editor_projection(source);
        assert!(
            matches!(projection, EditorProjection::Native(_)),
            "{projection:?}"
        );
    }

    #[test]
    fn canonical_ast_keeps_standalone_managed_images_editable() {
        let source = "# Heading 2\n\n# h2 testttttttttt\n\nthis is a note blabla hello world\n\n![Pasted image](assets/11ce96fc0358a8c741e4086afec6f3ae0ffb5e43f35f4b22f270a437016bcf9b.png)\n\n*blabla*\n\n*glalala*";
        let projection = editor_projection(source);
        assert!(
            matches!(projection, EditorProjection::Native(_)),
            "{projection:?}"
        );
        assert_eq!(
            parse_carve(source).map(|document| serialize_carve(&document)),
            Ok(source.to_owned())
        );
    }
}
