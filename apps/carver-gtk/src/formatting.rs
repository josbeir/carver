//! Carve formatting commands used by the rich editor toolbar.

use gtk::prelude::*;

/// Appends the common Carve formatting controls to an editor toolbar.
pub(crate) fn append_controls(toolbar: &gtk::Box, buffer: &gtk::TextBuffer) {
    append_wrap_button(toolbar, buffer, "format-bold-button", "B", "Bold", "*", "*");
    append_wrap_button(
        toolbar,
        buffer,
        "format-italic-button",
        "I",
        "Italic",
        "/",
        "/",
    );
    append_wrap_button(
        toolbar,
        buffer,
        "format-strike-button",
        "S",
        "Strikethrough",
        "~~",
        "~~",
    );
    append_line_prefix_button(
        toolbar,
        buffer,
        "format-heading-button",
        "H",
        "Heading",
        "# ",
    );
    append_line_prefix_button(
        toolbar,
        buffer,
        "format-bullet-button",
        "•",
        "Bulleted List",
        "- ",
    );
    append_line_prefix_button(
        toolbar,
        buffer,
        "format-task-button",
        "☑",
        "Task List",
        "- [ ] ",
    );
    append_wrap_button(
        toolbar,
        buffer,
        "format-code-button",
        "</>",
        "Inline Code",
        "`",
        "`",
    );
    append_wrap_button(
        toolbar,
        buffer,
        "format-link-button",
        "↗",
        "Link",
        "[",
        "](https://)",
    );
}

fn append_wrap_button(
    toolbar: &gtk::Box,
    buffer: &gtk::TextBuffer,
    name: &str,
    label: &str,
    tooltip: &str,
    prefix: &str,
    suffix: &str,
) {
    let button = gtk::Button::with_label(label);
    button.set_widget_name(name);
    button.set_tooltip_text(Some(tooltip));
    button.add_css_class("flat");
    let buffer = buffer.clone();
    let prefix = prefix.to_owned();
    let suffix = suffix.to_owned();
    button.connect_clicked(move |_| wrap_selection(&buffer, &prefix, &suffix));
    toolbar.append(&button);
}

fn append_line_prefix_button(
    toolbar: &gtk::Box,
    buffer: &gtk::TextBuffer,
    name: &str,
    label: &str,
    tooltip: &str,
    prefix: &str,
) {
    let button = gtk::Button::with_label(label);
    button.set_widget_name(name);
    button.set_tooltip_text(Some(tooltip));
    button.add_css_class("flat");
    let buffer = buffer.clone();
    let prefix = prefix.to_owned();
    button.connect_clicked(move |_| prefix_current_line(&buffer, &prefix));
    toolbar.append(&button);
}

fn wrap_selection(buffer: &gtk::TextBuffer, prefix: &str, suffix: &str) {
    if let Some((mut start, mut end)) = buffer.selection_bounds() {
        let selection = buffer.text(&start, &end, false);
        buffer.delete(&mut start, &mut end);
        buffer.insert(&mut start, prefix);
        buffer.insert(&mut start, &selection);
        buffer.insert(&mut start, suffix);
    } else {
        buffer.insert_at_cursor(&format!("{prefix}text{suffix}"));
    }
}

fn prefix_current_line(buffer: &gtk::TextBuffer, prefix: &str) {
    let insert_mark = buffer.get_insert();
    let mut cursor = buffer.iter_at_mark(&insert_mark);
    cursor.set_line_offset(0);
    buffer.insert(&mut cursor, prefix);
}
