//! Carve serialization for the native rich text buffer.

use gtk::prelude::*;

use crate::formatting;

pub(crate) fn buffer_text(buffer: &gtk::TextBuffer) -> glib::GString {
    let mut output = Vec::new();
    let mut line_number = 0;
    while line_number < buffer.line_count() {
        let Some(line) = buffer.iter_at_line(line_number) else {
            break;
        };
        if line.char() == '\n' {
            output.push(String::new());
            line_number += 1;
            continue;
        }
        let mut end = line;
        end.forward_to_line_end();
        if let Some(language) = formatting::code_block_language(&line) {
            let mut content = Vec::new();
            while let Some(code_line) = buffer.iter_at_line(line_number) {
                let mut code_end = code_line;
                code_end.forward_to_line_end();
                content.push(buffer.text(&code_line, &code_end, false).to_string());
                line_number += 1;
                if line_number >= buffer.line_count() {
                    break;
                }
                let Some(next_line) = buffer.iter_at_line(line_number) else {
                    break;
                };
                if formatting::code_block_language(&next_line).as_deref() != Some(&language) {
                    break;
                }
            }
            output.push(format!("```{language}\n{}\n```", content.join("\n")));
            continue;
        }
        output.push(serialize_line(&line, &end));
        line_number += 1;
    }
    if output.last().is_some_and(String::is_empty) {
        let mut trailing_break = buffer.end_iter();
        if trailing_break.backward_char() && has_tag(&trailing_break, "rich-image-break") {
            output.pop();
        }
    }
    output.join("\n").into()
}

fn serialize_line(start: &gtk::TextIter, end: &gtk::TextIter) -> String {
    if has_line_tag(start, "rich-rule") {
        return "---".to_owned();
    }
    let (prefix, marker_width) = if has_line_tag(start, "rich-heading-1") {
        ("# ", 0)
    } else if has_line_tag(start, "rich-heading-2") {
        ("## ", 0)
    } else if has_line_tag(start, "rich-heading-3") {
        ("### ", 0)
    } else if has_line_tag(start, "rich-quote") {
        ("> ", 0)
    } else if let Some((prefix, width)) = list_marker(start) {
        (prefix, width)
    } else {
        ("", 0)
    };
    let mut output = prefix.to_owned();
    let mut current = *start;
    let mut active = [false; 8];
    let delimiters = [
        ("rich-bold", "*"),
        ("rich-italic", "/"),
        ("rich-strike", "~"),
        ("rich-underline", "_"),
        ("rich-highlight", "="),
        ("rich-inserted", "{+"),
        ("rich-deleted", "{-"),
        ("rich-code", "`"),
    ];
    let closing_delimiters = ["*", "/", "~", "_", "=", "+}", "-}", "`"];
    while current.offset() < end.offset() {
        let mut next = current;
        next.forward_char();
        let tags = [
            has_tag(&current, "rich-bold"),
            has_tag(&current, "rich-italic"),
            has_tag(&current, "rich-strike"),
            has_tag(&current, "rich-underline"),
            has_tag(&current, "rich-highlight"),
            has_tag(&current, "rich-inserted"),
            has_tag(&current, "rich-deleted"),
            has_tag(&current, "rich-code"),
        ];
        for index in (0..active.len()).rev() {
            if active[index] && !tags[index] {
                output.push_str(closing_delimiters[index]);
                active[index] = false;
            }
        }
        for index in 0..active.len() {
            if !active[index] && tags[index] {
                output.push_str(delimiters[index].1);
                active[index] = true;
            }
        }
        let is_marker = current.offset() < start.offset() + marker_width;
        if !is_marker && !has_tag(&current, "rich-structural") && current.char() != '\u{fffc}' {
            if let Some((markup, metadata_end)) = serialize_metadata(&current, end) {
                output.push_str(&markup);
                current = metadata_end;
                continue;
            }
            output.push(current.char());
        }
        current = next;
    }
    for index in (0..active.len()).rev() {
        if active[index] {
            output.push_str(closing_delimiters[index]);
        }
    }
    output
}

fn serialize_metadata(
    start: &gtk::TextIter,
    end: &gtk::TextIter,
) -> Option<(String, gtk::TextIter)> {
    if let Some(destination) = formatting::link_destination(start) {
        let link_end = metadata_end(start, end, formatting::link_destination, &destination);
        return Some((
            format!("[{}]({destination})", iter_text(start, &link_end)),
            link_end,
        ));
    }
    let attributes = formatting::attribute_spec(start)?;
    let attribute_end = metadata_end(start, end, formatting::attribute_spec, &attributes);
    Some((
        format!("[{}]{{{attributes}}}", iter_text(start, &attribute_end)),
        attribute_end,
    ))
}

fn metadata_end(
    start: &gtk::TextIter,
    end: &gtk::TextIter,
    metadata: impl Fn(&gtk::TextIter) -> Option<String>,
    expected: &str,
) -> gtk::TextIter {
    let mut current = *start;
    while current.offset() < end.offset() && metadata(&current).as_deref() == Some(expected) {
        current.forward_char();
    }
    current
}

fn iter_text(start: &gtk::TextIter, end: &gtk::TextIter) -> String {
    let mut output = String::new();
    let mut current = *start;
    while current.offset() < end.offset() {
        output.push(current.char());
        current.forward_char();
    }
    output
}

fn list_marker(start: &gtk::TextIter) -> Option<(&'static str, i32)> {
    if has_line_tag(start, "rich-list-task-checked") || starts_with(start, "☑ ") {
        Some(("- [x] ", 2))
    } else if starts_with(start, "☐ ") {
        Some(("- [ ] ", 2))
    } else if has_line_tag(start, "rich-list-bullet") || starts_with(start, "• ") {
        Some(("- ", 2))
    } else if has_line_tag(start, "rich-list-ordered") || starts_with(start, "1. ") {
        Some(("1. ", 3))
    } else if has_line_tag(start, "rich-list-task") {
        Some(("- [ ] ", 2))
    } else {
        None
    }
}

fn starts_with(start: &gtk::TextIter, text: &str) -> bool {
    let mut current = *start;
    text.chars().all(|character| {
        let matches = current.char() == character;
        if matches {
            current.forward_char();
        }
        matches
    })
}

pub(super) fn has_tag(iter: &gtk::TextIter, name: &str) -> bool {
    iter.tags()
        .iter()
        .any(|tag| tag.name().as_deref() == Some(name))
}

fn has_line_tag(iter: &gtk::TextIter, name: &str) -> bool {
    let mut current = *iter;
    loop {
        if has_tag(&current, name) {
            return true;
        }
        if !current.forward_char() || current.line() != iter.line() {
            return false;
        }
    }
}
