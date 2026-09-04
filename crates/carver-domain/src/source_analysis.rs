//! Position-aware Carve source analysis for editor integrations.

use std::ops::Range;

use carve::{BlockNode, EmphasisKind, InlineNode, Options, Pos, parse_with_options};

/// A semantic AST node suitable for source-editor context and syntax styling.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SourceNodeKind {
    /// A paragraph block.
    Paragraph,
    /// A heading with its authored level.
    Heading(u8),
    /// An unordered list.
    UnorderedList,
    /// An ordered list.
    OrderedList,
    /// A list item, optionally a task item.
    ListItem {
        /// Whether this item carries a task-list checkbox.
        task: bool,
    },
    /// A block quote.
    BlockQuote,
    /// A fenced code block.
    CodeBlock,
    /// A table.
    Table,
    /// A table row.
    TableRow,
    /// A table header cell.
    TableHeader,
    /// A table data cell.
    TableCell,
    /// A direct image with an optional responsive width.
    Image {
        /// The authored responsive width percentage, when present.
        width: Option<u8>,
    },
    /// A link.
    Link,
    /// Bold text.
    Bold,
    /// Italic text.
    Italic,
    /// Bold italic text.
    BoldItalic,
    /// Underlined text.
    Underline,
    /// Struck-through text.
    Strike,
    /// Highlighted text.
    Highlight,
    /// Superscript text.
    Superscript,
    /// Subscript text.
    Subscript,
    /// Inline code.
    InlineCode,
    /// A raw or extension construct.
    Raw,
    /// A comment.
    Comment,
    /// A supported structural container not represented above.
    Container,
}

impl SourceNodeKind {
    /// Returns the compact markup-oriented label used by the source breadcrumb.
    #[must_use]
    pub fn label(self) -> String {
        match self {
            Self::Paragraph => "p".to_owned(),
            Self::Heading(level) => format!("h{level}"),
            Self::UnorderedList => "ul".to_owned(),
            Self::OrderedList => "ol".to_owned(),
            Self::ListItem { .. } => "li".to_owned(),
            Self::BlockQuote => "blockquote".to_owned(),
            Self::CodeBlock => "code-block".to_owned(),
            Self::Table => "table".to_owned(),
            Self::TableRow => "tr".to_owned(),
            Self::TableHeader => "th".to_owned(),
            Self::TableCell => "td".to_owned(),
            Self::Image { .. } => "image".to_owned(),
            Self::Link => "link".to_owned(),
            Self::Bold => "bold".to_owned(),
            Self::Italic => "italic".to_owned(),
            Self::BoldItalic => "bold-italic".to_owned(),
            Self::Underline => "underline".to_owned(),
            Self::Strike => "strike".to_owned(),
            Self::Highlight => "highlight".to_owned(),
            Self::Superscript => "sup".to_owned(),
            Self::Subscript => "sub".to_owned(),
            Self::InlineCode => "code".to_owned(),
            Self::Raw => "raw".to_owned(),
            Self::Comment => "comment".to_owned(),
            Self::Container => "container".to_owned(),
        }
    }
}

/// The shared AST ancestry at a source selection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceContext {
    path: Vec<SourceNodeKind>,
}

impl SourceContext {
    /// Returns the outermost-to-innermost semantic node path.
    #[must_use]
    pub fn path(&self) -> &[SourceNodeKind] {
        &self.path
    }

    /// Returns whether this context contains only an ordinary paragraph.
    #[must_use]
    pub fn is_plain_paragraph(&self) -> bool {
        self.path.as_slice() == [SourceNodeKind::Paragraph]
    }

    /// Renders the compact source breadcrumb.
    #[must_use]
    pub fn breadcrumb(&self) -> String {
        self.path
            .iter()
            .map(|node| node.label())
            .collect::<Vec<_>>()
            .join(" › ")
    }
}

/// A cached, position-aware Carve parse suitable for an editor buffer.
#[derive(Clone, Debug, Default)]
pub struct SourceAnalysis {
    nodes: Vec<AnalyzedNode>,
}

#[derive(Clone, Debug)]
struct AnalyzedNode {
    range: Range<usize>,
    path: Vec<SourceNodeKind>,
}

impl SourceAnalysis {
    /// Parses canonical Carve source once and records its positioned semantic nodes.
    #[must_use]
    pub fn parse(source: &str) -> Self {
        let document = parse_with_options(source, &Options::default().with_positions(true));
        let mut analysis = Self::default();
        let mut path = Vec::new();
        for block in &document.children {
            analysis.visit_block(block, &mut path);
        }
        analysis
    }

