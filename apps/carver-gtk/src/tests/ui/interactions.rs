//! Native link dialogs and image paste exercise the host/reducer boundary.
use super::*;
use crate::mvu::{AppMsg, EditorMsg};

fn entry(root: &gtk::Widget, placeholder: &str) -> Option<gtk::Entry> {
    if let Some(entry) = root.downcast_ref::<gtk::Entry>()
        && entry.placeholder_text().as_deref() == Some(placeholder)
    {
        return Some(entry.clone());
    }
    let mut child = root.first_child();
    while let Some(widget) = child {
        if let Some(entry) = entry(&widget, placeholder) {
            return Some(entry);
        }
        child = widget.next_sibling();
    }
    None
}

pub(super) fn source_link_should_keep_the_captured_selection() -> TestResult {
    let fixture = document_sidebar::fixture()?;
    fixture.runtime.dispatch(AppMsg::Editor(EditorMsg::Load {
        note_id: carver_sdk::NoteId::new(),
        revision: carver_sdk::Revision(1),
        source: "Read Carver now".into(),
    }));
    let source =
        widget_as::<sourceview5::View>(&fixture.surface, "source-editor").ok_or("source")?;
    let buffer = source.buffer();
    buffer.select_range(&buffer.iter_at_offset(5), &buffer.iter_at_offset(11));
    let link = widget_as::<gtk::Button>(&fixture.surface, "format-link-button").ok_or("link")?;
    link.emit_clicked();
    assert!(run_main_context_until(|| find_alert(
        fixture.window.upcast_ref()
    )
    .is_some()));
    let dialog = find_alert(fixture.window.upcast_ref()).ok_or("link dialog")?;
    assert_eq!(
        entry(dialog.upcast_ref(), "Link text")
            .ok_or("text")?
            .text(),
        "Carver"
    );
    entry(dialog.upcast_ref(), "https://example.com")
        .ok_or("url")?
        .set_text("https://example.com");
    buffer.place_cursor(&buffer.end_iter());
    dialog.emit_by_name::<()>("response", &[&"insert"]);
    dialog.force_close();
    assert_eq!(
        fixture.runtime.model().editor.ok_or("editor")?.source,
        "Read [Carver](https://example.com) now"
    );
    fixture.window.close();
    Ok(())
}

pub(super) fn rich_link_should_update_canonical_source() -> TestResult {
    let fixture = document_sidebar::fixture()?;
    fixture.runtime.dispatch(AppMsg::Editor(EditorMsg::Load {
        note_id: carver_sdk::NoteId::new(),
        revision: carver_sdk::Revision(1),
        source: "Read".into(),
    }));
    widget_as::<gtk::ToggleButton>(&fixture.surface, "editor-mode-rich")
        .ok_or("rich mode")?
        .set_active(true);
    let rich = widget_as::<webkit6::WebView>(&fixture.surface, "rich-editor").ok_or("rich")?;
    assert_web_script_should_be_true(
        &rich,
        "Boolean(window.carverEditor && document.querySelector('.tiptap'))",
    );
    widget_as::<gtk::Button>(&fixture.surface, "format-link-button")
        .ok_or("link")?
        .emit_clicked();
    assert!(run_main_context_until(|| find_alert(
        fixture.window.upcast_ref()
    )
    .is_some()));
    let dialog = find_alert(fixture.window.upcast_ref()).ok_or("dialog")?;
    entry(dialog.upcast_ref(), "Link text")
        .ok_or("text")?
        .set_text("Carver");
    entry(dialog.upcast_ref(), "https://example.com")
        .ok_or("url")?
        .set_text("https://example.com");
    dialog.emit_by_name::<()>("response", &[&"insert"]);
    dialog.force_close();
    assert!(run_main_context_until(|| fixture
        .runtime
        .model()
        .editor
        .is_some_and(|doc| doc
            .source
            .contains("[Carver](https://example.com)"))));
    fixture.window.close();
    Ok(())
}

