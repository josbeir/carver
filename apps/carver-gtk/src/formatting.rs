//! Native rich-text formatting commands used by the editor toolbar.

use std::{cell::RefCell, rc::Rc};

use adw::prelude::*;
use gtk::prelude::*;
use libadwaita as adw;

use crate::editor::source_commands;

const BLOCK_TAGS: [&str; 9] = [
    "rich-heading-1",
    "rich-heading-2",
    "rich-heading-3",
    "rich-heading-4",
    "rich-heading-5",
    "rich-heading-6",
    "rich-list-bullet",
    "rich-list-ordered",
    "rich-list-task",
];

/// Appends the common Carve formatting controls to an editor toolbar.
pub(crate) fn append_controls(toolbar: &gtk::Box, buffer: &gtk::TextBuffer) {
    install_tags(buffer);
    append_tag_button(
        toolbar,
        buffer,
        "format-bold-button",
        "format-text-bold-symbolic",
        "Bold (Ctrl+B)",
        "rich-bold",
    );
    append_tag_button(
        toolbar,
        buffer,
        "format-italic-button",
        "format-text-italic-symbolic",
        "Italic (Ctrl+I)",
        "rich-italic",
    );
    append_tag_button(
        toolbar,
        buffer,
        "format-strike-button",
        "format-text-strikethrough-symbolic",
        "Strikethrough (Ctrl+Shift+X)",
        "rich-strike",
    );
    append_tag_button(
        toolbar,
        buffer,
        "format-underline-button",
        "format-text-underline-symbolic",
        "Underline (Ctrl+U)",
        "rich-underline",
    );
    append_more_text_formatting(toolbar, buffer);
    append_heading_menu(toolbar, buffer);
    append_block_button(
        toolbar,
        buffer,
        "format-bullet-button",
        "view-list-bullet-symbolic",
        "Bulleted list (Ctrl+Shift+8)",
        "rich-list-bullet",
        "• ",
    );
    append_block_button(
        toolbar,
        buffer,
        "format-ordered-button",
        "view-list-ordered-symbolic",
        "Numbered list (Ctrl+Shift+7)",
        "rich-list-ordered",
        "1. ",
    );
    append_block_button(
        toolbar,
        buffer,
        "format-task-button",
        "object-select-symbolic",
        "Task list",
        "rich-list-task",
        "☐ ",
    );
    append_tag_button(
        toolbar,
        buffer,
        "format-code-button",
        "text-x-generic-symbolic",
        "Inline code",
        "rich-code",
    );
    append_link_button(toolbar, buffer);
}

/// Appends the Carve-source equivalents of the native rich editor controls.
pub(crate) fn append_source_controls(toolbar: &gtk::Box, buffer: &gtk::TextBuffer) {
    append_source_inline_controls(toolbar, buffer);
    append_source_heading_menu(toolbar, buffer);
    append_source_block_controls(toolbar, buffer);
}

fn append_source_inline_controls(toolbar: &gtk::Box, buffer: &gtk::TextBuffer) {
    for (name, icon, tooltip, opening, closing) in [
        (
            "source-format-bold-button",
            "format-text-bold-symbolic",
            "Bold (Ctrl+B)",
            "*",
            "*",
        ),
        (
            "source-format-italic-button",
            "format-text-italic-symbolic",
            "Italic (Ctrl+I)",
            "/",
            "/",
        ),
        (
            "source-format-strike-button",
            "format-text-strikethrough-symbolic",
            "Strikethrough (Ctrl+Shift+X)",
            "~",
            "~",
        ),
        (
            "source-format-underline-button",
            "format-text-underline-symbolic",
            "Underline (Ctrl+U)",
            "_",
            "_",
        ),
        (
            "source-format-code-button",
            "text-x-generic-symbolic",
            "Inline code",
            "`",
            "`",
        ),
    ] {
        let button = icon_button(name, icon, tooltip);
        let buffer = buffer.clone();
        button.connect_clicked(move |_| source_commands::toggle_inline(&buffer, opening, closing));
        toolbar.append(&button);
    }
    append_more_source_formatting(toolbar, buffer);
}

