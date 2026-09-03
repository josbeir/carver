//! Source-editor formatting controls.
//!
//! Rich formatting belongs to the browser editor. This module intentionally
//! operates on literal Carve source only.

use std::{cell::RefCell, rc::Rc};

use adw::prelude::*;
use gtk::prelude::*;
use libadwaita as adw;

use crate::{controller::AppState, editor::source_commands};

/// Appends Carve-source formatting controls to an editor toolbar.
pub(crate) fn append_source_controls(
    toolbar: &gtk::Box,
    buffer: &gtk::TextBuffer,
    state: &Rc<AppState>,
    toast_overlay: &adw::ToastOverlay,
) {
    append_source_inline_controls(toolbar, buffer);
    append_more_source_formatting(toolbar, buffer);
    append_source_heading_menu(toolbar, buffer);
    append_source_list_controls(toolbar, buffer);
    append_source_inline_code_button(toolbar, buffer);
    append_source_code_block_button(toolbar, buffer);
    append_source_link_button(toolbar, buffer);
    append_source_table_picker(toolbar, buffer);
    append_source_image_menu(toolbar, buffer, state, toast_overlay);
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

fn append_source_table_picker(toolbar: &gtk::Box, buffer: &gtk::TextBuffer) {
    let buffer = buffer.clone();
    append_table_picker(
        toolbar,
        "source-format-table-button",
        move |rows, columns, header| {
            source_commands::insert_table(&buffer, rows, columns, header);
        },
    );
}

fn append_source_image_menu(
    toolbar: &gtk::Box,
    buffer: &gtk::TextBuffer,
    state: &Rc<AppState>,
    toast_overlay: &adw::ToastOverlay,
) {
    let menu = gtk::MenuButton::new();
    menu.set_widget_name("source-format-image-button");
    menu.set_icon_name("image-x-generic-symbolic");
    menu.set_tooltip_text(Some("Insert or resize image"));
    menu.add_css_class("flat");
    let choices = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let insert = gtk::Button::with_label("Insert image…");
    insert.add_css_class("flat");
    let buffer_for_insert = buffer.clone();
    let state = Rc::clone(state);
    let toast_overlay = toast_overlay.clone();
    insert.connect_clicked(move |button| {
        let buffer = buffer_for_insert.clone();
        choose_managed_image(button, &state, &toast_overlay, move |path, alt| {
            source_commands::insert_image(&buffer, &alt, &path);
        });
    });
    choices.append(&insert);
    choices.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
    for (label, width) in [
        ("Original size", None),
        ("25%", Some(25)),
        ("50%", Some(50)),
        ("75%", Some(75)),
        ("100%", Some(100)),
    ] {
        let choice = gtk::Button::with_label(label);
        choice.add_css_class("flat");
        let buffer = buffer.clone();
        choice.connect_clicked(move |_| {
            let _ = source_commands::set_image_width(&buffer, width);
        });
        choices.append(&choice);
    }
    let popover = gtk::Popover::new();
    popover.set_child(Some(&choices));
    menu.set_popover(Some(&popover));
    toolbar.append(&menu);
}

/// Opens the native image chooser and stores the selected file as a note asset.
///
/// The callback only receives asset paths created by the storage client; source
/// and rich editing therefore cannot accidentally persist machine-local paths.
pub(crate) fn choose_managed_image(
    button: &gtk::Button,
    state: &Rc<AppState>,
    toast_overlay: &adw::ToastOverlay,
    on_insert: impl Fn(String, String) + 'static,
) {
    let filter = gtk::FileFilter::new();
    filter.set_name(Some("Images"));
    for mime_type in [
        "image/png",
        "image/jpeg",
        "image/gif",
        "image/webp",
        "image/svg+xml",
    ] {
        filter.add_mime_type(mime_type);
    }
    let filters = gtk::gio::ListStore::new::<gtk::FileFilter>();
    filters.append(&filter);
    let dialog = gtk::FileDialog::builder().title("Insert Image").build();
    dialog.set_filters(Some(&filters));
    dialog.set_default_filter(Some(&filter));
    let parent = button.root().and_downcast::<gtk::Window>();
    let state = Rc::clone(state);
    let toast_overlay = toast_overlay.clone();
    let on_insert: Rc<dyn Fn(String, String)> = Rc::new(on_insert);
    let dialog_parent = parent.clone();
    dialog.open(
        parent.as_ref(),
        None::<&gtk::gio::Cancellable>,
        move |result| {
            let Ok(file) = result else {
                return;
            };
            let name = file
                .basename()
                .and_then(|name| name.into_os_string().into_string().ok())
                .unwrap_or_else(|| String::from("Image"));
            let alt = name
                .rsplit_once('.')
                .map_or_else(|| name.clone(), |(stem, _)| stem.to_owned());
            show_image_alt_dialog(
                &file,
                &alt,
                &state,
                &toast_overlay,
                Rc::clone(&on_insert),
                dialog_parent.as_ref(),
            );
        },
    );
}

fn show_image_alt_dialog(
    file: &gtk::gio::File,
    suggested_alt: &str,
    state: &Rc<AppState>,
    toast_overlay: &adw::ToastOverlay,
    on_insert: Rc<dyn Fn(String, String)>,
    parent: Option<&gtk::Window>,
) {
    let alt = gtk::Entry::new();
    alt.set_text(suggested_alt);
    alt.set_placeholder_text(Some("Description (optional)"));
    let dialog = adw::AlertDialog::builder()
        .heading("Image description")
        .body("Used as alternative text when the image cannot be displayed.")
        .extra_child(&alt)
        .default_response("insert")
        .close_response("cancel")
        .build();
    dialog.add_responses(&[("cancel", "Cancel"), ("insert", "Insert")]);
    let file = file.clone();
    let state = Rc::clone(state);
    let toast_overlay = toast_overlay.clone();
    dialog.connect_response(None, move |_dialog, response| {
        if response != "insert" {
            return;
        }
        let Some(extension) = image_extension_for_file(&file) else {
            toast_overlay.add_toast(adw::Toast::new("Unsupported image format"));
            return;
        };
        import_managed_image_file(
            &file,
            alt.text().as_str(),
            &state,
            &toast_overlay,
            Rc::clone(&on_insert),
            extension,
        );
    });
    dialog.present(parent);
}

/// Stores a local image and invokes `on_insert` with its portable asset path.
pub(crate) fn import_managed_image_file(
    file: &gtk::gio::File,
    alt: &str,
    state: &Rc<AppState>,
    toast_overlay: &adw::ToastOverlay,
    on_insert: Rc<dyn Fn(String, String)>,
    extension: &'static str,
) {
    let alt = alt.to_owned();
    let state = Rc::clone(state);
    let toast_overlay = toast_overlay.clone();
    file.load_bytes_async(None::<&gtk::gio::Cancellable>, move |result| {
        let Ok((bytes, _)) = result else {
            toast_overlay.add_toast(adw::Toast::new("Could not read the selected image"));
            return;
        };
        let Some(note) = state.current_note.borrow().clone() else {
            toast_overlay.add_toast(adw::Toast::new("Select a note before inserting an image"));
            return;
        };
        let client = state.client.clone();
        let toast_overlay = toast_overlay.clone();
        glib::spawn_future_local(async move {
            match client
                .store_asset_async(note.id, extension.to_owned(), bytes.as_ref().to_vec())
                .await
            {
                Ok(path) => on_insert(path, alt),
                Err(error) => toast_overlay
                    .add_toast(adw::Toast::new(&format!("Could not store image: {error}"))),
            }
        });
    });
}

pub(crate) fn image_extension_for_file(file: &gtk::gio::File) -> Option<&'static str> {
    let extension = file
        .basename()?
        .extension()?
        .to_string_lossy()
        .to_ascii_lowercase();
    match extension.as_str() {
        "png" => Some("png"),
        "jpg" | "jpeg" => Some("jpg"),
        "gif" => Some("gif"),
        "webp" => Some("webp"),
        "svg" => Some("svg"),
        _ => None,
    }
}

