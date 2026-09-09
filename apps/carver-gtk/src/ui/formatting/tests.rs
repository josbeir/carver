use super::capture_selection;

pub(crate) fn captured_source_selection_should_delete_marks_after_reading_offsets() {
    let buffer = gtk::TextBuffer::new(None);
    buffer.set_text("Carver");
    let start = buffer.iter_at_offset(1);
    let end = buffer.iter_at_offset(4);
    buffer.select_range(&start, &end);
    let Some(marks) = capture_selection(&buffer) else {
        panic!("the selected source range should create marks");
    };
    let start_mark = marks.start.clone();
    let end_mark = marks.end.clone();

    assert_eq!(marks.into_range(&buffer), 1..4);
    assert!(start_mark.is_deleted());
    assert!(end_mark.is_deleted());
}

use super::*;
use crate::ui::tests::{
    support::{TestResult, run_main_context_until, widget_as},
    ui::{document_sidebar, interactions::find_alert},
};

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
        Some(&fixture.window),
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
        Some(&fixture.window),
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