    /// Returns the deepest AST ancestry enclosing the complete selection.
    #[must_use]
    pub fn context_for(&self, selection: Range<usize>) -> Option<SourceContext> {
        self.nodes
            .iter()
            .filter(|node| contains_selection(&node.range, &selection))
            .max_by_key(|node| node.path.len())
            .map(|node| SourceContext {
                path: node.path.clone(),
            })
    }

    fn push(&mut self, pos: Option<&Pos>, kind: SourceNodeKind, path: &mut Vec<SourceNodeKind>) {
        path.push(kind);
        if let Some(range) = pos.and_then(pos_range) {
            self.nodes.push(AnalyzedNode {
                range: range.clone(),
                path: path.clone(),
            });
        }
    }

    fn pop(path: &mut Vec<SourceNodeKind>) {
        let _ = path.pop();
    }

    // CONTEXT: Keeping structural variants and their child traversal together makes the
    // outermost-to-innermost path lifecycle explicit.
    #[expect(clippy::too_many_lines)]
    fn visit_block(&mut self, block: &BlockNode, path: &mut Vec<SourceNodeKind>) {
        match block {
            BlockNode::Heading(node) => {
                self.push(node.pos.as_ref(), SourceNodeKind::Heading(node.level), path);
                self.visit_inlines(&node.children, path);
                Self::pop(path);
            }
            BlockNode::Paragraph(node) => {
                self.push(node.pos.as_ref(), SourceNodeKind::Paragraph, path);
                self.visit_inlines(&node.children, path);
                Self::pop(path);
            }
            BlockNode::CodeBlock(node) => {
                self.leaf(node.pos.as_ref(), SourceNodeKind::CodeBlock, path);
            }
            BlockNode::List(node) => {
                let kind = if node.ordered {
                    SourceNodeKind::OrderedList
                } else {
                    SourceNodeKind::UnorderedList
                };
                self.push(node.pos.as_ref(), kind, path);
                for item in &node.items {
                    self.push(
                        item.pos.as_ref(),
                        SourceNodeKind::ListItem {
                            task: item.checked.is_some(),
                        },
                        path,
                    );
                    for child in &item.children {
                        self.visit_block(child, path);
                    }
                    Self::pop(path);
                }
                Self::pop(path);
            }
            BlockNode::BlockQuote(node) => self.container(
                node.pos.as_ref(),
                SourceNodeKind::BlockQuote,
                &node.children,
                path,
            ),
            BlockNode::Table(node) => {
                self.push(node.pos.as_ref(), SourceNodeKind::Table, path);
                for row in &node.rows {
                    self.push(row.pos.as_ref(), SourceNodeKind::TableRow, path);
                    for cell in &row.cells {
                        self.push(
                            cell.pos.as_ref(),
                            if cell.header {
                                SourceNodeKind::TableHeader
                            } else {
                                SourceNodeKind::TableCell
                            },
                            path,
                        );
                        self.visit_inlines(&cell.children, path);
                        Self::pop(path);
                    }
                    Self::pop(path);
                }
                Self::pop(path);
            }
            BlockNode::Admonition(node) => self.container(
                node.pos.as_ref(),
                SourceNodeKind::Container,
                &node.children,
                path,
            ),
            BlockNode::Div(node) => self.container(
                node.pos.as_ref(),
                SourceNodeKind::Container,
                &node.children,
                path,
            ),
            BlockNode::LineBlock(node) => self.container(
                node.pos.as_ref(),
                SourceNodeKind::Container,
                &node.children,
                path,
            ),
            BlockNode::FigureGroup(node) => self.container(
                node.pos.as_ref(),
                SourceNodeKind::Container,
                &node.children,
                path,
            ),
            BlockNode::BlockImage(node) => self.leaf(
                node.pos.as_ref(),
                SourceNodeKind::Image {
                    width: image_width(node.attrs.as_ref()),
                },
                path,
            ),
            BlockNode::RawBlock(node) => self.leaf(node.pos.as_ref(), SourceNodeKind::Raw, path),
            BlockNode::Comment(node) => self.leaf(node.pos.as_ref(), SourceNodeKind::Comment, path),
            BlockNode::Extension(node) => {
                self.container(node.pos.as_ref(), SourceNodeKind::Raw, &node.children, path);
            }
            BlockNode::ThematicBreak(node) => {
                self.leaf(node.pos.as_ref(), SourceNodeKind::Container, path);
            }
            _ => {}
        }
    }

