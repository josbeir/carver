//! Native rich-text formatting commands used by the editor toolbar.

use gtk::prelude::*;

const BLOCK_TAGS: [&str; 6] = [
    "rich-heading-1",
    "rich-heading-2",
    "rich-heading-3",
    "rich-list-bullet",
    "rich-list-ordered",
    "rich-list-task",
];

/// Appends the common Carve formatting controls to an editor toolbar.
pub(crate) fn append_controls(toolbar: &gtk::Box, buffer: &gtk::TextBuffer) {
    install_tags(buffer);
    append_tag_button(toolbar, buffer, "format-bold-button", "format-text-bold-symbolic", "Bold", "rich-bold");
    append_tag_button(toolbar, buffer, "format-italic-button", "format-text-italic-symbolic", "Italic", "rich-italic");
    append_tag_button(toolbar, buffer, "format-strike-button", "format-text-strikethrough-symbolic", "Strikethrough", "rich-strike");
    append_heading_menu(toolbar, buffer);
    append_block_button(toolbar, buffer, "format-bullet-button", "view-list-bullet-symbolic", "Bulleted list", "rich-list-bullet", "• ");
    append_block_button(toolbar, buffer, "format-ordered-button", "view-list-ordered-symbolic", "Numbered list", "rich-list-ordered", "1. ");
    append_block_button(toolbar, buffer, "format-task-button", "object-select-symbolic", "Task list", "rich-list-task", "☐ ");
    append_tag_button(toolbar, buffer, "format-code-button", "text-x-generic-symbolic", "Inline code", "rich-code");
    append_tag_button(toolbar, buffer, "format-link-button", "insert-link-symbolic", "Link", "rich-link");
}

/// Installs visual and structural tags used by the native rich projection.
pub(crate) fn install_tags(buffer: &gtk::TextBuffer) {
    ensure_tag(buffer, "rich-bold", |tag| tag.set_weight(700));
    ensure_tag(buffer, "rich-italic", |tag| tag.set_style(gtk::pango::Style::Italic));
    ensure_tag(buffer, "rich-strike", |tag| tag.set_strikethrough(true));
    ensure_tag(buffer, "rich-code", |tag| tag.set_family(Some("monospace")));
    ensure_tag(buffer, "rich-link", |tag| {
        tag.set_foreground(Some("@accent_color"));
        tag.set_underline(gtk::pango::Underline::Single);
    });
    ensure_tag(buffer, "rich-heading-1", |tag| {
        tag.set_weight(700);
        tag.set_scale(1.6);
        tag.set_pixels_above_lines(12);
        tag.set_pixels_below_lines(6);
    });
    ensure_tag(buffer, "rich-heading-2", |tag| {
        tag.set_weight(700);
        tag.set_scale(1.35);
        tag.set_pixels_above_lines(10);
        tag.set_pixels_below_lines(5);
    });
    ensure_tag(buffer, "rich-heading-3", |tag| {
        tag.set_weight(700);
        tag.set_scale(1.15);
        tag.set_pixels_above_lines(8);
        tag.set_pixels_below_lines(4);
    });
    for name in ["rich-list-bullet", "rich-list-ordered", "rich-list-task"] {
        ensure_tag(buffer, name, |tag| {
            tag.set_left_margin(28);
            tag.set_indent(-20);
        });
    }
    ensure_tag(buffer, "rich-structural", |tag| tag.set_editable(false));
    ensure_tag(buffer, "rich-hidden-source", |tag| tag.set_invisible(true));
}

fn ensure_tag(buffer: &gtk::TextBuffer, name: &str, configure: impl FnOnce(&gtk::TextTag)) {
    if buffer.tag_table().lookup(name).is_none()
        && let Some(tag) = buffer.create_tag(Some(name), &[])
    {
        configure(&tag);
    }
}

fn append_tag_button(toolbar: &gtk::Box, buffer: &gtk::TextBuffer, name: &str, icon: &str, tooltip: &str, tag_name: &str) {
    let button = icon_button(name, icon, tooltip);
    let buffer = buffer.clone();
    let tag_name = tag_name.to_owned();
    button.connect_clicked(move |_| toggle_tag_on_selection(&buffer, &tag_name));
    toolbar.append(&button);
}

