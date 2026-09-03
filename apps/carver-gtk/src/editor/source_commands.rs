//! Carve-source formatting commands shared by the source toolbar and shortcuts.

use gtk::prelude::*;

/// Wraps the selection in a Carve inline delimiter, or inserts an empty pair.
pub(crate) fn toggle_inline(buffer: &gtk::TextBuffer, opening: &str, closing: &str) {
    let Some((start, end)) = buffer.selection_bounds() else {
        let mut cursor = buffer.iter_at_mark(&buffer.get_insert());
        buffer.insert(&mut cursor, opening);
        buffer.insert(&mut cursor, closing);
        let mut inside = cursor;
        inside.backward_chars(i32::try_from(closing.chars().count()).unwrap_or_default());
        buffer.place_cursor(&inside);
        return;
    };
    let selected = buffer.text(&start, &end, false).to_string();
    let replacement = inline_replacement(&selected, opening, closing);
    let mut start = start;
    let mut end = end;
    buffer.delete(&mut start, &mut end);
    buffer.insert(&mut start, &replacement);
    let selection_end = start;
    let selection_start = buffer.iter_at_offset(
        selection_end.offset() - i32::try_from(replacement.chars().count()).unwrap_or_default(),
    );
    buffer.select_range(&selection_start, &selection_end);
}

/// Sets a heading level for every selected line, replacing any existing heading.
/// A zero level returns the lines to ordinary paragraph text.
pub(crate) fn set_heading(buffer: &gtk::TextBuffer, level: u8) {
    let prefix = if level == 0 {
        String::new()
    } else {
        format!("{} ", "#".repeat(usize::from(level.min(6))))
    };
    replace_selected_lines(buffer, |line| heading_replacement(line, &prefix));
}

/// Switches every selected line between a Carve list marker and ordinary text.
/// Existing supported list markers are replaced instead of nesting prefixes.
pub(crate) fn toggle_list(buffer: &gtk::TextBuffer, prefix: &str) {
    let (first, last) = selected_line_range(buffer);
    let lines = selected_lines(buffer, first, last);
    let remove = lines.iter().all(|line| line.starts_with(prefix));
    replace_selected_lines(buffer, |line| {
        let bare = strip_list_marker(line);
        list_replacement(bare, prefix, remove)
    });
}

/// Wraps selected lines in a fenced Carve code block.
pub(crate) fn toggle_code_block(buffer: &gtk::TextBuffer) {
    let Some((start, end)) = buffer.selection_bounds() else {
        toggle_inline(buffer, "`", "`");
        return;
    };
    let selected = buffer.text(&start, &end, false).to_string();
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
    replace_selection(buffer, &start, &end, &replacement);
}

/// Inserts a direct Carve link using already collected dialog values.
pub(crate) fn insert_link(buffer: &gtk::TextBuffer, text: &str, destination: &str) {
    let markup = format!("[{text}]({destination})");
    if let Some((start, end)) = buffer.selection_bounds() {
        replace_selection(buffer, &start, &end, &markup);
    } else {
        buffer.insert_at_cursor(&markup);
    }
}

fn replace_selection(
    buffer: &gtk::TextBuffer,
    start: &gtk::TextIter,
    end: &gtk::TextIter,
    replacement: &str,
) {
    let mut start = *start;
    let mut end = *end;
    buffer.delete(&mut start, &mut end);
    buffer.insert(&mut start, replacement);
    buffer.place_cursor(&start);
}

fn replace_selected_lines(buffer: &gtk::TextBuffer, transform: impl Fn(&str) -> String) {
    let (first, last) = selected_line_range(buffer);
    let replacement = selected_lines(buffer, first, last)
        .iter()
        .map(|line| transform(line))
        .collect::<Vec<_>>()
        .join("\n");
    let Some(mut start) = buffer.iter_at_line(first) else {
        return;
    };
    let mut end = buffer.iter_at_line(last).unwrap_or(start);
    end.forward_to_line_end();
    buffer.delete(&mut start, &mut end);
    buffer.insert(&mut start, &replacement);
}

fn selected_lines(buffer: &gtk::TextBuffer, first: i32, last: i32) -> Vec<String> {
    (first..=last)
        .filter_map(|line| buffer.iter_at_line(line))
        .map(|start| {
            let mut end = start;
            end.forward_to_line_end();
            buffer.text(&start, &end, false).to_string()
        })
        .collect()
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
    let digits = line.bytes().take_while(u8::is_ascii_digit).count();
    line.get(digits..)
        .and_then(|tail| tail.strip_prefix(". "))
        .unwrap_or(line)
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

fn selected_line_range(buffer: &gtk::TextBuffer) -> (i32, i32) {
    let Some((mut start, mut end)) = buffer.selection_bounds() else {
        let cursor = buffer.iter_at_mark(&buffer.get_insert());
        return (cursor.line(), cursor.line());
    };
    start.set_line_offset(0);
    if end.starts_line() && end.line() > start.line() {
        end.backward_char();
    }
    (start.line(), end.line())
}

#[cfg(test)]
pub(crate) mod tests;
