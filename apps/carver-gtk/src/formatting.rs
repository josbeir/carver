//! Source-editor formatting controls.
//!
//! Rich formatting belongs to the browser editor. This module intentionally
//! operates on literal Carve source only.

use std::{cell::RefCell, rc::Rc};

use adw::prelude::*;
use gtk::prelude::*;
use libadwaita as adw;

use crate::editor::source_commands;

/// Appends Carve-source formatting controls to an editor toolbar.
pub(crate) fn append_source_controls(toolbar: &gtk::Box, buffer: &gtk::TextBuffer) {
    append_source_inline_controls(toolbar, buffer);
    append_more_source_formatting(toolbar, buffer);
    append_source_heading_menu(toolbar, buffer);
    append_source_list_controls(toolbar, buffer);
    append_source_inline_code_button(toolbar, buffer);
    append_source_code_block_button(toolbar, buffer);
    append_source_link_button(toolbar, buffer);
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
    ] {
        let button = icon_button(name, icon, tooltip);
        let buffer = buffer.clone();
        button.connect_clicked(move |_| source_commands::toggle_inline(&buffer, opening, closing));
        toolbar.append(&button);
    }
}

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
        choice.set_halign(gtk::Align::Fill);
        choice.set_hexpand(true);
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
        choice.set_halign(gtk::Align::Fill);
        choice.set_hexpand(true);
        let buffer = buffer.clone();
        choice.connect_clicked(move |_| source_commands::set_heading(&buffer, level));
        choices.append(&choice);
    }
    popover.set_child(Some(&choices));
    heading.set_popover(Some(&popover));
    toolbar.append(&heading);
}

fn append_source_list_controls(toolbar: &gtk::Box, buffer: &gtk::TextBuffer) {
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
}

fn append_source_inline_code_button(toolbar: &gtk::Box, buffer: &gtk::TextBuffer) {
    let button = icon_button(
        "source-format-code-button",
        "text-editor-symbolic",
        "Inline code",
    );
    let buffer = buffer.clone();
    button.connect_clicked(move |_| source_commands::toggle_inline(&buffer, "`", "`"));
    toolbar.append(&button);
}

fn append_source_code_block_button(toolbar: &gtk::Box, buffer: &gtk::TextBuffer) {
    let button = icon_button(
        "source-format-code-block-button",
        "utilities-terminal-symbolic",
        "Code block",
    );
    let buffer = buffer.clone();
    button.connect_clicked(move |_| source_commands::toggle_code_block(&buffer));
    toolbar.append(&button);
}

fn append_source_link_button(toolbar: &gtk::Box, buffer: &gtk::TextBuffer) {
    let button = icon_button(
        "source-format-link-button",
        "insert-link-symbolic",
        "Insert link",
    );
    let buffer = buffer.clone();
    button.connect_clicked(move |button| show_source_link_dialog(button, &buffer));
    toolbar.append(&button);
}

fn icon_button(name: &str, icon: &str, tooltip: &str) -> gtk::Button {
    let button = gtk::Button::from_icon_name(icon);
    button.set_widget_name(name);
    button.set_tooltip_text(Some(tooltip));
    button.add_css_class("flat");
    button
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