    fn visit_inlines(&mut self, nodes: &[InlineNode], path: &mut Vec<SourceNodeKind>) {
        for node in nodes {
            match node {
                InlineNode::Emphasis(node) => {
                    let kind = match node.kind {
                        EmphasisKind::Strong => SourceNodeKind::Bold,
                        EmphasisKind::Italic => SourceNodeKind::Italic,
                        EmphasisKind::Underline => SourceNodeKind::Underline,
                        EmphasisKind::Strike => SourceNodeKind::Strike,
                        EmphasisKind::Super => SourceNodeKind::Superscript,
                        EmphasisKind::Sub => SourceNodeKind::Subscript,
                        EmphasisKind::Highlight => SourceNodeKind::Highlight,
                        EmphasisKind::BoldItalic => SourceNodeKind::BoldItalic,
                    };
                    self.push(node.pos.as_ref(), kind, path);
                    self.visit_inlines(&node.children, path);
                    Self::pop(path);
                }
                InlineNode::Link(node) => {
                    self.push(node.pos.as_ref(), SourceNodeKind::Link, path);
                    self.visit_inlines(&node.children, path);
                    Self::pop(path);
                }
                InlineNode::Span(node) => {
                    self.push(node.pos.as_ref(), SourceNodeKind::Container, path);
                    self.visit_inlines(&node.children, path);
                    Self::pop(path);
                }
                InlineNode::Extension(node) => {
                    self.push(node.pos.as_ref(), SourceNodeKind::Raw, path);
                    self.visit_inlines(&node.children, path);
                    Self::pop(path);
                }
                InlineNode::CriticInsert(node) => {
                    self.push(node.pos.as_ref(), SourceNodeKind::Container, path);
                    self.visit_inlines(&node.children, path);
                    Self::pop(path);
                }
                InlineNode::CriticDelete(node) => {
                    self.push(node.pos.as_ref(), SourceNodeKind::Container, path);
                    self.visit_inlines(&node.children, path);
                    Self::pop(path);
                }
                InlineNode::Image(node) => self.leaf(
                    node.pos.as_ref(),
                    SourceNodeKind::Image {
                        width: image_width(node.attrs.as_ref()),
                    },
                    path,
                ),
                InlineNode::Code(node) => {
                    self.leaf(node.pos.as_ref(), SourceNodeKind::InlineCode, path);
                }
                InlineNode::RawInline(node) => {
                    self.leaf(node.pos.as_ref(), SourceNodeKind::Raw, path);
                }
                InlineNode::LiteralInline(node) => {
                    self.leaf(node.pos.as_ref(), SourceNodeKind::Raw, path);
                }
                InlineNode::Comment(node) => {
                    self.leaf(node.pos.as_ref(), SourceNodeKind::Comment, path);
                }
                InlineNode::CriticComment(node) => {
                    self.leaf(node.pos.as_ref(), SourceNodeKind::Comment, path);
                }
                InlineNode::AutoLink(node) => {
                    self.leaf(node.pos.as_ref(), SourceNodeKind::Link, path);
                }
                InlineNode::CrossRef(node) => {
                    self.leaf(node.pos.as_ref(), SourceNodeKind::Link, path);
                }
                _ => {}
            }
        }
    }

    fn leaf(&mut self, pos: Option<&Pos>, kind: SourceNodeKind, path: &mut Vec<SourceNodeKind>) {
        self.push(pos, kind, path);
        Self::pop(path);
    }

    fn container(
        &mut self,
        pos: Option<&Pos>,
        kind: SourceNodeKind,
        children: &[BlockNode],
        path: &mut Vec<SourceNodeKind>,
    ) {
        self.push(pos, kind, path);
        for child in children {
            self.visit_block(child, path);
        }
        Self::pop(path);
    }
}

fn pos_range(pos: &Pos) -> Option<Range<usize>> {
    (pos.start_offset < pos.end_offset).then_some(pos.start_offset..pos.end_offset)
}

fn contains_selection(node: &Range<usize>, selection: &Range<usize>) -> bool {
    if selection.is_empty() {
        node.start <= selection.start && selection.start < node.end
    } else {
        node.start <= selection.start && selection.end <= node.end
    }
}

fn image_width(attrs: Option<&carve::Attrs>) -> Option<u8> {
    attrs?
        .key_values
        .get("width")?
        .strip_suffix('%')?
        .parse()
        .ok()
}

#[cfg(test)]
mod tests;
