//! Shared native formatting dialogs, managed-image import, and table controls.

use std::{cell::RefCell, rc::Rc};

use adw::prelude::*;
use carver_editor_protocol::TableSelection;
use gettextrs::gettext;
use gtk::prelude::*;
use libadwaita as adw;

use super::editor::{focus::EditorFocusRestorer, source_commands};
use crate::mvu::{AppDispatcher, AppMsg, EditorMsg, ImportFileSource, ImportTarget};

/// Indexed cells making up the table-size hover grid.
///
/// The list is built once and never mutated afterwards, so it needs no interior
/// mutability: reading it never holds a `RefCell` borrow across GTK calls.
type TablePickerCells = Rc<Vec<(u8, u8, gtk::Button)>>;

/// Largest table the picker grid can express. Larger live tables stay read-only.
const TABLE_PICKER_ROWS: u8 = 4;
const TABLE_PICKER_COLUMNS: u8 = 6;

/// Opens the native image chooser and stores the selected file as a note asset.
///
/// The callback only receives asset paths created by the storage client; source
/// and rich editing therefore cannot accidentally persist machine-local paths.
pub(crate) fn choose_managed_image(
    button: &impl IsA<gtk::Widget>,
    dispatcher: &AppDispatcher,
    toast_overlay: &adw::ToastOverlay,
    source_target: ImportTarget,
    focus: &EditorFocusRestorer,
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
    let dialog = gtk::FileDialog::builder()
        .title(gettext("Insert Image"))
        .build();
    dialog.set_filters(Some(&filters));
    dialog.set_default_filter(Some(&filter));
    let parent = button.root().and_downcast::<gtk::Window>();
    let dispatcher = dispatcher.clone();
    let toast_overlay = toast_overlay.clone();
    let focus = focus.clone();
    let dialog_parent = parent.clone();
    dialog.open(
        parent.as_ref(),
        None::<&gtk::gio::Cancellable>,
        move |result| {
            let Ok(file) = result else {
                focus.restore_later();
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
                source_target,
                &focus,
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
    source_target: ImportTarget,
    focus: &EditorFocusRestorer,
) {
    let alt = gtk::Entry::new();
    alt.set_text(suggested_alt);
    alt.set_placeholder_text(Some(&gettext("Description (optional)")));
    let dialog = adw::AlertDialog::builder()
        .heading(gettext("Image description"))
        .body(gettext(
            "Used as alternative text when the image cannot be displayed.",
        ))
        .extra_child(&alt)
        .default_response("insert")
        .close_response("cancel")
        .build();
    let cancel = gettext("Cancel");
    let insert = gettext("Insert");
    dialog.add_responses(&[("cancel", cancel.as_str()), ("insert", insert.as_str())]);
    let file = file.clone();
    let dispatcher = dispatcher.clone();
    let toast_overlay = toast_overlay.clone();
    let focus = focus.clone();
    dialog.connect_response(None, move |_dialog, response| {
        if response != "insert" {
            focus.restore_later();
            return;
        }
        let Some(extension) = image_extension_for_file(&file) else {
            toast_overlay.add_toast(adw::Toast::new(&gettext("Unsupported image format")));
            focus.restore_later();
            return;
        };
        import_managed_image_file(
            &file,
            alt.text().as_str(),
            &dispatcher,
            extension,
            source_target.clone(),
        );
        focus.restore_later();
    });
    dialog.present(parent);
}

/// Requests storage of a selected image through the scoped import runtime.
fn import_managed_image_file(
    file: &gtk::gio::File,
    alt: &str,
    dispatcher: &AppDispatcher,
    extension: &str,
    target: ImportTarget,
) {
    let _ = dispatcher.dispatch(AppMsg::Editor(EditorMsg::ImportFiles {
        target,
        files: vec![ImportFileSource {
            uri: file.uri().to_string(),
            label: alt.to_owned(),
            extension: extension.to_owned(),
            image: true,
        }],
    }));
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

/// Opens a chooser while retaining the initiating editor session and insertion target.
pub(crate) fn choose_managed_files(
    button: &impl IsA<gtk::Widget>,
    dispatcher: &AppDispatcher,
    target: ImportTarget,
) {
    let dialog = gtk::FileDialog::builder()
        .title(gettext("Add files"))
        .build();
    let parent = button.root().and_downcast::<gtk::Window>();
    let dispatcher = dispatcher.clone();
    dialog.open_multiple(
        parent.as_ref(),
        None::<&gtk::gio::Cancellable>,
        move |result| {
            let Ok(files) = result else {
                return;
            };
            let files: Vec<_> = files.iter::<gtk::gio::File>().flatten().collect();
            import_managed_files(&files, &dispatcher, target);
        },
    );
}

/// Dispatches an ordered batch; the runtime owns all reads and storage.
pub(crate) fn import_managed_files(
    files: &[gtk::gio::File],
    dispatcher: &AppDispatcher,
    target: ImportTarget,
) {
    let files = files
        .iter()
        .map(|file| {
            let name = file.basename().map_or_else(
                || "Attachment".into(),
                |name| name.to_string_lossy().into_owned(),
            );
            let extension = attachment_extension(&name);
            let image = image_extension_for_file(file).is_some();
            let label = if image {
                name.rsplit_once('.')
                    .map_or_else(|| name.clone(), |(stem, _)| stem.to_owned())
            } else {
                name
            };
            ImportFileSource {
                uri: file.uri().to_string(),
                label,
                extension,
                image,
            }
        })
        .collect();
    let _ = dispatcher.dispatch(AppMsg::Editor(EditorMsg::ImportFiles { target, files }));
}

fn attachment_extension(name: &str) -> String {
    name.rsplit_once('.')
        .map(|(_, extension)| extension)
        .filter(|extension| {
            !extension.is_empty()
                && extension.len() <= 32
                && extension.bytes().all(|byte| byte.is_ascii_alphanumeric())
        })
        .map_or_else(|| String::from("bin"), str::to_ascii_lowercase)
}

/// A hoverable table-size picker that reflects the current table structure.
#[derive(Clone)]
pub(crate) struct TablePicker {
    menu: gtk::MenuButton,
    dimensions: gtk::Label,
    header_row: gtk::Switch,
    cells: TablePickerCells,
    current: Rc<RefCell<Option<TableSelection>>>,
}

impl TablePicker {
    /// Returns the menu button that anchors the picker in a toolbar.
    pub(crate) fn widget(&self) -> &gtk::MenuButton {
        &self.menu
    }

    /// Reflects the table enclosing the selection, or resets to insertion defaults.
    ///
    /// Repeated calls with the same geometry are ignored so an open popover keeps
    /// the hover highlight the user is holding.
    pub(crate) fn set_table(&self, table: Option<TableSelection>) {
        let current = *self.current.borrow();
        if current == table {
            return;
        }
        self.current.replace(table);
        render_table_picker(&self.dimensions, &self.header_row, &self.cells, table);
    }
}

/// Applies a table geometry to the picker's label, header switch, and cells.
fn render_table_picker(
    dimensions: &gtk::Label,
    header_row: &gtk::Switch,
    cells: &[(u8, u8, gtk::Button)],
    table: Option<TableSelection>,
) {
    let (rows, columns, header) = table.map_or((1, 1, true), |table| {
        (table.rows, table.columns, table.header)
    });
    // A table larger than the grid cannot be expressed without dropping trailing
    // rows or columns, so keep the whole picker read-only instead of offering a
    // click that would damage the document.
    let oversized =
        rows > u32::from(TABLE_PICKER_ROWS) || columns > u32::from(TABLE_PICKER_COLUMNS);
    dimensions.set_text(&tr_fmt!(
        gettext("{rows} × {columns}"),
        rows = rows,
        columns = columns
    ));
    header_row.set_active(header);
    header_row.set_sensitive(!oversized);
    for (cell_row, cell_column, cell) in cells {
        let selected = u32::from(*cell_row) <= rows && u32::from(*cell_column) <= columns;
        if selected {
            cell.add_css_class("selected");
        } else {
            cell.remove_css_class("selected");
        }
        cell.set_sensitive(!oversized);
    }
}

/// Builds the hover grid and returns it with its indexed picker cells.
fn build_table_size_grid() -> (gtk::Grid, TablePickerCells) {
    let grid = gtk::Grid::new();
    // Keep the picker intentional at every popover width: the cells share the
    // available width instead of leaving a detached grid in the middle.
    grid.set_halign(gtk::Align::Fill);
    grid.set_hexpand(true);
    grid.set_column_homogeneous(true);
    grid.set_row_spacing(4);
    grid.set_column_spacing(4);
    let mut cells = Vec::new();
    for row in 1_u8..=TABLE_PICKER_ROWS {
        for column in 1_u8..=TABLE_PICKER_COLUMNS {
            let cell = gtk::Button::new();
            cell.add_css_class("table-size-cell");
            cell.set_hexpand(true);
            cell.set_tooltip_text(Some(&tr_fmt!(
                gettext("{rows} rows × {columns} columns"),
                rows = row,
                columns = column
            )));
            grid.attach(&cell, i32::from(column - 1), i32::from(row - 1), 1, 1);
            cells.push((row, column, cell));
        }
    }
    (grid, Rc::new(cells))
}

/// Appends the shared hoverable table-size picker used by both editing modes.
pub(crate) fn append_table_picker(
    toolbar: &gtk::Box,
    name: &str,
    on_insert: impl Fn(u8, u8, bool) + 'static,
) -> TablePicker {
    let menu = gtk::MenuButton::new();
    menu.set_widget_name(name);
    menu.set_icon_name("view-grid-symbolic");
    menu.set_tooltip_text(Some(&gettext("Insert table")));
    menu.add_css_class("flat");

    let content = gtk::Box::new(gtk::Orientation::Vertical, 12);
    content.add_css_class("table-size-picker");
    content.set_size_request(300, -1);
    content.set_margin_start(16);
    content.set_margin_end(16);
    content.set_margin_top(16);
    content.set_margin_bottom(16);
    let dimensions = gtk::Label::new(Some(&tr_fmt!(
        gettext("{rows} × {columns}"),
        rows = 1,
        columns = 1
    )));
    dimensions.set_halign(gtk::Align::Center);
    content.append(&dimensions);
    let (grid, cells) = build_table_size_grid();
    content.append(&grid);
    let header_row = gtk::Switch::new();
    header_row.set_active(true);
    let header_box = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    header_box.append(&gtk::Label::new(Some(&gettext("Header row"))));
    header_box.append(&header_row);
    content.append(&header_box);

    let popover = gtk::Popover::new();
    popover.set_child(Some(&content));
    let on_insert: Rc<dyn Fn(u8, u8, bool)> = Rc::new(on_insert);
    for (row, column, cell) in cells.iter() {
        let dimensions = dimensions.clone();
        let cells_for_motion = Rc::clone(&cells);
        let motion = gtk::EventControllerMotion::new();
        let row = row.to_owned();
        let column = column.to_owned();
        motion.connect_enter(move |_, _, _| {
            dimensions.set_text(&tr_fmt!(
                gettext("{rows} × {columns}"),
                rows = row,
                columns = column
            ));
            for (cell_row, cell_column, cell) in cells_for_motion.iter() {
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
            on_insert(row, column, header_row.is_active());
            popover.popdown();
        });
    }
    menu.set_popover(Some(&popover));
    render_table_picker(&dimensions, &header_row, &cells, None);
    let picker = TablePicker {
        menu: menu.clone(),
        dimensions,
        header_row,
        cells,
        current: Rc::new(RefCell::new(None)),
    };
    // Reset to the live geometry on close so a hover left behind by an earlier
    // interaction never masquerades as the current table when reopened.
    let current = Rc::clone(&picker.current);
    let dimensions = picker.dimensions.clone();
    let header_row = picker.header_row.clone();
    let cells = Rc::clone(&picker.cells);
    popover.connect_closed(move |_| {
        // Copy the geometry out first: the temporary `Ref` from `borrow()` would
        // otherwise stay live across the GTK setters below, where a re-entrant
        // property notification could try to borrow the picker again.
        let table = *current.borrow();
        render_table_picker(&dimensions, &header_row, &cells, table);
    });
    toolbar.append(&menu);
    picker
}

pub(crate) fn show_source_link_dialog(
    _anchor: &impl IsA<gtk::Widget>,
    buffer: &gtk::TextBuffer,
    dispatcher: &AppDispatcher,
    _focus: &EditorFocusRestorer,
) {
    let selection = source_commands::selection_from_buffer(buffer);
    let text = buffer
        .selection_bounds()
        .map_or_else(String::new, |(start, end)| {
            buffer.text(&start, &end, false).to_string()
        });
    let _ = dispatcher.dispatch(AppMsg::Editor(EditorMsg::LinkDialogRequested {
        origin: crate::mvu::LinkDialogOrigin::Source { selection, text },
    }));
}

#[cfg(test)]
pub(crate) mod tests;

/// Presents the shared table grid independently of toolbar visibility.
pub(crate) fn show_table_picker(
    anchor: &gtk::Widget,
    on_insert: impl Fn(u8, u8, bool) + 'static,
    focus: &EditorFocusRestorer,
) {
    let dialog = adw::Dialog::builder()
        .title(gettext("Insert table"))
        .content_width(380)
        .build();
    dialog.set_widget_name("palette-table-dialog");
    let weak_dialog = dialog.downgrade();
    let holder = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let picker = append_table_picker(
        &holder,
        "palette-table-picker",
        move |rows, columns, header| {
            if let Some(dialog) = weak_dialog.upgrade() {
                dialog.force_close();
            }
            on_insert(rows, columns, header);
        },
    );
    if let Some(popover) = picker.widget().popover()
        && let Some(content) = popover.child()
    {
        popover.set_child(None::<&gtk::Widget>);
        let view = adw::ToolbarView::new();
        view.add_top_bar(&adw::HeaderBar::new());
        view.set_content(Some(&content));
        dialog.set_child(Some(&view));
    }
    let focus = focus.clone();
    dialog.connect_closed(move |_| focus.restore_later());
    dialog.present(anchor.root().as_ref());
}
