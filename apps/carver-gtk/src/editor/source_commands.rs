//! Carve-source formatting commands shared by the source toolbar and shortcuts.

use std::ops::Range;

use carver_domain::source_analysis::{SourceContext, SourceNodeKind};
use gtk::prelude::*;

use super::{
    buffer_text,
    toolbar::{ToolbarCommand, ToolbarState},
};

/// Canonical source plus a character-based selection for pure editing commands.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SourceEdit {
    source: String,
    selection: Range<usize>,
}

impl SourceEdit {
    /// Creates an edit snapshot, clamping the selection to valid character boundaries.
    #[must_use]
    pub(crate) fn new(source: impl Into<String>, selection: Range<usize>) -> Self {
        let source = source.into();
        let length = source.chars().count();
        let start = selection.start.min(length);
        let end = selection.end.clamp(start, length);
        Self {
            source,
            selection: start..end,
        }
    }

    /// Returns the canonical Carve source.
    #[must_use]
    pub(crate) fn source(&self) -> &str {
        &self.source
    }

    /// Returns the current character-based selection.
    #[must_use]
    pub(crate) fn selection(&self) -> Range<usize> {
        self.selection.clone()
    }

    /// Applies an inline delimiter command.
    pub(crate) fn toggle_inline(&mut self, opening: &str, closing: &str) {
        if self.selection.is_empty() {
            let cursor = self.selection.start;
            self.replace(cursor..cursor, &format!("{opening}{closing}"), 0);
            let cursor = cursor.saturating_add(opening.chars().count());
            self.selection = cursor..cursor;
            return;
        }
        let selected = self.selected_text();
        let replacement = inline_replacement(&selected, opening, closing);
        let length = replacement.chars().count();
        let range = self.selection.clone();
        self.replace(range, &replacement, length);
    }

    /// Sets a heading level for the selected lines.
    pub(crate) fn set_heading(&mut self, level: u8) {
        let prefix = if level == 0 {
            String::new()
        } else {
            format!("{} ", "#".repeat(usize::from(level.min(6))))
        };
        self.transform_selected_lines(|line| heading_replacement(line, &prefix));
    }

    /// Toggles a supported list marker across selected lines.
    pub(crate) fn toggle_list(&mut self, prefix: &str) {
        let lines = self.selected_lines();
        let remove = lines.iter().all(|line| line.starts_with(prefix));
        self.transform_selected_lines(|line| {
            list_replacement(strip_list_marker(line), prefix, remove)
        });
    }

