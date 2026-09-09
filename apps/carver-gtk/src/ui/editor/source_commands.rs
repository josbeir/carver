//! Carve-source formatting commands shared by the source toolbar and shortcuts.

use std::ops::Range;

use carver_domain::source_analysis::{SourceContext, SourceNodeKind};
use gtk::prelude::*;

use crate::mvu::SourceImageTarget;

use super::{
    buffer_text,
    toolbar::{ToolbarCommand, ToolbarState},
};

pub(crate) fn toolbar_state_from_context(context: Option<SourceContext>) -> ToolbarState {
    let mut state = ToolbarState::default();
    let Some(context) = context else {
        return state;
    };
    for node in context.path() {
        match node {
            SourceNodeKind::Heading(level) => state.set_heading(*level),
            SourceNodeKind::UnorderedList => state.activate(ToolbarCommand::BulletList),
            SourceNodeKind::OrderedList => state.activate(ToolbarCommand::OrderedList),
            SourceNodeKind::ListItem { task: true } => state.activate(ToolbarCommand::TaskList),
            SourceNodeKind::CodeBlock => state.activate(ToolbarCommand::CodeBlock),
            SourceNodeKind::Table => state.set_table(true),
            SourceNodeKind::Image { width } => state.set_image_width(*width),
            SourceNodeKind::Link => state.activate(ToolbarCommand::Link),
            SourceNodeKind::Bold => state.activate(ToolbarCommand::Bold),
            SourceNodeKind::Italic => state.activate(ToolbarCommand::Italic),
            SourceNodeKind::BoldItalic => {
                state.activate(ToolbarCommand::Bold);
                state.activate(ToolbarCommand::Italic);
            }
            SourceNodeKind::Strike => state.activate(ToolbarCommand::Strike),
            SourceNodeKind::Underline => state.activate(ToolbarCommand::Underline),
            SourceNodeKind::Highlight => state.activate(ToolbarCommand::Highlight),
            SourceNodeKind::Superscript => state.activate(ToolbarCommand::Superscript),
            SourceNodeKind::Subscript => state.activate(ToolbarCommand::Subscript),
            SourceNodeKind::InlineCode => state.activate(ToolbarCommand::InlineCode),
            SourceNodeKind::Frontmatter
            | SourceNodeKind::DefinitionList
            | SourceNodeKind::DefinitionTerm
            | SourceNodeKind::DefinitionDescription
            | SourceNodeKind::Paragraph
            | SourceNodeKind::ListItem { task: false }
            | SourceNodeKind::BlockQuote
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

/// Returns the current source selection in Unicode code-point offsets.
pub(crate) fn selection_from_buffer(buffer: &gtk::TextBuffer) -> Range<usize> {
    buffer.selection_bounds().map_or_else(
        || {
            let cursor = usize::try_from(buffer.iter_at_mark(&buffer.get_insert()).offset())
                .unwrap_or_default();
            cursor..cursor
        },
        |(start, end)| {
            usize::try_from(start.offset()).unwrap_or_default()
                ..usize::try_from(end.offset()).unwrap_or_default()
        },
    )
}

/// Captures the source snapshot and selection that a native image import may replace.
pub(crate) fn image_target_from_buffer(buffer: &gtk::TextBuffer) -> SourceImageTarget {
    SourceImageTarget {
        source: buffer_text(buffer).to_string(),
        selection: selection_from_buffer(buffer),
    }
}

/// Synchronizes canonical source with the smallest possible buffer replacement.
///
/// Keeping the edit local lets `GtkTextView` retain its scroll anchor while an
/// asynchronous operation, such as managed-image storage, completes.
pub(crate) fn replace_source_buffer(buffer: &gtk::TextBuffer, source: &str) {
    replace_changed_buffer_range(buffer, source);
}

/// Replaces only the changed span so `GtkTextView` retains its scroll anchor.
fn replace_changed_buffer_range(buffer: &gtk::TextBuffer, replacement: &str) {
    let current = buffer_text(buffer);
    if current == replacement {
        return;
    }
    let current_length = current.chars().count();
    let replacement_length = replacement.chars().count();
    let common_prefix = current
        .chars()
        .zip(replacement.chars())
        .take_while(|(current, replacement)| current == replacement)
        .count();
    let remaining_current = current_length.saturating_sub(common_prefix);
    let remaining_replacement = replacement_length.saturating_sub(common_prefix);
    let common_suffix = current
        .chars()
        .rev()
        .zip(replacement.chars().rev())
        .take(remaining_current.min(remaining_replacement))
        .take_while(|(current, replacement)| current == replacement)
        .count();
    let replacement_span = replacement
        .chars()
        .skip(common_prefix)
        .take(remaining_replacement.saturating_sub(common_suffix))
        .collect::<String>();
    let mut start = buffer.iter_at_offset(i32::try_from(common_prefix).unwrap_or(i32::MAX));
    let mut end = buffer.iter_at_offset(
        i32::try_from(current_length.saturating_sub(common_suffix)).unwrap_or(i32::MAX),
    );

    buffer.begin_user_action();
    buffer.delete(&mut start, &mut end);
    buffer.insert(&mut start, &replacement_span);
    buffer.end_user_action();
}

#[cfg(test)]
pub(crate) mod tests;
