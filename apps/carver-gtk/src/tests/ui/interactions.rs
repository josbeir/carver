//! Native link dialogs and image paste exercise the host/reducer boundary.
use super::*;
use crate::mvu::{AppMsg, EditorMsg};

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
    assert!(run_main_context_until(|| find_link_dialog(
        fixture.window.upcast_ref()
    )
    .is_some()));
    let dialog = find_link_dialog(fixture.window.upcast_ref()).ok_or("link dialog")?;
    assert_eq!(
        widget_as::<adw::EntryRow>(dialog.upcast_ref(), "link-dialog-text")
            .ok_or("text")?
            .text()
            .to_string(),
        "Carver"
    );
    widget_as::<adw::EntryRow>(dialog.upcast_ref(), "link-dialog-address")
        .ok_or("address")?
        .set_text("https://example.com");
    buffer.place_cursor(&buffer.end_iter());
    widget_as::<gtk::Button>(dialog.upcast_ref(), "link-dialog-insert")
        .ok_or("insert")?
        .emit_clicked();
    assert!(run_main_context_until(|| fixture
        .runtime
        .model()
        .editor
        .is_some_and(
            |doc| doc.source == "Read [Carver](https://example.com) now"
        )));
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
    widget_as::<adw::ViewStack>(&fixture.surface, "editor-mode-stack")
        .ok_or("editor mode stack")?
        .set_visible_child_name("rich");
    let rich = widget_as::<webkit6::WebView>(&fixture.surface, "rich-editor").ok_or("rich")?;
    assert_web_script_should_be_true(
        &rich,
        "Boolean(window.carverEditor && document.querySelector('.tiptap'))",
    );
    widget_as::<gtk::Button>(&fixture.surface, "format-link-button")
        .ok_or("link")?
        .emit_clicked();
    assert!(run_main_context_until(|| find_link_dialog(
        fixture.window.upcast_ref()
    )
    .is_some()));
    let dialog = find_link_dialog(fixture.window.upcast_ref()).ok_or("dialog")?;
    widget_as::<adw::EntryRow>(dialog.upcast_ref(), "link-dialog-text")
        .ok_or("text")?
        .set_text("Carver");
    widget_as::<adw::EntryRow>(dialog.upcast_ref(), "link-dialog-address")
        .ok_or("address")?
        .set_text("https://example.com");
    widget_as::<gtk::Button>(dialog.upcast_ref(), "link-dialog-insert")
        .ok_or("insert")?
        .emit_clicked();
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
        .find(|controller| controller.name().as_deref() == Some("source-smart-paste"))
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

pub(super) fn source_smart_paste_should_preserve_markdown_delimiters() -> TestResult {
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
    let clipboard = source.clipboard();
    clipboard.set_text("**bold**");
    let controllers = source.observe_controllers();
    let paste = controllers
        .iter::<glib::Object>()
        .flatten()
        .filter_map(|controller| controller.downcast::<gtk::EventControllerKey>().ok())
        .find(|controller| controller.name().as_deref() == Some("source-smart-paste"))
        .ok_or("smart paste controller")?;
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
        .is_some_and(|doc| doc.source.contains("**bold**"))));
    clipboard.set_content(None::<&gtk::gdk::ContentProvider>)?;
    fixture.window.close();
    Ok(())
}

pub(super) fn rich_changes_should_be_ignored_while_another_mode_is_active() -> TestResult {
    let fixture = document_sidebar::fixture()?;
    fixture.runtime.dispatch(AppMsg::Editor(EditorMsg::Load {
        note_id: carver_sdk::NoteId::new(),
        revision: carver_sdk::Revision(1),
        source: "# Current".into(),
    }));
    widget_as::<adw::ViewStack>(&fixture.surface, "editor-mode-stack")
        .ok_or("editor mode stack")?
        .set_visible_child_name("rich");
    let rich = widget_as::<webkit6::WebView>(&fixture.surface, "rich-editor").ok_or("rich")?;
    assert_web_script_should_be_true(
        &rich,
        "Boolean(window.carverEditor && document.querySelector('.tiptap'))",
    );
    // The active rich projection may commit a source change.
    assert_web_script_should_be_true(
        &rich,
        r"(() => {
        window.webkit.messageHandlers.carver.postMessage(JSON.stringify({type:'changed', session:2, revision:1, source:'# From rich'}));
        return true;
    })()",
    );
    assert!(run_main_context_until(|| fixture
        .runtime
        .model()
        .editor
        .is_some_and(|doc| doc.source.contains("From rich"))));

    widget_as::<adw::ViewStack>(&fixture.surface, "editor-mode-stack")
        .ok_or("editor mode stack")?
        .set_visible_child_name("source");
    assert!(run_main_context_until(|| !rich.is_mapped()));
    // A late rich change must not overwrite the now-active source projection.
    assert_web_script_should_be_true(
        &rich,
        r"(() => {
        window.webkit.messageHandlers.carver.postMessage(JSON.stringify({type:'changed', session:2, revision:2, source:'# Stale'}));
        return true;
    })()",
    );
    assert_web_script_should_be_true(&rich, "true");
    let source = fixture.runtime.model().editor.ok_or("editor")?.source;
    assert!(source.contains("From rich"));
    assert!(!source.contains("Stale"));
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

/// Finds the unified link dialog by its widget name across the window and its toplevels.
pub(crate) fn find_link_dialog(root: &gtk::Widget) -> Option<adw::Dialog> {
    link_dialog_descendant(root).or_else(|| {
        gtk::Window::list_toplevels()
            .iter()
            .find_map(link_dialog_descendant)
    })
}

fn link_dialog_descendant(root: &gtk::Widget) -> Option<adw::Dialog> {
    if let Some(dialog) = root.downcast_ref::<adw::Dialog>()
        && dialog.widget_name() == "link-dialog"
    {
        return Some(dialog.clone());
    }
    let mut child = root.first_child();
    while let Some(widget) = child {
        if let Some(dialog) = link_dialog_descendant(&widget) {
            return Some(dialog);
        }
        child = widget.next_sibling();
    }
    None
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
    assert!(run_main_context_until(|| find_link_dialog(
        fixture.window.upcast_ref()
    )
    .is_some()));
    let dialog = find_link_dialog(fixture.window.upcast_ref()).ok_or("dialog")?;
    widget_as::<adw::EntryRow>(dialog.upcast_ref(), "link-dialog-text")
        .ok_or("text")?
        .set_text("Replacement");
    widget_as::<adw::EntryRow>(dialog.upcast_ref(), "link-dialog-address")
        .ok_or("address")?
        .set_text("https://example.com");
    widget_as::<gtk::Button>(dialog.upcast_ref(), "link-dialog-cancel")
        .ok_or("cancel")?
        .emit_clicked();
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
    widget_as::<adw::ViewStack>(&fixture.surface, "editor-mode-stack")
        .ok_or("editor mode stack")?
        .set_visible_child_name("rich");
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