fn append_block_button(toolbar: &gtk::Box, buffer: &gtk::TextBuffer, name: &str, icon: &str, tooltip: &str, tag_name: &str, marker: &str) {
    let button = icon_button(name, icon, tooltip);
    let buffer = buffer.clone();
    let tag_name = tag_name.to_owned();
    let marker = marker.to_owned();
    button.connect_clicked(move |_| toggle_current_block(&buffer, &tag_name, &marker));
    toolbar.append(&button);
}

fn append_heading_menu(toolbar: &gtk::Box, buffer: &gtk::TextBuffer) {
    let menu = gtk::MenuButton::new();
    menu.set_widget_name("format-heading-button");
    menu.set_icon_name("format-text-rich-symbolic");
    menu.set_tooltip_text(Some("Text style"));
    menu.add_css_class("flat");
    let popover = gtk::Popover::new();
    let choices = gtk::Box::new(gtk::Orientation::Vertical, 0);
    for (label, tag) in [("Normal text", None), ("Heading 1", Some("rich-heading-1")), ("Heading 2", Some("rich-heading-2")), ("Heading 3", Some("rich-heading-3"))] {
        let choice = gtk::Button::with_label(label);
        choice.add_css_class("flat");
        let buffer = buffer.clone();
        let tag = tag.map(str::to_owned);
        choice.connect_clicked(move |_| set_current_heading(&buffer, tag.as_deref()));
        choices.append(&choice);
    }
    popover.set_child(Some(&choices));
    menu.set_popover(Some(&popover));
    toolbar.append(&menu);
}

fn icon_button(name: &str, icon: &str, tooltip: &str) -> gtk::Button {
    let button = gtk::Button::from_icon_name(icon);
    button.set_widget_name(name);
    button.set_tooltip_text(Some(tooltip));
    button.add_css_class("flat");
    button
}

fn toggle_tag_on_selection(buffer: &gtk::TextBuffer, tag_name: &str) {
    let Some((start, end)) = buffer.selection_bounds() else { return; };
    if start.tags().iter().any(|tag| tag.name().as_deref() == Some(tag_name)) {
        buffer.remove_tag_by_name(tag_name, &start, &end);
    } else {
        buffer.apply_tag_by_name(tag_name, &start, &end);
    }
}

fn toggle_current_block(buffer: &gtk::TextBuffer, tag_name: &str, marker: &str) {
    let (mut start, mut end) = current_line_bounds(buffer);
    let active = start.tags().iter().any(|tag| tag.name().as_deref() == Some(tag_name));
    remove_block_tags(buffer, &start, &end);
    remove_structural_prefix(buffer, &mut start, &mut end);
    if !active {
        buffer.insert(&mut start, marker);
        let marker_end = start;
        let mut marker_start = marker_end;
        marker_start.backward_chars(marker.chars().count() as i32);
        buffer.apply_tag_by_name("rich-structural", &marker_start, &marker_end);
        end = marker_end;
        end.forward_to_line_end();
        buffer.apply_tag_by_name(tag_name, &marker_start, &end);
    }
}

fn set_current_heading(buffer: &gtk::TextBuffer, tag_name: Option<&str>) {
    let (mut start, mut end) = current_line_bounds(buffer);
    remove_block_tags(buffer, &start, &end);
    remove_structural_prefix(buffer, &mut start, &mut end);
    if let Some(tag_name) = tag_name {
        buffer.apply_tag_by_name(tag_name, &start, &end);
    }
}

fn current_line_bounds(buffer: &gtk::TextBuffer) -> (gtk::TextIter, gtk::TextIter) {
    let mut start = buffer.iter_at_mark(&buffer.get_insert());
    start.set_line_offset(0);
    let mut end = start;
    end.forward_to_line_end();
    (start, end)
}

fn remove_block_tags(buffer: &gtk::TextBuffer, start: &gtk::TextIter, end: &gtk::TextIter) {
    for tag in BLOCK_TAGS { buffer.remove_tag_by_name(tag, start, end); }
}

fn remove_structural_prefix(buffer: &gtk::TextBuffer, start: &mut gtk::TextIter, end: &mut gtk::TextIter) {
    let mut prefix_end = *start;
    while !prefix_end.ends_line() && prefix_end.tags().iter().any(|tag| tag.name().as_deref() == Some("rich-structural")) {
        prefix_end.forward_char();
    }
    if prefix_end.offset() > start.offset() {
        buffer.delete(start, &mut prefix_end);
        *end = *start;
        end.forward_to_line_end();
    }
}