    /// Toggles an ordered list, serializing selected items with consecutive numbers.
    pub(crate) fn toggle_ordered_list(&mut self) {
        let lines = self.selected_lines();
        let remove = lines.iter().all(|line| ordered_list_item(line).is_some());
        let replacement = lines
            .iter()
            .enumerate()
            .map(|(index, line)| {
                let text = strip_list_marker(line);
                if remove {
                    text.to_owned()
                } else {
                    format!("{}. {text}", index + 1)
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        let length = replacement.chars().count();
        let range = self.selected_line_range();
        self.replace(range, &replacement, length);
    }

    /// Toggles a fenced code block around the selected source.
    pub(crate) fn toggle_code_block(&mut self) {
        if self.selection.is_empty() {
            self.toggle_inline("`", "`");
            return;
        }
        let selected = self.selected_text();
        let replacement = if selected.starts_with("```") && selected.ends_with("\n```") {
            selected
                .strip_prefix("```")
                .and_then(|text| text.strip_prefix('\n'))
                .and_then(|text| text.strip_suffix("\n```"))
                .unwrap_or(&selected)
                .to_owned()
        } else {
            format!("```\n{selected}\n```")
        };
        let length = replacement.chars().count();
        let range = self.selection.clone();
        self.replace(range, &replacement, length);
    }

    /// Inserts a direct Carve link at the selection or cursor.
    pub(crate) fn insert_link(&mut self, text: &str, destination: &str) {
        let markup = format!("[{text}]({destination})");
        let length = markup.chars().count();
        let range = self.selection.clone();
        self.replace(range.clone(), &markup, 0);
        let cursor = range.start.saturating_add(length);
        self.selection = cursor..cursor;
    }

    /// Inserts a Carve table at the cursor.
    pub(crate) fn insert_table(&mut self, rows: u8, columns: u8, header: bool) {
        if rows == 0 || columns == 0 {
            return;
        }
        let mut table = String::new();
        for row in 0..rows {
            for _ in 0..columns {
                table.push('|');
                if header && row == 0 {
                    table.push('=');
                }
                table.push(' ');
            }
            table.push('|');
            if row + 1 != rows {
                table.push('\n');
            }
        }
        let markup = format!("\n{table}\n");
        let length = markup.chars().count();
        let cursor = self.selection.end;
        self.replace(cursor..cursor, &markup, 0);
        let cursor = cursor.saturating_add(length);
        self.selection = cursor..cursor;
    }

    /// Updates the width attribute of the direct image containing the cursor.
    pub(crate) fn set_image_width(&mut self, width: Option<u8>) -> bool {
        let cursor = character_to_byte(&self.source, self.selection.end);
        let Some((start, end)) = cursor.and_then(|cursor| image_span_at(&self.source, cursor))
        else {
            return false;
        };
        let replacement = image_with_width(&self.source[start..end], width);
        let Some(start) = character_offset_at_byte(&self.source, start) else {
            return false;
        };
        let Some(end) = character_offset_at_byte(&self.source, end) else {
            return false;
        };
        let length = replacement.chars().count();
        self.replace(start..end, &replacement, length);
        true
    }

    fn selected_text(&self) -> String {
        let start =
            character_to_byte(&self.source, self.selection.start).unwrap_or(self.source.len());
        let end = character_to_byte(&self.source, self.selection.end).unwrap_or(self.source.len());
        self.source[start..end].to_owned()
    }

    fn selected_lines(&self) -> Vec<String> {
        let range = self.selected_line_range();
        self.source[character_to_byte(&self.source, range.start).unwrap_or(self.source.len())
            ..character_to_byte(&self.source, range.end).unwrap_or(self.source.len())]
            .split('\n')
            .map(str::to_owned)
            .collect()
    }

    fn transform_selected_lines(&mut self, transform: impl Fn(&str) -> String) {
        let range = self.selected_line_range();
        let replacement = self
            .selected_lines()
            .iter()
            .map(|line| transform(line))
            .collect::<Vec<_>>()
            .join("\n");
        let length = replacement.chars().count();
        self.replace(range, &replacement, length);
    }

    fn selected_line_range(&self) -> Range<usize> {
        let start =
            character_to_byte(&self.source, self.selection.start).unwrap_or(self.source.len());
        let end = character_to_byte(&self.source, self.selection.end).unwrap_or(self.source.len());
        let start = self.source[..start]
            .rfind('\n')
            .map_or(0, |index| index + 1);
        let end = self.source[end..]
            .find('\n')
            .map_or(self.source.len(), |index| end + index);
        character_offset_at_byte(&self.source, start).unwrap_or_default()
            ..character_offset_at_byte(&self.source, end).unwrap_or_default()
    }

    fn replace(&mut self, range: Range<usize>, replacement: &str, selected_length: usize) {
        let start = character_to_byte(&self.source, range.start).unwrap_or(self.source.len());
        let end = character_to_byte(&self.source, range.end).unwrap_or(self.source.len());
        self.source.replace_range(start..end, replacement);
        let selection_start = range.start;
        self.selection = selection_start..selection_start.saturating_add(selected_length);
    }
}

/// Wraps the selection in a Carve inline delimiter, or inserts an empty pair.
pub(crate) fn toggle_inline(buffer: &gtk::TextBuffer, opening: &str, closing: &str) {
    apply_buffer_edit(buffer, |edit| edit.toggle_inline(opening, closing));
}

/// Sets a heading level for every selected line, replacing any existing heading.
/// A zero level returns the lines to ordinary paragraph text.
pub(crate) fn set_heading(buffer: &gtk::TextBuffer, level: u8) {
    apply_buffer_edit(buffer, |edit| edit.set_heading(level));
}

/// Switches every selected line between a Carve list marker and ordinary text.
/// Existing supported list markers are replaced instead of nesting prefixes.
pub(crate) fn toggle_list(buffer: &gtk::TextBuffer, prefix: &str) {
    apply_buffer_edit(buffer, |edit| edit.toggle_list(prefix));
}

/// Switches selected lines between a consecutively numbered Carve list and plain text.
pub(crate) fn toggle_ordered_list(buffer: &gtk::TextBuffer) {
    apply_buffer_edit(buffer, SourceEdit::toggle_ordered_list);
}

/// Wraps selected lines in a fenced Carve code block.
pub(crate) fn toggle_code_block(buffer: &gtk::TextBuffer) {
    apply_buffer_edit(buffer, SourceEdit::toggle_code_block);
}

/// Inserts a direct Carve link using already collected dialog values.
pub(crate) fn insert_link(buffer: &gtk::TextBuffer, text: &str, destination: &str) {
    apply_buffer_edit(buffer, |edit| edit.insert_link(text, destination));
}

/// Inserts a Carve table with the selected dimensions at the source cursor.
pub(crate) fn insert_table(buffer: &gtk::TextBuffer, rows: u8, columns: u8, header: bool) {
    apply_buffer_edit(buffer, |edit| edit.insert_table(rows, columns, header));
}

/// Updates the width attribute of the direct Carve image containing the cursor.
///
/// Returns `false` when the cursor does not point at a direct image form.
pub(crate) fn set_image_width(buffer: &gtk::TextBuffer, width: Option<u8>) -> bool {
    let mut edit = source_edit_from_buffer(buffer);
    let changed = edit.set_image_width(width);
    if changed {
        apply_source_edit(buffer, &edit);
    }
    changed
}

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

fn apply_buffer_edit(buffer: &gtk::TextBuffer, command: impl FnOnce(&mut SourceEdit)) {
    let mut edit = source_edit_from_buffer(buffer);
    command(&mut edit);
    apply_source_edit(buffer, &edit);
}

fn source_edit_from_buffer(buffer: &gtk::TextBuffer) -> SourceEdit {
    let source = buffer.text(&buffer.start_iter(), &buffer.end_iter(), false);
    let selection = buffer.selection_bounds().map_or_else(
        || {
            let cursor = usize::try_from(buffer.iter_at_mark(&buffer.get_insert()).offset())
                .unwrap_or_default();
            cursor..cursor
        },
        |(start, end)| {
            usize::try_from(start.offset()).unwrap_or_default()
                ..usize::try_from(end.offset()).unwrap_or_default()
        },
    );
    SourceEdit::new(source, selection)
}

/// Returns the current source selection in Unicode code-point offsets.
pub(crate) fn selection_from_buffer(buffer: &gtk::TextBuffer) -> Range<usize> {
    source_edit_from_buffer(buffer).selection()
}

fn apply_source_edit(buffer: &gtk::TextBuffer, edit: &SourceEdit) {
    replace_source_buffer(buffer, edit.source());
    let selection = edit.selection();
    let start = buffer.iter_at_offset(i32::try_from(selection.start).unwrap_or(i32::MAX));
    let end = buffer.iter_at_offset(i32::try_from(selection.end).unwrap_or(i32::MAX));
    if selection.is_empty() {
        buffer.place_cursor(&start);
    } else {
        buffer.select_range(&start, &end);
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

fn image_span_at(source: &str, cursor: usize) -> Option<(usize, usize)> {
    let start = source[..cursor].rfind("![")?;
    let close = source[start..].find(')')? + start + 1;
    let mut end = close;
    if source[end..].starts_with('{') {
        let attrs_end = source[end..].find('}')? + end + 1;
        end = attrs_end;
    }
    (cursor <= end).then_some((start, end))
}

fn image_with_width(image: &str, width: Option<u8>) -> String {
    let Some(close) = image.rfind(')') else {
        return image.to_owned();
    };
    let (image, attrs) = image.split_at(close + 1);
    let attrs = attrs
        .strip_prefix('{')
        .and_then(|attrs| attrs.strip_suffix('}'));
    let mut entries = image_attributes_without_width(attrs.unwrap_or_default());
    if let Some(width) = width {
        entries.push(format!("width=\"{width}%\""));
    }
    if entries.is_empty() {
        image.to_owned()
    } else {
        format!("{image}{{{}}}", entries.join(" "))
    }
}

fn image_attributes_without_width(attributes: &str) -> Vec<String> {
    let mut entries = Vec::new();
    let mut cursor = 0;
    while cursor < attributes.len() {
        cursor += attributes[cursor..]
            .char_indices()
            .take_while(|(_, character)| character.is_whitespace())
            .map(|(offset, character)| offset + character.len_utf8())
            .last()
            .unwrap_or_default();
        if cursor >= attributes.len() {
            break;
        }
        let start = cursor;
        let key_end = attributes[cursor..]
            .char_indices()
            .find_map(|(offset, character)| {
                (character == '=' || character.is_whitespace()).then_some(cursor + offset)
            })
            .unwrap_or(attributes.len());
        let key = &attributes[start..key_end];
        cursor = key_end;
        if attributes[cursor..].starts_with('=') {
            cursor += 1;
            if let Some(quote) = attributes[cursor..]
                .chars()
                .next()
                .filter(|quote| *quote == '\'' || *quote == '"')
            {
                cursor += quote.len_utf8();
                cursor = attributes[cursor..]
                    .find(quote)
                    .map_or(attributes.len(), |offset| {
                        cursor + offset + quote.len_utf8()
                    });
            } else {
                cursor = attributes[cursor..]
                    .char_indices()
                    .find_map(|(offset, character)| {
                        character.is_whitespace().then_some(cursor + offset)
                    })
                    .unwrap_or(attributes.len());
            }
        }
        if key != "width" {
            entries.push(attributes[start..cursor].to_owned());
        }
    }
    entries
}

fn character_to_byte(source: &str, character_offset: usize) -> Option<usize> {
    source
        .char_indices()
        .nth(character_offset)
        .map(|(byte, _)| byte)
        .or_else(|| (character_offset == source.chars().count()).then_some(source.len()))
}

fn character_offset_at_byte(source: &str, byte_offset: usize) -> Option<usize> {
    (byte_offset <= source.len()).then(|| source[..byte_offset].chars().count())
}

fn strip_list_marker(line: &str) -> &str {
    if let Some(text) = line
        .strip_prefix("- [ ] ")
        .or_else(|| line.strip_prefix("- [x] "))
    {
        return text;
    }
    if let Some(text) = line.strip_prefix("- ").or_else(|| line.strip_prefix("* ")) {
        return text;
    }
    ordered_list_item(line).unwrap_or(line)
}

fn ordered_list_item(line: &str) -> Option<&str> {
    let digits = line.bytes().take_while(u8::is_ascii_digit).count();
    (digits > 0)
        .then(|| line.get(digits..).and_then(|tail| tail.strip_prefix(". ")))
        .flatten()
}

fn inline_replacement(selected: &str, opening: &str, closing: &str) -> String {
    if selected.starts_with(opening) && selected.ends_with(closing) {
        selected[opening.len()..selected.len().saturating_sub(closing.len())].to_owned()
    } else {
        format!("{opening}{selected}{closing}")
    }
}

fn heading_replacement(line: &str, prefix: &str) -> String {
    let without_heading = line
        .strip_prefix('#')
        .and_then(|_| line.trim_start_matches('#').strip_prefix(' '))
        .unwrap_or(line);
    format!("{prefix}{without_heading}")
}

fn list_replacement(line: &str, prefix: &str, remove: bool) -> String {
    if remove {
        line.to_owned()
    } else {
        format!("{prefix}{line}")
    }
}

#[cfg(test)]
pub(crate) mod tests;