/// Groups less-frequent marks because GNOME provides no symbolic icons for them.
fn append_more_text_formatting(toolbar: &gtk::Box, buffer: &gtk::TextBuffer) {
    let menu = gtk::MenuButton::new();
    menu.set_widget_name("format-more-button");
    menu.set_icon_name("view-more-symbolic");
    menu.set_tooltip_text(Some("More text formatting"));
    menu.add_css_class("flat");
    let popover = gtk::Popover::new();
    let choices = gtk::Box::new(gtk::Orientation::Vertical, 0);
    for (label, tag) in [
        ("Highlight", "rich-highlight"),
        ("Superscript", "rich-superscript"),
        ("Subscript", "rich-subscript"),
    ] {
        let choice = gtk::Button::with_label(label);
        choice.add_css_class("flat");
        let buffer = buffer.clone();
        choice.connect_clicked(move |_| toggle_tag_on_selection(&buffer, tag));
        choices.append(&choice);
    }
    popover.set_child(Some(&choices));
    menu.set_popover(Some(&popover));
    toolbar.append(&menu);
}

/// Source-mode counterpart to the native rich editor's more-formatting menu.
fn append_more_source_formatting(toolbar: &gtk::Box, buffer: &gtk::TextBuffer) {
    let menu = gtk::MenuButton::new();
    menu.set_widget_name("source-format-more-button");
    menu.set_icon_name("view-more-symbolic");
    menu.set_tooltip_text(Some("More text formatting"));
    menu.add_css_class("flat");
    let popover = gtk::Popover::new();
    let choices = gtk::Box::new(gtk::Orientation::Vertical, 0);
    for (label, opening, closing) in [
        ("Highlight", "=", "="),
        ("Superscript", "{^", "^}"),
        ("Subscript", "{,", ",}"),
    ] {
        let choice = gtk::Button::with_label(label);
        choice.add_css_class("flat");
        let buffer = buffer.clone();
        choice.connect_clicked(move |_| source_commands::toggle_inline(&buffer, opening, closing));
        choices.append(&choice);
    }
    popover.set_child(Some(&choices));
    menu.set_popover(Some(&popover));
    toolbar.append(&menu);
}

fn append_source_heading_menu(toolbar: &gtk::Box, buffer: &gtk::TextBuffer) {
    let heading = gtk::MenuButton::new();
    heading.set_widget_name("source-format-heading-button");
    heading.set_icon_name("format-text-rich-symbolic");
    heading.set_tooltip_text(Some("Text style"));
    heading.add_css_class("flat");
    let popover = gtk::Popover::new();
    let choices = gtk::Box::new(gtk::Orientation::Vertical, 0);
    for (label, level) in [
        ("Normal text", 0),
        ("Heading 1", 1),
        ("Heading 2", 2),
        ("Heading 3", 3),
        ("Heading 4", 4),
        ("Heading 5", 5),
        ("Heading 6", 6),
    ] {
        let choice = gtk::Button::with_label(label);
        choice.add_css_class("flat");
        let buffer = buffer.clone();
        choice.connect_clicked(move |_| source_commands::set_heading(&buffer, level));
        choices.append(&choice);
    }
    popover.set_child(Some(&choices));
    heading.set_popover(Some(&popover));
    toolbar.append(&heading);
}

fn append_source_block_controls(toolbar: &gtk::Box, buffer: &gtk::TextBuffer) {
    for (name, icon, tooltip, prefix) in [
        (
            "source-format-bullet-button",
            "view-list-bullet-symbolic",
            "Bulleted list (Ctrl+Shift+8)",
            "- ",
        ),
        (
            "source-format-ordered-button",
            "view-list-ordered-symbolic",
            "Numbered list (Ctrl+Shift+7)",
            "1. ",
        ),
        (
            "source-format-task-button",
            "object-select-symbolic",
            "Task list",
            "- [ ] ",
        ),
    ] {
        let button = icon_button(name, icon, tooltip);
        let buffer = buffer.clone();
        button.connect_clicked(move |_| source_commands::toggle_list(&buffer, prefix));
        toolbar.append(&button);
    }

    let code_block = icon_button(
        "source-format-code-block-button",
        "text-x-generic-symbolic",
        "Code block",
    );
    let source = buffer.clone();
    code_block.connect_clicked(move |_| source_commands::toggle_code_block(&source));
    toolbar.append(&code_block);

    let link = icon_button(
        "source-format-link-button",
        "insert-link-symbolic",
        "Insert link",
    );
    let source = buffer.clone();
    link.connect_clicked(move |button| show_source_link_dialog(button, &source));
    toolbar.append(&link);
}

