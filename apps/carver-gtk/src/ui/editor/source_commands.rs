//! Carve-source formatting commands shared by the source toolbar and shortcuts.

use std::ops::Range;

use carver_domain::source_analysis::SourceContext;
use gtk::prelude::*;

use crate::mvu::SourceImageTarget;

use super::{buffer_text, toolbar::ToolbarState};

pub(crate) fn toolbar_state_from_context(context: Option<SourceContext>) -> ToolbarState {
    ToolbarState::from_rich(&crate::mvu::format_command::selection_from_context(context))
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
    replace_changed_buffer_range(buffer, source, None);
}

/// Includes the intended selection in the changed span so native redo restores its cursor.
pub(crate) fn replace_source_buffer_with_selection(
    buffer: &gtk::TextBuffer,
    source: &str,
    selection: Range<usize>,
) {
    replace_changed_buffer_range(buffer, source, Some(selection));
}

/// Replaces only the changed span so `GtkTextView` retains its scroll anchor.
fn replace_changed_buffer_range(
    buffer: &gtk::TextBuffer,
    replacement: &str,
    selection: Option<Range<usize>>,
) {
    let current = buffer_text(buffer);
    if current == replacement {
        return;
    }
    let current_length = current.chars().count();
    let replacement_length = replacement.chars().count();
    let mut common_prefix = current
        .chars()
        .zip(replacement.chars())
        .take_while(|(current, replacement)| current == replacement)
        .count();
    if let Some(selection) = selection.as_ref() {
        common_prefix = common_prefix.min(selection.start);
    }
    let remaining_current = current_length.saturating_sub(common_prefix);
    let remaining_replacement = replacement_length.saturating_sub(common_prefix);
    let mut common_suffix = current
        .chars()
        .rev()
        .zip(replacement.chars().rev())
        .take(remaining_current.min(remaining_replacement))
        .take_while(|(current, replacement)| current == replacement)
        .count();
    if let Some(selection) = selection.as_ref() {
        common_suffix = common_suffix.min(replacement_length.saturating_sub(selection.end));
    }
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
    if let Some(selection) = selection {
        let start = buffer.iter_at_offset(i32::try_from(selection.start).unwrap_or(i32::MAX));
        let end = buffer.iter_at_offset(i32::try_from(selection.end).unwrap_or(i32::MAX));
        buffer.select_range(&end, &start);
    }
    buffer.end_user_action();
}

#[cfg(test)]
pub(crate) mod tests;