pub(crate) fn image_alt_for_file(file: &gtk::gio::File) -> String {
    let name = file.basename().map_or_else(
        || String::from("Image"),
        |name| name.to_string_lossy().into_owned(),
    );
    name.rsplit_once('.')
        .map_or(name.clone(), |(stem, _)| stem.to_owned())
}

/// Appends the shared hoverable table-size picker used by both editing modes.
pub(crate) fn append_table_picker(
    toolbar: &gtk::Box,
    name: &str,
    on_insert: impl Fn(u8, u8, bool) + 'static,
) -> gtk::MenuButton {
    let menu = gtk::MenuButton::new();
    menu.set_widget_name(name);
    menu.set_icon_name("view-grid-symbolic");
    menu.set_tooltip_text(Some("Insert table"));
    menu.add_css_class("flat");

    let content = gtk::Box::new(gtk::Orientation::Vertical, 12);
    content.add_css_class("table-size-picker");
    content.set_size_request(300, -1);
    content.set_margin_start(16);
    content.set_margin_end(16);
    content.set_margin_top(16);
    content.set_margin_bottom(16);
    let dimensions = gtk::Label::new(Some("1 × 1"));
    dimensions.set_halign(gtk::Align::Center);
    content.append(&dimensions);
    let grid = gtk::Grid::new();
    // Keep the picker intentional at every popover width: the cells share the
    // available width instead of leaving a detached grid in the middle.
    grid.set_halign(gtk::Align::Fill);
    grid.set_hexpand(true);
    grid.set_column_homogeneous(true);
    grid.set_row_spacing(4);
    grid.set_column_spacing(4);
    content.append(&grid);
    let header_row = gtk::Switch::new();
    header_row.set_active(true);
    let header_box = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    header_box.append(&gtk::Label::new(Some("Header row")));
    header_box.append(&header_row);
    content.append(&header_box);

    let popover = gtk::Popover::new();
    popover.set_child(Some(&content));
    let cells = Rc::new(RefCell::new(Vec::new()));
    let on_insert: Rc<dyn Fn(u8, u8, bool)> = Rc::new(on_insert);
    for row in 1..=4 {
        for column in 1..=6 {
            let cell = gtk::Button::new();
            cell.add_css_class("table-size-cell");
            cell.set_hexpand(true);
            cell.set_tooltip_text(Some(&format!("{row} rows × {column} columns")));
            grid.attach(&cell, column - 1, row - 1, 1, 1);
            cells.borrow_mut().push((row, column, cell));
        }
    }
    for (row, column, cell) in cells.borrow().iter() {
        let dimensions = dimensions.clone();
        let cells_for_motion = Rc::clone(&cells);
        let motion = gtk::EventControllerMotion::new();
        let row = row.to_owned();
        let column = column.to_owned();
        motion.connect_enter(move |_, _, _| {
            dimensions.set_text(&format!("{row} × {column}"));
            for (cell_row, cell_column, cell) in cells_for_motion.borrow().iter() {
                if *cell_row <= row && *cell_column <= column {
                    cell.add_css_class("selected");
                } else {
                    cell.remove_css_class("selected");
                }
            }
        });
        cell.add_controller(motion);

        let on_insert = Rc::clone(&on_insert);
        let header_row = header_row.clone();
        let popover = popover.clone();
        let row = row.to_owned();
        let column = column.to_owned();
        cell.connect_clicked(move |_| {
            let (Ok(row), Ok(column)) = (u8::try_from(row), u8::try_from(column)) else {
                return;
            };
            on_insert(row, column, header_row.is_active());
            popover.popdown();
        });
    }
    menu.set_popover(Some(&popover));
    toolbar.append(&menu);
    menu
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
