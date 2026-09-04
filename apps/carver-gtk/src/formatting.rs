//! Shared native formatting dialogs, managed-image import, and table controls.

use std::{cell::RefCell, rc::Rc};

use adw::prelude::*;
use gtk::prelude::*;
use libadwaita as adw;

use crate::{
    editor::source_commands,
    mvu::{AppDispatcher, AppMsg, EditorMsg},
};

/// Opens the native image chooser and stores the selected file as a note asset.
///
/// The callback only receives asset paths created by the storage client; source
/// and rich editing therefore cannot accidentally persist machine-local paths.
pub(crate) fn choose_managed_image(
    button: &gtk::Button,
    dispatcher: &AppDispatcher,
    toast_overlay: &adw::ToastOverlay,
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
    let dispatcher = dispatcher.clone();
    let toast_overlay = toast_overlay.clone();
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
                &dispatcher,
                &toast_overlay,
                dialog_parent.as_ref(),
            );
        },
    );
}

fn show_image_alt_dialog(
    file: &gtk::gio::File,
    suggested_alt: &str,
    dispatcher: &AppDispatcher,
    toast_overlay: &adw::ToastOverlay,
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
    let dispatcher = dispatcher.clone();
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
            &dispatcher,
            &toast_overlay,
            extension,
        );
    });
    dialog.present(parent);
}

/// Stores a local image and invokes `on_insert` with its portable asset path.
pub(crate) fn import_managed_image_file(
    file: &gtk::gio::File,
    alt: &str,
    dispatcher: &AppDispatcher,
    toast_overlay: &adw::ToastOverlay,
    extension: &'static str,
) {
    let alt = alt.to_owned();
    let dispatcher = dispatcher.clone();
    let toast_overlay = toast_overlay.clone();
    file.load_bytes_async(None::<&gtk::gio::Cancellable>, move |result| {
        let Ok((bytes, _)) = result else {
            toast_overlay.add_toast(adw::Toast::new("Could not read the selected image"));
            return;
        };
        let _ = dispatcher.dispatch(AppMsg::Editor(EditorMsg::ImportImage {
            extension: extension.to_owned(),
            bytes: bytes.as_ref().to_vec(),
            alt,
            source_selection: None,
        }));
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

pub(crate) fn show_source_link_dialog(anchor: &impl IsA<gtk::Widget>, buffer: &gtk::TextBuffer) {
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
    dialog.present(anchor.root().as_ref());
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