pub(super) fn source_image_paste_should_store_a_managed_asset() -> TestResult {
    let fixture = document_sidebar::fixture()?;
    let category = fixture.client.create_category("Paste")?;
    let note = fixture.client.create_note(category.id)?;
    fixture.runtime.dispatch(AppMsg::Editor(EditorMsg::Load {
        note_id: note.id,
        revision: note.revision,
        source: String::new(),
    }));
    let source =
        widget_as::<sourceview5::View>(&fixture.surface, "source-editor").ok_or("source")?;
    let bytes = glib::Bytes::from_owned(vec![255_u8, 0, 0, 255]);
    let texture = gtk::gdk::MemoryTexture::new(1, 1, gtk::gdk::MemoryFormat::R8g8b8a8, &bytes, 4);
    let clipboard = source.clipboard();
    clipboard.set_texture(&texture);
    let controllers = source.observe_controllers();
    let paste = controllers
        .iter::<glib::Object>()
        .flatten()
        .filter_map(|controller| controller.downcast::<gtk::EventControllerKey>().ok())
        .find(|controller| controller.name().as_deref() == Some("source-image-paste"))
        .ok_or("image paste controller")?;
    paste.emit_by_name::<bool>(
        "key-pressed",
        &[
            &gtk::gdk::Key::v,
            &0_u32,
            &gtk::gdk::ModifierType::CONTROL_MASK,
        ],
    );
    assert!(run_main_context_until(|| fixture
        .runtime
        .model()
        .editor
        .is_some_and(|doc| doc.source.contains("assets/"))));
    let document = fixture.runtime.model().editor.ok_or("document")?;
    let paths = carver_export::managed_asset_paths(&document.source);
    assert_eq!(paths.len(), 1);
    assert!(
        fixture
            .client
            .note_asset_bytes(note.id, &paths[0])?
            .is_some()
    );
    clipboard.set_content(None::<&gtk::gdk::ContentProvider>)?;
    fixture.window.close();
    Ok(())
}

pub(crate) fn find_alert(root: &gtk::Widget) -> Option<adw::AlertDialog> {
    alert_descendant(root).or_else(|| {
        gtk::Window::list_toplevels()
            .iter()
            .find_map(alert_descendant)
    })
}

fn alert_descendant(root: &gtk::Widget) -> Option<adw::AlertDialog> {
    if let Some(dialog) = root.downcast_ref::<adw::AlertDialog>() {
        return Some(dialog.clone());
    }
    let mut child = root.first_child();
    while let Some(widget) = child {
        if let Some(dialog) = alert_descendant(&widget) {
            return Some(dialog);
        }
        child = widget.next_sibling();
    }
    None
}

pub(super) fn cancelled_source_link_should_leave_the_document_unchanged() -> TestResult {
    let fixture = document_sidebar::fixture()?;
    fixture.runtime.dispatch(AppMsg::Editor(EditorMsg::Load {
        note_id: carver_sdk::NoteId::new(),
        revision: carver_sdk::Revision(1),
        source: "Keep this".into(),
    }));
    widget_as::<gtk::Button>(&fixture.surface, "format-link-button")
        .ok_or("link")?
        .emit_clicked();
    assert!(run_main_context_until(|| find_alert(
        fixture.window.upcast_ref()
    )
    .is_some()));
    let dialog = find_alert(fixture.window.upcast_ref()).ok_or("dialog")?;
    entry(dialog.upcast_ref(), "Link text")
        .ok_or("text")?
        .set_text("Replacement");
    entry(dialog.upcast_ref(), "https://example.com")
        .ok_or("url")?
        .set_text("https://example.com");
    dialog.emit_by_name::<()>("response", &[&"cancel"]);
    dialog.force_close();
    assert_eq!(
        fixture.runtime.model().editor.ok_or("editor")?.source,
        "Keep this"
    );
    fixture.window.close();
    Ok(())
}

pub(super) fn stale_web_messages_should_not_change_the_active_document() -> TestResult {
    let fixture = document_sidebar::fixture()?;
    fixture.runtime.dispatch(AppMsg::Editor(EditorMsg::Load {
        note_id: carver_sdk::NoteId::new(),
        revision: carver_sdk::Revision(1),
        source: "# Current".into(),
    }));
    widget_as::<gtk::ToggleButton>(&fixture.surface, "editor-mode-rich")
        .ok_or("rich mode")?
        .set_active(true);
    let rich = widget_as::<webkit6::WebView>(&fixture.surface, "rich-editor").ok_or("rich")?;
    assert_web_script_should_be_true(
        &rich,
        "Boolean(window.carverEditor && document.querySelector('.tiptap'))",
    );
    assert_web_script_should_be_true(
        &rich,
        r"(() => {
        const post = value => window.webkit.messageHandlers.carver.postMessage(value);
        post('not JSON');
        post(JSON.stringify({type:'changed', session:999999, revision:1, source:'Wrong note'}));
        post(JSON.stringify({type:'unsupported', session:999999, unsupported:['raw'], degraded:[]}));
        post(JSON.stringify({type:'paste-image', session:999999, mime_type:'image/png', data:'invalid'}));
        post(JSON.stringify({type:'copy-selection', session:999999, source:'Wrong copy'}));
        return true;
    })()",
    );
    // A second round trip ensures the preceding bridge messages have been delivered.
    assert_web_script_should_be_true(
        &rich,
        "document.querySelector('.tiptap').textContent.includes('Current')",
    );
    let model = fixture.runtime.model();
    assert_eq!(model.editor.ok_or("editor")?.source, "# Current");
    assert!(model.editor_copy_request.is_none());
    assert_eq!(
        model.preferences.editor_mode,
        carver_config::EditorMode::Rich
    );
    fixture.window.close();
    Ok(())
}
