use super::*;
use crate::ui::tests::{
    support::{TestResult, run_main_context_until, widget_as},
    ui::{document_sidebar, interactions::find_alert},
};
use carver_editor_protocol::TableSelection;

pub(crate) fn image_description_should_import_only_after_confirmation() -> TestResult {
    let fixture = document_sidebar::fixture()?;
    let category = fixture.client.create_category("Images")?;
    let note = fixture.client.create_note(category.id)?;
    fixture.runtime.dispatch(AppMsg::Editor(EditorMsg::Load {
        note_id: note.id,
        revision: note.revision,
        source: String::new(),
    }));
    let source =
        widget_as::<sourceview5::View>(&fixture.surface, "source-editor").ok_or("source")?;
    let rich = widget_as::<webkit6::WebView>(&fixture.surface, "rich-editor").ok_or("rich")?;
    let focus = EditorFocusRestorer::new(
        Rc::new(std::cell::Cell::new(carver_config::EditorMode::Source)),
        source.upcast_ref(),
        &rich,
    );
    let dispatcher = AppDispatcher::default();
    fixture.runtime.bind_dispatcher(&dispatcher);
    let target = ImportTarget {
        session: fixture.runtime.model().editor.ok_or("editor")?.session,
        source: Some(source_commands::image_target_from_buffer(&source.buffer())),
    };
    let file_path = fixture.directory.path().join("local-image.png");
    let bytes = glib::Bytes::from_owned(vec![255_u8; 4]);
    let texture = gtk::gdk::MemoryTexture::new(1, 1, gtk::gdk::MemoryFormat::R8g8b8a8, &bytes, 4);
    std::fs::write(&file_path, texture.save_to_png_bytes())?;
    let file = gtk::gio::File::for_path(&file_path);
    let overlay = adw::ToastOverlay::new();
    show_image_alt_dialog(
        &file,
        "Diagram",
        &dispatcher,
        &overlay,
        Some(fixture.window.upcast_ref()),
        target.clone(),
        &focus,
    );
    let dialog = find_alert(fixture.window.upcast_ref()).ok_or("description dialog")?;
    dialog.emit_by_name::<()>("response", &[&"cancel"]);
    dialog.force_close();
    assert_eq!(fixture.runtime.model().editor.ok_or("editor")?.source, "");
    show_image_alt_dialog(
        &file,
        "Diagram",
        &dispatcher,
        &overlay,
        Some(fixture.window.upcast_ref()),
        target,
        &focus,
    );
    let dialog = find_alert(fixture.window.upcast_ref()).ok_or("description dialog")?;
    dialog.emit_by_name::<()>("response", &[&"insert"]);
    assert!(run_main_context_until(|| fixture
        .runtime
        .model()
        .editor
        .is_some_and(|doc| doc.source.contains("![Diagram](assets/"))));
    let document = fixture.runtime.model().editor.ok_or("editor")?;
    assert!(
        !document
            .source
            .contains(file_path.to_string_lossy().as_ref())
    );
    let paths = carver_export::managed_asset_paths(&document.source);
    assert_eq!(paths.len(), 1);
    assert!(
        fixture
            .client
            .note_asset_bytes(note.id, &paths[0])?
            .is_some()
    );
    fixture.window.close();
    Ok(())
}

fn picker_cell(picker: &TablePicker, row: u8, column: u8) -> Option<gtk::Button> {
    picker
        .cells
        .iter()
        .find(|(cell_row, cell_column, _)| *cell_row == row && *cell_column == column)
        .map(|(_, _, cell)| cell.clone())
}

fn cell_is_selected(picker: &TablePicker, row: u8, column: u8) -> bool {
    picker_cell(picker, row, column).is_some_and(|cell| cell.has_css_class("selected"))
}

fn cell_is_sensitive(picker: &TablePicker, row: u8, column: u8) -> bool {
    picker_cell(picker, row, column).is_some_and(|cell| cell.is_sensitive())
}

/// The picker must mirror the live table and drop a stale hover when reopened.
pub(crate) fn table_picker_should_reflect_live_table_and_reset() {
    let toolbar = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    let picker = append_table_picker(&toolbar, "table-picker-test", |_, _, _| {});
    assert_eq!(picker.widget().widget_name(), "table-picker-test");
    assert_eq!(picker.dimensions.text(), "1 × 1");
    assert!(picker.header_row.is_active());
    assert!(cell_is_selected(&picker, 1, 1));
    assert!(!cell_is_selected(&picker, 2, 1));

    picker.set_table(Some(TableSelection {
        rows: 3,
        columns: 4,
        header: false,
    }));
    assert_eq!(picker.dimensions.text(), "3 × 4");
    assert!(!picker.header_row.is_active());
    assert!(cell_is_selected(&picker, 1, 1));
    assert!(cell_is_selected(&picker, 3, 4));
    assert!(!cell_is_selected(&picker, 4, 4));
    assert!(!cell_is_selected(&picker, 3, 5));

    // Simulate a hover left behind after the popover closed; closing must
    // restore the live table instead of presenting the stale highlight.
    for (_, _, cell) in picker.cells.iter() {
        cell.remove_css_class("selected");
    }
    assert!(!cell_is_selected(&picker, 3, 4));
    if let Some(popover) = picker.widget().popover() {
        popover.emit_by_name::<()>("closed", &[]);
    }
    assert!(cell_is_selected(&picker, 3, 4));

    picker.set_table(None);
    assert_eq!(picker.dimensions.text(), "1 × 1");
    assert!(picker.header_row.is_active());
    assert!(cell_is_selected(&picker, 1, 1));
    assert!(!cell_is_selected(&picker, 2, 1));

    // A table larger than the grid cannot be resized without dropping content,
    // so the picker must refuse the action entirely.
    picker.set_table(Some(TableSelection {
        rows: 5,
        columns: 7,
        header: true,
    }));
    assert_eq!(picker.dimensions.text(), "5 × 7");
    assert!(picker.header_row.is_active());
    assert!(!picker.header_row.is_sensitive());
    assert!(!cell_is_sensitive(&picker, 1, 1));
    assert!(cell_is_selected(&picker, 4, 6));

    // Returning within bounds restores the resize action.
    picker.set_table(Some(TableSelection {
        rows: 2,
        columns: 3,
        header: true,
    }));
    assert!(picker.header_row.is_sensitive());
    assert!(cell_is_sensitive(&picker, 1, 1));
}