/// Installs visual and structural tags used by the native rich projection.
pub(crate) fn install_tags(buffer: &gtk::TextBuffer) {
    ensure_tag(buffer, "rich-bold", |tag| tag.set_weight(700));
    ensure_tag(buffer, "rich-italic", |tag| {
        tag.set_style(gtk::pango::Style::Italic);
    });
    ensure_tag(buffer, "rich-strike", |tag| tag.set_strikethrough(true));
    ensure_tag(buffer, "rich-code", |tag| tag.set_family(Some("monospace")));
    ensure_tag(buffer, "rich-underline", |tag| {
        tag.set_underline(gtk::pango::Underline::Single);
    });
    ensure_tag(buffer, "rich-highlight", |_| {});
    ensure_tag(buffer, "rich-superscript", |tag| {
        tag.set_rise(6_000);
        tag.set_scale(0.8);
    });
    ensure_tag(buffer, "rich-subscript", |tag| {
        tag.set_rise(-3_000);
        tag.set_scale(0.8);
    });
    ensure_tag(buffer, "rich-inserted", |tag| {
        tag.set_underline(gtk::pango::Underline::Single);
        tag.set_foreground(Some("#26a269"));
    });
    ensure_tag(buffer, "rich-deleted", |tag| {
        tag.set_strikethrough(true);
        tag.set_foreground(Some("#c01c28"));
    });
    ensure_tag(buffer, "rich-quote", |tag| {
        tag.set_left_margin(20);
        tag.set_indent(-12);
        tag.set_style(gtk::pango::Style::Italic);
    });
    ensure_tag(buffer, "rich-rule", |tag| {
        tag.set_foreground(Some("#77767b"));
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
    for (name, scale) in [
        ("rich-heading-4", 1.05),
        ("rich-heading-5", 1.0),
        ("rich-heading-6", 0.95),
    ] {
        ensure_tag(buffer, name, |tag| {
            tag.set_weight(700);
            tag.set_scale(scale);
            tag.set_pixels_above_lines(6);
            tag.set_pixels_below_lines(3);
        });
    }
    for name in ["rich-list-bullet", "rich-list-ordered", "rich-list-task"] {
        ensure_tag(buffer, name, |tag| {
            tag.set_left_margin(28);
            tag.set_indent(-20);
        });
    }
    ensure_tag(buffer, "rich-list-task-checked", |_| {});
    ensure_tag(buffer, "rich-structural", |tag| tag.set_editable(false));
    ensure_tag(buffer, "rich-hidden-source", |tag| tag.set_invisible(true));
    ensure_tag(buffer, "rich-image-break", |_| {});
}

/// Applies colors derived from the active GNOME theme to editor-only rich tags.
pub(crate) fn apply_theme_colors(buffer: &gtk::TextBuffer) {
    let manager = adw::StyleManager::default();
    let dark = manager.is_dark();
    let accent = manager.accent_color_rgba();
    let accent_foreground = contrasting_foreground(&accent);
    let background = if dark {
        gtk::gdk::RGBA::new(0.12, 0.12, 0.13, 1.0)
    } else {
        gtk::gdk::RGBA::new(1.0, 1.0, 1.0, 1.0)
    };
    let foreground = if dark {
        gtk::gdk::RGBA::new(0.96, 0.96, 0.97, 1.0)
    } else {
        gtk::gdk::RGBA::new(0.12, 0.12, 0.13, 1.0)
    };
    let inline_code_background = blend(&background, &accent, if dark { 0.2 } else { 0.1 });
    let block_background = blend(&background, &accent, if dark { 0.12 } else { 0.06 });

    set_tag_colors(buffer, "rich-code", &inline_code_background, &accent);
    set_tag_colors(buffer, "rich-highlight", &accent, &accent_foreground);
    buffer.tag_table().foreach(|tag| {
        if tag
            .name()
            .as_deref()
            .is_some_and(|name| name.starts_with("rich-code-block-"))
        {
            tag.set_background_rgba(Some(&block_background));
            tag.set_paragraph_background_rgba(Some(&block_background));
            tag.set_foreground_rgba(Some(&foreground));
        }
    });
}

fn contrasting_foreground(background: &gtk::gdk::RGBA) -> gtk::gdk::RGBA {
    let luminance =
        0.2126 * background.red() + 0.7152 * background.green() + 0.0722 * background.blue();
    if luminance > 0.55 {
        gtk::gdk::RGBA::new(0.0, 0.0, 0.0, 1.0)
    } else {
        gtk::gdk::RGBA::new(1.0, 1.0, 1.0, 1.0)
    }
}

fn set_tag_colors(
    buffer: &gtk::TextBuffer,
    name: &str,
    background: &gtk::gdk::RGBA,
    foreground: &gtk::gdk::RGBA,
) {
    if let Some(tag) = buffer.tag_table().lookup(name) {
        tag.set_background_rgba(Some(background));
        tag.set_foreground_rgba(Some(foreground));
    }
}

fn blend(background: &gtk::gdk::RGBA, overlay: &gtk::gdk::RGBA, amount: f32) -> gtk::gdk::RGBA {
    let remaining = 1.0 - amount;
    gtk::gdk::RGBA::new(
        background.red() * remaining + overlay.red() * amount,
        background.green() * remaining + overlay.green() * amount,
        background.blue() * remaining + overlay.blue() * amount,
        1.0,
    )
}

fn append_link_button(toolbar: &gtk::Box, buffer: &gtk::TextBuffer) {
    let button = icon_button("format-link-button", "insert-link-symbolic", "Insert link");
    let buffer = buffer.clone();
    button.connect_clicked(move |button| show_link_dialog(button, &buffer));
    toolbar.append(&button);
}

fn show_link_dialog(button: &gtk::Button, buffer: &gtk::TextBuffer) {
    let content = gtk::Box::new(gtk::Orientation::Vertical, 8);
    let text = gtk::Entry::new();
    text.set_widget_name("link-text-entry");
    text.set_placeholder_text(Some("Link text"));
    if let Some((start, end)) = buffer.selection_bounds() {
        text.set_text(&buffer.text(&start, &end, false));
    }
    let url = gtk::Entry::new();
    url.set_widget_name("link-url-entry");
    url.set_placeholder_text(Some("https://example.com"));
    url.set_input_purpose(gtk::InputPurpose::Url);
    content.append(&gtk::Label::new(Some("Text")));
    content.append(&text);
    content.append(&gtk::Label::new(Some("Address")));
    content.append(&url);
    let dialog = adw::AlertDialog::builder()
        .heading("Insert Link")
        .extra_child(&content)
        .default_response("insert")
        .close_response("cancel")
        .build();
    dialog.add_responses(&[("cancel", "Cancel"), ("insert", "Insert")]);
    let buffer = buffer.clone();
    let selection = Rc::new(RefCell::new(capture_selection(&buffer)));
    let selection_for_response = Rc::clone(&selection);
    dialog.connect_response(None, move |_dialog, response| {
        restore_selection(&buffer, selection_for_response.borrow_mut().take());
        if response == "insert" {
            let destination = url.text();
            let link_text = text.text();
            if !destination.trim().is_empty() && !link_text.trim().is_empty() {
                insert_link(&buffer, &link_text, &destination);
            }
        }
    });
    dialog.present(button.root().as_ref());
}

fn show_source_link_dialog(button: &gtk::Button, buffer: &gtk::TextBuffer) {
    let content = gtk::Box::new(gtk::Orientation::Vertical, 8);
    let text = gtk::Entry::new();
    text.set_placeholder_text(Some("Link text"));
    if let Some((start, end)) = buffer.selection_bounds() {
        text.set_text(&buffer.text(&start, &end, false));
    }
    let url = gtk::Entry::new();
    url.set_placeholder_text(Some("https://example.com"));
    url.set_input_purpose(gtk::InputPurpose::Url);
    content.append(&gtk::Label::new(Some("Text")));
    content.append(&text);
    content.append(&gtk::Label::new(Some("Address")));
    content.append(&url);
    let dialog = adw::AlertDialog::builder()
        .heading("Insert Link")
        .extra_child(&content)
        .default_response("insert")
        .close_response("cancel")
        .build();
    dialog.add_responses(&[("cancel", "Cancel"), ("insert", "Insert")]);
    let source = buffer.clone();
    let selection = Rc::new(RefCell::new(capture_selection(&source)));
    let selection_for_response = Rc::clone(&selection);
    dialog.connect_response(None, move |_dialog, response| {
        restore_selection(&source, selection_for_response.borrow_mut().take());
        if response == "insert" {
            let destination = url.text();
            let link_text = text.text();
            if !destination.trim().is_empty() && !link_text.trim().is_empty() {
                source_commands::insert_link(&source, &link_text, &destination);
            }
        }
    });
    dialog.present(button.root().as_ref());
}

struct SelectionMarks {
    start: gtk::TextMark,
    end: gtk::TextMark,
}

fn capture_selection(buffer: &gtk::TextBuffer) -> Option<SelectionMarks> {
    let (start, end) = buffer.selection_bounds()?;
    Some(SelectionMarks {
        start: buffer.create_mark(None, &start, true),
        end: buffer.create_mark(None, &end, false),
    })
}

fn restore_selection(buffer: &gtk::TextBuffer, selection: Option<SelectionMarks>) {
    let Some(selection) = selection else {
        return;
    };
    let start = buffer.iter_at_mark(&selection.start);
    let end = buffer.iter_at_mark(&selection.end);
    buffer.select_range(&start, &end);
    buffer.delete_mark(&selection.start);
    buffer.delete_mark(&selection.end);
}

/// Returns the destination attached to the link tag at an iterator.
pub(crate) fn link_destination(iter: &gtk::TextIter) -> Option<String> {
    encoded_tag_value(iter, "rich-link-")
}

/// Applies a destination-specific visual link tag to a range.
pub(crate) fn apply_link_tag(
    buffer: &gtk::TextBuffer,
    start: &gtk::TextIter,
    end: &gtk::TextIter,
    destination: &str,
) {
    let name = encoded_tag_name("rich-link-", destination);
    if buffer.tag_table().lookup(&name).is_none()
        && let Some(tag) = buffer.create_tag(Some(&name), &[])
    {
        tag.set_underline(gtk::pango::Underline::Single);
    }
    buffer.apply_tag_by_name(&name, start, end);
}

/// Applies the visual tag and language metadata for a fenced code block.
pub(crate) fn apply_code_block_tag(
    buffer: &gtk::TextBuffer,
    start: &gtk::TextIter,
    end: &gtk::TextIter,
    language: Option<&str>,
) {
    let name = encoded_tag_name("rich-code-block-", language.unwrap_or_default());
    if buffer.tag_table().lookup(&name).is_none()
        && let Some(tag) = buffer.create_tag(Some(&name), &[])
    {
        tag.set_family(Some("monospace"));
        tag.set_left_margin(12);
        tag.set_right_margin(12);
        tag.set_pixels_above_lines(8);
        tag.set_pixels_below_lines(8);
    }
    buffer.apply_tag_by_name(&name, start, end);
}

/// Returns the fenced-code language attached to an iterator, if any.
pub(crate) fn code_block_language(iter: &gtk::TextIter) -> Option<String> {
    encoded_tag_value(iter, "rich-code-block-")
}

/// Applies an attribute metadata tag without exposing Carve delimiters in rich text.
pub(crate) fn apply_attribute_tag(
    buffer: &gtk::TextBuffer,
    start: &gtk::TextIter,
    end: &gtk::TextIter,
    attributes: &str,
) {
    let name = encoded_tag_name("rich-attribute-", attributes);
    if buffer.tag_table().lookup(&name).is_none()
        && let Some(tag) = buffer.create_tag(Some(&name), &[])
    {
        tag.set_underline(gtk::pango::Underline::Single);
        tag.set_underline_rgba(Some(&gtk::gdk::RGBA::new(0.5, 0.5, 0.5, 1.0)));
    }
    buffer.apply_tag_by_name(&name, start, end);
}

/// Returns the Carve attributes attached to an iterator, if any.
pub(crate) fn attribute_spec(iter: &gtk::TextIter) -> Option<String> {
    encoded_tag_value(iter, "rich-attribute-")
}

fn insert_link(buffer: &gtk::TextBuffer, text: &str, destination: &str) {
    let Ok(text_width) = i32::try_from(text.chars().count()) else {
        return;
    };
    if let Some((mut start, mut end)) = buffer.selection_bounds() {
        buffer.delete(&mut start, &mut end);
        let start_offset = start.offset();
        buffer.insert(&mut start, text);
        let range_start = buffer.iter_at_offset(start_offset);
        let range_end = buffer.iter_at_offset(start_offset + text_width);
        apply_link_tag(buffer, &range_start, &range_end, destination);
    } else {
        let mut start = buffer.iter_at_mark(&buffer.get_insert());
        let start_offset = start.offset();
        buffer.insert(&mut start, text);
        let range_start = buffer.iter_at_offset(start_offset);
        let range_end = buffer.iter_at_offset(start_offset + text_width);
        apply_link_tag(buffer, &range_start, &range_end, destination);
    }
}

fn encoded_tag_name(prefix: &str, value: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(value.len().saturating_mul(2));
    for byte in value.bytes() {
        encoded.push(HEX[usize::from(byte >> 4)] as char);
        encoded.push(HEX[usize::from(byte & 0x0f)] as char);
    }
    format!("{prefix}{encoded}")
}

fn encoded_tag_value(iter: &gtk::TextIter, prefix: &str) -> Option<String> {
    let value_at = |iter: &gtk::TextIter| {
        iter.tags().iter().find_map(|tag| {
            tag.name()
                .as_deref()
                .and_then(|name| name.strip_prefix(prefix))
                .and_then(decode_tag_value)
        })
    };
    let mut current = *iter;
    loop {
        if let Some(value) = value_at(&current) {
            return Some(value);
        }
        if !current.forward_char() || current.line() != iter.line() {
            return None;
        }
    }
}

fn decode_tag_value(encoded: &str) -> Option<String> {
    if !encoded.len().is_multiple_of(2) {
        return None;
    }
    let bytes = (0..encoded.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(&encoded[index..index + 2], 16).ok())
        .collect::<Option<Vec<_>>>()?;
    String::from_utf8(bytes).ok()
}

fn ensure_tag(buffer: &gtk::TextBuffer, name: &str, configure: impl FnOnce(&gtk::TextTag)) {
    if buffer.tag_table().lookup(name).is_none()
        && let Some(tag) = buffer.create_tag(Some(name), &[])
    {
        configure(&tag);
    }
}

fn append_tag_button(
    toolbar: &gtk::Box,
    buffer: &gtk::TextBuffer,
    name: &str,
    icon: &str,
    tooltip: &str,
    tag_name: &str,
) {
    let button = icon_button(name, icon, tooltip);
    let buffer = buffer.clone();
    let tag_name = tag_name.to_owned();
    button.connect_clicked(move |_| toggle_tag_on_selection(&buffer, &tag_name));
    toolbar.append(&button);
}

fn append_block_button(
    toolbar: &gtk::Box,
    buffer: &gtk::TextBuffer,
    name: &str,
    icon: &str,
    tooltip: &str,
    tag_name: &str,
    marker: &str,
) {
    let button = icon_button(name, icon, tooltip);
    let buffer = buffer.clone();
    let tag_name = tag_name.to_owned();
    let marker = marker.to_owned();
    button.connect_clicked(move |_| toggle_selected_blocks(&buffer, &tag_name, &marker));
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
    for (label, tag) in [
        ("Normal text", None),
        ("Heading 1", Some("rich-heading-1")),
        ("Heading 2", Some("rich-heading-2")),
        ("Heading 3", Some("rich-heading-3")),
        ("Heading 4", Some("rich-heading-4")),
        ("Heading 5", Some("rich-heading-5")),
        ("Heading 6", Some("rich-heading-6")),
    ] {
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

/// Toggles an inline formatting tag over the current selection.
pub(crate) fn toggle_tag_on_selection(buffer: &gtk::TextBuffer, tag_name: &str) {
    let Some((start, end)) = buffer.selection_bounds() else {
        return;
    };
    if start
        .tags()
        .iter()
        .any(|tag| tag.name().as_deref() == Some(tag_name))
    {
        buffer.remove_tag_by_name(tag_name, &start, &end);
    } else {
        buffer.apply_tag_by_name(tag_name, &start, &end);
    }
}

/// Toggles a structural block tag for every line touched by the selection.
pub(crate) fn toggle_selected_blocks(buffer: &gtk::TextBuffer, tag_name: &str, marker: &str) {
    let (first_line, last_line) = selected_line_range(buffer);
    let active = (first_line..=last_line).all(|line| {
        buffer
            .iter_at_line(line)
            .is_some_and(|start| has_tag(&start, tag_name))
    });
    for line in first_line..=last_line {
        let Some(mut start) = buffer.iter_at_line(line) else {
            continue;
        };
        let mut end = start;
        end.forward_to_line_end();
        toggle_line_block(buffer, tag_name, marker, active, &mut start, &mut end);
    }
}

fn toggle_line_block(
    buffer: &gtk::TextBuffer,
    tag_name: &str,
    marker: &str,
    active: bool,
    start: &mut gtk::TextIter,
    end: &mut gtk::TextIter,
) {
    remove_block_tags(buffer, start, end);
    remove_structural_prefix(buffer, start, end);
    if !active {
        buffer.insert(start, marker);
        let marker_end = *start;
        let mut marker_start = marker_end;
        let Ok(marker_width) = i32::try_from(marker.chars().count()) else {
            return;
        };
        marker_start.backward_chars(marker_width);
        buffer.apply_tag_by_name("rich-structural", &marker_start, &marker_end);
        *end = marker_end;
        end.forward_to_line_end();
        buffer.apply_tag_by_name(tag_name, &marker_start, end);
    }
}

fn selected_line_range(buffer: &gtk::TextBuffer) -> (i32, i32) {
    let Some((mut start, mut end)) = buffer.selection_bounds() else {
        let (start, _) = current_line_bounds(buffer);
        return (start.line(), start.line());
    };
    start.set_line_offset(0);
    if end.starts_line() && end.line() > start.line() {
        end.backward_char();
    }
    (start.line(), end.line())
}

fn has_tag(iter: &gtk::TextIter, tag_name: &str) -> bool {
    iter.tags()
        .iter()
        .any(|tag| tag.name().as_deref() == Some(tag_name))
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
    for tag in BLOCK_TAGS {
        buffer.remove_tag_by_name(tag, start, end);
    }
}

fn remove_structural_prefix(
    buffer: &gtk::TextBuffer,
    start: &mut gtk::TextIter,
    end: &mut gtk::TextIter,
) {
    let mut prefix_end = *start;
    while !prefix_end.ends_line()
        && prefix_end
            .tags()
            .iter()
            .any(|tag| tag.name().as_deref() == Some("rich-structural"))
    {
        prefix_end.forward_char();
    }
    if prefix_end.offset() > start.offset() {
        buffer.delete(start, &mut prefix_end);
        *end = *start;
        end.forward_to_line_end();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "requires a graphical display; CI runs it under Xvfb"]
    fn restored_link_selection_replaces_instead_of_duplicating_text()
    -> Result<(), Box<dyn std::error::Error>> {
        gtk::init()?;
        let buffer = gtk::TextBuffer::new(None);
        buffer.set_text("Carver link");
        let start = buffer.start_iter();
        let end = buffer.end_iter();
        buffer.select_range(&start, &end);
        let selection = capture_selection(&buffer);
        buffer.place_cursor(&buffer.end_iter());

        restore_selection(&buffer, selection);
        insert_link(&buffer, "Carver link", "https://carver.invalid");

        assert_eq!(buffer_text(&buffer), "Carver link");
        assert_eq!(
            link_destination(&buffer.start_iter()).as_deref(),
            Some("https://carver.invalid")
        );
        Ok(())
    }

    fn buffer_text(buffer: &gtk::TextBuffer) -> String {
        buffer
            .text(&buffer.start_iter(), &buffer.end_iter(), false)
            .to_string()
    }
}
