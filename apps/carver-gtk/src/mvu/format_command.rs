//! Mode-neutral formatting commands shared by native editor controls.

/// A formatting operation understood by both editor projections.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum FormatCommand {
    /// Bold formatting.
    Bold,
    /// Italic formatting.
    Italic,
    /// Strike formatting.
    Strike,
    /// Underline formatting.
    Underline,
    /// Highlight formatting.
    Highlight,
    /// Superscript formatting.
    Superscript,
    /// Subscript formatting.
    Subscript,
    /// Inline code formatting.
    InlineCode,
    /// Code block formatting.
    CodeBlock,
    /// Block quote formatting.
    BlockQuote,
    /// Bullet list formatting.
    BulletList,
    /// Ordered list formatting.
    OrderedList,
    /// Task list formatting.
    TaskList,
    /// Link formatting.
    Link,
}

impl FormatCommand {
    pub(crate) fn rich_name(self) -> &'static str {
        match self {
            Self::Bold => "bold",
            Self::Italic => "italic",
            Self::Strike => "strike",
            Self::Underline => "underline",
            Self::Highlight => "highlight",
            Self::Superscript => "superscript",
            Self::Subscript => "subscript",
            Self::InlineCode => "inline-code",
            Self::CodeBlock => "code-block",
            Self::BlockQuote => "blockquote",
            Self::BulletList => "bullet-list",
            Self::OrderedList => "ordered-list",
            Self::TaskList => "task-list",
            Self::Link => "link",
        }
    }
}

use super::SourceCommand;

pub(crate) fn source_command(command: FormatCommand) -> Option<SourceCommand> {
    Some(match command {
        FormatCommand::Bold => SourceCommand::ToggleInline {
            opening: String::from("*"),
            closing: String::from("*"),
        },
        FormatCommand::Italic => SourceCommand::ToggleInline {
            opening: String::from("/"),
            closing: String::from("/"),
        },
        FormatCommand::Strike => SourceCommand::ToggleInline {
            opening: String::from("~"),
            closing: String::from("~"),
        },
        FormatCommand::Underline => SourceCommand::ToggleInline {
            opening: String::from("_"),
            closing: String::from("_"),
        },
        FormatCommand::Highlight => SourceCommand::ToggleInline {
            opening: String::from("="),
            closing: String::from("="),
        },
        FormatCommand::Superscript => SourceCommand::ToggleInline {
            opening: String::from("{^"),
            closing: String::from("^}"),
        },
        FormatCommand::Subscript => SourceCommand::ToggleInline {
            opening: String::from("{,"),
            closing: String::from(",}"),
        },
        FormatCommand::InlineCode => SourceCommand::ToggleInline {
            opening: String::from("`"),
            closing: String::from("`"),
        },
        FormatCommand::CodeBlock => SourceCommand::ToggleCodeBlock,
        FormatCommand::BlockQuote => SourceCommand::ToggleBlockQuote,
        FormatCommand::BulletList => SourceCommand::ToggleList(String::from("- ")),
        FormatCommand::OrderedList => SourceCommand::ToggleOrderedList,
        FormatCommand::TaskList => SourceCommand::ToggleList(String::from("- [ ] ")),
        FormatCommand::Link => return None,
    })
}

use carver_domain::source_analysis::{SourceContext, SourceNodeKind};

pub(crate) fn selection_from_context(
    context: Option<SourceContext>,
) -> carver_editor_protocol::SelectionState {
    let mut state = carver_editor_protocol::SelectionState::default();
    let Some(context) = context else {
        return state;
    };
    for node in context.path() {
        match node {
            SourceNodeKind::Heading(level) => state.heading = *level,
            SourceNodeKind::UnorderedList => state
                .active
                .push(FormatCommand::BulletList.rich_name().into()),
            SourceNodeKind::OrderedList => state
                .active
                .push(FormatCommand::OrderedList.rich_name().into()),
            SourceNodeKind::ListItem { task: true } => state
                .active
                .push(FormatCommand::TaskList.rich_name().into()),
            SourceNodeKind::CodeBlock => state
                .active
                .push(FormatCommand::CodeBlock.rich_name().into()),
            SourceNodeKind::BlockQuote => state.active.push("blockquote".into()),
            SourceNodeKind::Table => state.active.push("table".into()),
            SourceNodeKind::Image { width } => {
                state.active.push("image".into());
                state.image_width = *width;
            }
            SourceNodeKind::Link => state.active.push(FormatCommand::Link.rich_name().into()),
            SourceNodeKind::Bold => state.active.push(FormatCommand::Bold.rich_name().into()),
            SourceNodeKind::Italic => state.active.push(FormatCommand::Italic.rich_name().into()),
            SourceNodeKind::BoldItalic => {
                state.active.push(FormatCommand::Bold.rich_name().into());
                state.active.push(FormatCommand::Italic.rich_name().into());
            }
            SourceNodeKind::Strike => state.active.push(FormatCommand::Strike.rich_name().into()),
            SourceNodeKind::Underline => state
                .active
                .push(FormatCommand::Underline.rich_name().into()),
            SourceNodeKind::Highlight => state
                .active
                .push(FormatCommand::Highlight.rich_name().into()),
            SourceNodeKind::Superscript => state
                .active
                .push(FormatCommand::Superscript.rich_name().into()),
            SourceNodeKind::Subscript => state
                .active
                .push(FormatCommand::Subscript.rich_name().into()),
            SourceNodeKind::InlineCode => state
                .active
                .push(FormatCommand::InlineCode.rich_name().into()),
            SourceNodeKind::Frontmatter
            | SourceNodeKind::DefinitionList
            | SourceNodeKind::DefinitionTerm
            | SourceNodeKind::DefinitionDescription
            | SourceNodeKind::Paragraph
            | SourceNodeKind::ListItem { task: false }
            | SourceNodeKind::TableRow
            | SourceNodeKind::TableHeader
            | SourceNodeKind::TableCell
            | SourceNodeKind::Raw
            | SourceNodeKind::Comment
            | SourceNodeKind::Container => {}
        }
    }
    state
}
