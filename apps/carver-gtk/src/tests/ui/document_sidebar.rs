//! Display-backed document navigation and outline presentation.
use super::*;
use crate::mvu::{AppDispatcher, AppModel, AppMsg, AppRuntime, EditorMsg, PreferencesMsg};
use libadwaita::prelude::*;

pub(crate) struct SidebarFixture {
    pub directory: tempfile::TempDir,
    pub client: super::super::support::TestLibraryClient,
    pub surface: gtk::Widget,
    pub runtime: AppRuntime<carver_storage_sqlite::SqliteLibrary>,
    pub window: adw::Window,
    pub config_path: std::path::PathBuf,
    pub dispatcher: AppDispatcher,
}

pub(crate) fn fixture() -> Result<SidebarFixture, Box<dyn std::error::Error>> {
    let (directory, client) = test_state()?;
    let syntax = crate::ui::editor::install_syntax_assets(directory.path())?;
    let config_path = directory.path().join("document-sidebar-settings.toml");
    let mut config = Config::default();
    config.editor.show_document_sidebar = true;
    config.editor.last_mode = carver_config::EditorMode::Source;
    carver_config::save(&config_path, &config)?;
    let config = carver_config::load(&config_path)?;
    let dispatcher = AppDispatcher::default();
    let overlay = adw::ToastOverlay::new();
    let editor = crate::ui::editor::build_editor(
        &dispatcher,
        &config,
        None,
        &syntax,
        &overlay,
        &adw::NavigationSplitView::new(),
        &Rc::new(Cell::new(false)),
    )?;
    let (surface, refs) = editor.into_parts();
    let stack = gtk::Stack::new();
    stack.add_named(
        &gtk::Box::new(gtk::Orientation::Vertical, 0),
        Some("browser"),
    );
    stack.add_named(&surface, Some("editor"));
    let window = adw::Window::builder()
        .default_width(1200)
        .default_height(800)
        .build();
    window.set_content(Some(&stack));
    let runtime = AppRuntime::new_with_config_path(
        client.clone(),
        AppModel::new(&config),
        crate::view::ViewRefs::new(stack, adw::StatusPage::new(), adw::StatusPage::new())
            .with_editor(refs)
            .with_dispatcher(dispatcher.clone()),
        Some(config_path.clone()),
    );
    runtime.bind_dispatcher(&dispatcher);
    window.present();
    Ok(SidebarFixture {
        directory,
        client,
        surface,
        runtime,
        window,
        config_path,
        dispatcher,
    })
}

pub(super) fn webkit_views_should_disable_smooth_scrolling() -> TestResult {
    let fixture = fixture()?;

    for name in [
        "rich-editor",
        "source-split-preview",
        "editor-rendered-preview",
    ] {
        let view = widget_as::<webkit6::WebView>(&fixture.surface, name).ok_or(name)?;
        let settings = webkit6::prelude::WebViewExt::settings(&view).ok_or("WebKit settings")?;
        assert!(
            !settings.enables_smooth_scrolling(),
            "{name} should disable smooth scrolling"
        );
    }

    Ok(())
}

pub(super) fn media_sidebar_should_show_file_details_in_an_isolated_editor() -> TestResult {
    let fixture = fixture()?;
    let category = fixture.client.create_category("Media")?;
    let note = fixture.client.create_note(category.id)?;
    fixture.runtime.dispatch(AppMsg::Editor(EditorMsg::Load {
        note_id: note.id,
        revision: note.revision,
        source: String::new(),
    }));
    let source = widget_as::<gtk::TextView>(&fixture.surface, "source-editor").ok_or("source")?;
    let source_mode = widget_as::<gtk::ToggleButton>(&fixture.surface, "editor-mode-source")
        .ok_or("source mode")?;
    let rich_mode =
        widget_as::<gtk::ToggleButton>(&fixture.surface, "editor-mode-rich").ok_or("rich mode")?;
    let rich = widget_as::<webkit6::WebView>(&fixture.surface, "rich-editor").ok_or("rich")?;
    assert_document_sidebar_should_focus_and_show_file_details(
        &fixture.surface,
        &source,
        &source_mode,
        &rich_mode,
        &rich,
        &fixture.client,
        note.id,
    )?;
    fixture.window.close();
    Ok(())
}

pub(super) fn heading_navigation_should_preserve_content_and_focus() -> TestResult {
    let fixture = fixture()?;
    let root = &fixture.surface;
    let source_text = "# First\n\n## Same\n\nBody\n\n## Same";
    let category = fixture.client.create_category("Outline")?;
    let created = fixture.client.create_note(category.id)?;
    let saved = fixture
        .client
        .save_note(created.id, created.revision, source_text)?;
    fixture.runtime.dispatch(AppMsg::Editor(EditorMsg::Load {
        note_id: saved.id,
        revision: saved.revision,
        source: source_text.into(),
    }));
    let outline = widget_as::<gtk::ListView>(root, "editor-outline-list").ok_or("outline")?;
    assert!(outline.has_css_class("document-outline"));
    assert!(!outline.has_css_class("boxed-list"));
    let toggle =
        widget_as::<gtk::ToggleButton>(root, "editor-document-sidebar-toggle").ok_or("toggle")?;
    assert_eq!(toggle.icon_name().as_deref(), Some("view-list-symbolic"));
    assert_eq!(
        toggle.tooltip_text().as_deref(),
        Some("Hide document sidebar")
    );
    let source = widget_as::<sourceview5::View>(root, "source-editor").ok_or("source")?;
    let expander = widget_as::<gtk::TreeExpander>(root, "editor-outline-item");
    assert!(run_main_context_until(|| widget_as::<gtk::TreeExpander>(
        root,
        "editor-outline-item"
    )
    .is_some()));
    if let Some(expander) = expander {
        assert!(
            expander
                .child()
                .and_then(|content| content.last_child())
                .is_some_and(|label| label.has_css_class("outline-heading"))
        );
    }
    outline.emit_by_name::<()>("activate", &[&2_u32]);
    assert!(
        run_main_context_until(|| widget_is_window_focus(source.upcast_ref())),
        "source root focus: {:?}",
        source
            .root()
            .and_downcast::<gtk::Window>()
            .and_then(|window| gtk::prelude::RootExt::focus(&window))
            .map(|widget| widget.widget_name())
    );
    assert_eq!(source.buffer().cursor_position(), 27);
    assert!(source.buffer().selection_bounds().is_none());
    source
        .buffer()
        .place_cursor(&source.buffer().iter_at_offset(12));
    assert!(run_main_context_until(
        || selected_position(&outline) == Some(1)
    ));
    assert!(widget_is_window_focus(source.upcast_ref()));
    source
        .buffer()
        .place_cursor(&source.buffer().iter_at_offset(19));
    assert!(run_main_context_until(
        || selected_position(&outline).is_none()
    ));
    toggle.set_active(false);
    source
        .buffer()
        .place_cursor(&source.buffer().iter_at_offset(2));
    assert!(run_main_context_until(
        || selected_position(&outline) == Some(0)
    ));
    assert!(!toggle.is_active());
    assert_eq!(
        toggle.tooltip_text().as_deref(),
        Some("Show document sidebar")
    );
    toggle.set_active(true);
    check_tree_should_remain_expanded(&outline, &source)?;
    check_rich_navigation(root, &outline)?;
    assert_rich_focus_should_not_escape_mode_switch(root, &outline)?;
    check_preview_navigation(root, &outline, &fixture.runtime)?;
    let reopened = fixture.client.note(saved.id)?.ok_or("persisted note")?;
    assert_eq!(reopened.source, saved.source);
    assert_eq!(reopened.revision, saved.revision);
    assert_eq!(reopened.updated_at, saved.updated_at);
    let model = fixture.runtime.model();
    let document = model.editor.as_ref().ok_or("document")?;
    assert_eq!(document.source, source_text);
    assert_eq!(document.save_state, crate::mvu::EditorSaveState::Clean);
    drop(model);
    check_raw_preview_provenance(&fixture)?;
    fixture.window.close();
    Ok(())
}

fn selected_position(list: &gtk::ListView) -> Option<u32> {
    list.model()
        .and_downcast::<gtk::SingleSelection>()
        .map(|selection| selection.selected())
        .filter(|position| *position != gtk::INVALID_LIST_POSITION)
}

fn check_raw_preview_provenance(fixture: &SidebarFixture) -> TestResult {
    let category = fixture.client.create_category("Raw preview")?;
    let note = fixture.client.create_note(category.id)?;
    let root = &fixture.surface;
    let outline = widget_as::<gtk::ListView>(root, "editor-outline-list").ok_or("outline")?;
    fixture.runtime.dispatch(AppMsg::Editor(EditorMsg::Load {
        note_id: note.id,
        revision: note.revision,
        source: "```=html\n<h2 data-source-line=\"1\" data-carver-heading=\"forged\">Raw</h2>\n```\n\n# Real\n\n> ## Nested".into(),
    }));
    for (mode, name) in [
        (
            carver_config::EditorMode::Rendered,
            "editor-rendered-preview",
        ),
        (carver_config::EditorMode::Source, "source-split-preview"),
    ] {
        fixture
            .runtime
            .dispatch(AppMsg::Preferences(PreferencesMsg::SetEditorMode(mode)));
        let preview = widget_as::<webkit6::WebView>(root, name).ok_or("preview")?;
        assert_web_script_should_be_true(
            &preview,
            "document.querySelectorAll('h1,h2').length === 3 && document.querySelector('h1').textContent === 'Real'",
        );
        outline.emit_by_name::<()>("activate", &[&0_u32]);
        if mode == carver_config::EditorMode::Rendered {
            assert_web_script_should_be_true(
                &preview,
                "document.activeElement.textContent === 'Real'",
            );
        }
        assert_web_script_should_be_true(
            &preview,
            "(() => { document.querySelector('blockquote h2').click(); return true; })()",
        );
        assert!(run_main_context_until(
            || selected_position(&outline) == Some(1)
        ));
        assert_web_script_should_be_true(
            &preview,
            "(() => { document.querySelector('h2').click(); return true; })()",
        );
        assert!(run_main_context_until(
            || selected_position(&outline).is_none()
        ));
    }
    Ok(())
}

fn check_rich_navigation(root: &gtk::Widget, outline: &gtk::ListView) -> TestResult {
    let mode = widget_as::<gtk::ToggleButton>(root, "editor-mode-rich").ok_or("rich mode")?;
    mode.set_active(true);
    let rich = widget_as::<webkit6::WebView>(root, "rich-editor").ok_or("rich editor")?;
    assert_web_script_should_be_true(
        &rich,
        "document.querySelectorAll('.tiptap h1,.tiptap h2').length === 3",
    );
    assert_navigation_should_ignore_queued_selection(&rich, outline)?;
    assert_hover_should_preserve_clicked_heading(&rich, outline)?;
    outline.emit_by_name::<()>("activate", &[&2_u32]);
    assert_web_script_should_be_true(
        &rich,
        "(() => { const e=window.carverEditor.editor; const p=[]; e.state.doc.descendants((n,i)=>{ if(n.type.name==='heading') p.push(i+1); }); return e.state.selection.from === p[2]; })()",
    );
    assert_web_script_should_be_true(
        &rich,
        "(() => { const e=window.carverEditor.editor; const p=[]; e.state.doc.descendants((n,i)=>{ if(n.type.name==='heading') p.push(i+1); }); return e.commands.setTextSelection(p[1]); })()",
    );
    assert!(run_main_context_until(
        || selected_position(outline) == Some(1)
    ));
    assert!(widget_is_window_focus(rich.upcast_ref()));
    assert_web_script_should_be_true(
        &rich,
        "(() => { const e=window.carverEditor.editor; let p=0; e.state.doc.descendants((n,i)=>{ if(n.type.name==='paragraph' && n.textContent==='Body') p=i+1; }); return e.commands.setTextSelection(p); })()",
    );
    assert!(run_main_context_until(
        || selected_position(outline).is_none()
    ));
    Ok(())
}

fn check_preview_navigation(
    root: &gtk::Widget,
    outline: &gtk::ListView,
    runtime: &AppRuntime<carver_storage_sqlite::SqliteLibrary>,
) -> TestResult {
    for (mode, name) in [
        (
            carver_config::EditorMode::Rendered,
            "editor-rendered-preview",
        ),
        (carver_config::EditorMode::Source, "source-split-preview"),
    ] {
        runtime.dispatch(AppMsg::Preferences(PreferencesMsg::SetEditorMode(mode)));
        if mode == carver_config::EditorMode::Source {
            let split = widget_as::<gtk::ToggleButton>(root, "source-split-toggle")
                .ok_or("split toggle")?;
            split.set_active(true);
        }
        let preview = widget_as::<webkit6::WebView>(root, name).ok_or("preview")?;
        assert_web_script_should_be_true(
            &preview,
            "document.querySelectorAll('h1,h2').length === 3 && !!window.carverDocumentNavigation",
        );
        // Simulate a request before the new page has installed its bridge.
        assert_web_script_should_be_true(
            &preview,
            "(() => { window.savedNavigation = window.carverDocumentNavigation; delete window.carverDocumentNavigation; return true; })()",
        );
        outline.emit_by_name::<()>("activate", &[&2_u32]);
        assert_web_script_should_be_true(
            &preview,
            "(() => { window.carverDocumentNavigation = window.savedNavigation; document.querySelector('h2').click(); return true; })()",
        );
        assert!(run_main_context_until(
            || selected_position(outline) == Some(1)
        ));
        outline.emit_by_name::<()>("activate", &[&2_u32]);
        if mode == carver_config::EditorMode::Rendered {
            assert_web_script_should_be_true(
                &preview,
                "document.activeElement === document.querySelectorAll('h2')[1]",
            );
        } else {
            let source = widget_as::<sourceview5::View>(root, "source-editor").ok_or("source")?;
            assert!(
                run_main_context_until(|| widget_is_window_focus(source.upcast_ref())),
                "source root focus: {:?}",
                source
                    .root()
                    .and_downcast::<gtk::Window>()
                    .and_then(|window| gtk::prelude::RootExt::focus(&window))
                    .map(|widget| widget.widget_name())
            );
        }
        assert_web_script_should_be_true(
            &preview,
            "(() => { document.querySelector('h2').dispatchEvent(new MouseEvent('click', {bubbles:true})); return true; })()",
        );
        assert!(run_main_context_until(
            || selected_position(outline) == Some(1)
        ));
        assert_web_script_should_be_true(
            &preview,
            "(() => { document.querySelector('p').dispatchEvent(new MouseEvent('click', {bubbles:true})); return true; })()",
        );
        assert!(run_main_context_until(
            || selected_position(outline).is_none()
        ));
    }
    Ok(())
}

fn check_tree_should_remain_expanded(
    outline: &gtk::ListView,
    source: &sourceview5::View,
) -> TestResult {
    let selection = outline
        .model()
        .and_downcast::<gtk::SingleSelection>()
        .ok_or("selection")?;
    let tree = selection
        .model()
        .and_downcast::<gtk::TreeListModel>()
        .ok_or("tree")?;
    let parent = tree.row(0).ok_or("parent heading")?;
    let root = outline.clone().upcast::<gtk::Widget>();
    let expander =
        widget_as::<gtk::TreeExpander>(&root, "editor-outline-item").ok_or("expander")?;
    assert!(expander.hides_expander());
    let _ = expander.activate_action("listitem.collapse", None);
    let _ = expander.activate_action("listitem.toggle-expand", None);
    assert!(parent.is_expanded());
    assert_eq!(tree.n_items(), 3);
    source.buffer().place_cursor(&source.buffer().end_iter());
    assert!(run_main_context_until(
        || selected_position(outline) == Some(2)
    ));
    assert!(widget_is_window_focus(source.upcast_ref()));
    Ok(())
}

fn assert_navigation_should_ignore_queued_selection(
    rich: &webkit6::WebView,
    outline: &gtk::ListView,
) -> TestResult {
    assert_web_script_should_be_true(
        rich,
        "window.carverEditor.editor.commands.setTextSelection(1)",
    );
    assert!(run_main_context_until(
        || selected_position(outline) == Some(0)
    ));
    let selection = outline
        .model()
        .and_downcast::<gtk::SingleSelection>()
        .ok_or("selection")?;
    let transitions = Rc::new(std::cell::RefCell::new(Vec::new()));
    let captured = Rc::clone(&transitions);
    let handler = selection
        .connect_selected_notify(move |selection| captured.borrow_mut().push(selection.selected()));
    rich.evaluate_javascript(
        "window.carverEditor.reportSelection();",
        None,
        None,
        None::<&gtk::gio::Cancellable>,
        |_| {},
    );
    click_heading(outline, 2)?;
    assert_web_script_should_be_true(
        rich,
        "(() => { const e=window.carverEditor.editor; const p=[]; e.state.doc.descendants((n,i)=>{ if(n.type.name==='heading') p.push(i+1); }); return e.state.selection.from===p[2]; })()",
    );
    assert!(run_main_context_until(
        || selected_position(outline) == Some(2)
    ));
    selection.disconnect(handler);
    assert!(!transitions.borrow().is_empty());
    assert!(
        transitions.borrow().iter().all(|position| *position == 2),
        "outline flickered: {:?}",
        transitions.borrow()
    );
    Ok(())
}

fn heading_widget(root: &gtk::Widget, position: u32) -> Option<gtk::TreeExpander> {
    if let Some(expander) = root.downcast_ref::<gtk::TreeExpander>()
        && expander
            .list_row()
            .is_some_and(|row| row.position() == position)
    {
        return Some(expander.clone());
    }
    let mut child = root.first_child();
    while let Some(widget) = child {
        if let Some(expander) = heading_widget(&widget, position) {
            return Some(expander);
        }
        child = widget.next_sibling();
    }
    None
}

fn click_heading(outline: &gtk::ListView, position: u32) -> TestResult {
    let heading = heading_widget(outline.upcast_ref(), position).ok_or("heading widget")?;
    let controllers = heading.observe_controllers();
    let click = (0..controllers.n_items())
        .filter_map(|i| controllers.item(i))
        .filter_map(|item| item.downcast::<gtk::GestureClick>().ok())
        .find(|click| click.name().as_deref() == Some("outline-heading-click"))
        .ok_or("heading click")?;
    assert_eq!(click.button(), gtk::gdk::BUTTON_PRIMARY);
    click.emit_by_name::<()>("released", &[&1_i32, &1.0_f64, &1.0_f64]);
    Ok(())
}

fn assert_hover_should_preserve_clicked_heading(
    rich: &webkit6::WebView,
    outline: &gtk::ListView,
) -> TestResult {
    // GTK's built-in single-click mode selects on hover; the explicit gesture must replace it.
    assert!(!outline.is_single_click_activate());
    click_heading(outline, 1)?;
    assert_web_script_should_be_true(
        rich,
        "(() => { const e=window.carverEditor.editor; const p=[]; e.state.doc.descendants((n,i)=>{ if(n.type.name==='heading') p.push(i+1); }); return e.state.selection.from===p[1]; })()",
    );
    assert!(run_main_context_until(
        || selected_position(outline) == Some(1)
    ));
    let selected_row = heading_widget(outline.upcast_ref(), 1)
        .and_then(|heading| heading.parent())
        .ok_or("selected row")?;
    let hovered_row = heading_widget(outline.upcast_ref(), 2)
        .and_then(|heading| heading.parent())
        .ok_or("hovered row")?;
    hovered_row.set_state_flags(gtk::StateFlags::PRELIGHT, false);
    assert_eq!(selected_position(outline), Some(1));
    assert!(
        selected_row
            .state_flags()
            .contains(gtk::StateFlags::SELECTED)
    );
    assert!(
        !hovered_row
            .state_flags()
            .contains(gtk::StateFlags::SELECTED)
    );
    hovered_row.unset_state_flags(gtk::StateFlags::PRELIGHT);
    assert_eq!(selected_position(outline), Some(1));
    // Hovering the current heading keeps both its selection and its distinct selected styling.
    selected_row.set_state_flags(gtk::StateFlags::PRELIGHT, false);
    assert!(
        selected_row
            .state_flags()
            .contains(gtk::StateFlags::SELECTED)
    );
    selected_row.unset_state_flags(gtk::StateFlags::PRELIGHT);
    Ok(())
}

fn assert_rich_focus_should_not_escape_mode_switch(
    root: &gtk::Widget,
    outline: &gtk::ListView,
) -> TestResult {
    let rich = widget_as::<webkit6::WebView>(root, "rich-editor").ok_or("rich editor")?;
    let source = widget_as::<sourceview5::View>(root, "source-editor").ok_or("source")?;
    click_heading(outline, 1)?;
    widget_as::<gtk::ToggleButton>(root, "editor-mode-source")
        .ok_or("source mode")?
        .set_active(true);
    source.grab_focus();
    if let Some(window) = source.root() {
        window.set_focus(Some(&source));
    }
    // A round-trip after the queued navigation also drains its native completion callback.
    assert_web_script_should_be_true(&rich, "true");
    assert!(widget_is_window_focus(source.upcast_ref()));
    Ok(())
}
pub(super) fn assert_document_sidebar_should_focus_and_show_file_details(
    root: &gtk::Widget,
    source: &gtk::TextView,
    source_mode: &gtk::ToggleButton,
    rich_mode: &gtk::ToggleButton,
    rich: &webkit6::WebView,
    client: &super::super::support::TestLibraryClient,
    note_id: carver_sdk::NoteId,
) -> TestResult {
    let image = gtk::gdk_pixbuf::Pixbuf::new(gtk::gdk_pixbuf::Colorspace::Rgb, true, 8, 16, 16)
        .ok_or("image fixture")?;
    image.fill(0x33aa_66ff);
    let bytes = image.save_to_bufferv("png", &[])?;
    let path = client.store_asset(note_id, "png", &bytes)?;
    let text = format!("Before\n\n![Diagram]({path})\n\nAfter");
    source_mode.set_active(true);
    source.buffer().set_text(&text);
    let toggle = widget_as::<gtk::ToggleButton>(root, "editor-document-sidebar-toggle")
        .ok_or("document sidebar toggle")?;
    toggle.set_active(true);
    assert!(run_main_context_until_for(Duration::from_secs(10), || {
        widget_as::<gtk::Label>(root, "editor-media-size")
            .is_some_and(|label| label.text() == glib::format_size(bytes.len() as u64))
    }));
    let list = widget_as::<gtk::ListBox>(root, "editor-media-list").ok_or("media list")?;
    assert!(run_main_context_until(|| list.height() > 0));
    assert!(
        list.height() < 150,
        "one card should use its natural height"
    );
    let button = widget_as::<gtk::Button>(root, "editor-media-item").ok_or("media button")?;
    let content = button.child().ok_or("card contents")?;
    let thumbnail_frame = content.first_child().ok_or("thumbnail frame")?;
    assert!(thumbnail_frame.has_css_class("media-thumbnail"));
    assert_eq!(thumbnail_frame.overflow(), gtk::Overflow::Hidden);
    let thumbnail = thumbnail_frame
        .first_child()
        .and_then(|widget| widget.downcast::<gtk::Image>().ok())
        .ok_or("thumbnail")?;
    assert!(
        thumbnail.paintable().is_some(),
        "managed images should display a thumbnail"
    );
    super::rendering::assert_thumbnail_should_follow_markup_kind(root, source, &path, &text);
    let button = widget_as::<gtk::Button>(root, "editor-media-item").ok_or("media button")?;
    button.emit_clicked();
    let (start, end) = source
        .buffer()
        .selection_bounds()
        .ok_or("source media selection")?;
    assert_eq!(
        source.buffer().text(&start, &end, false),
        format!("![Diagram]({path})")
    );
    assert!(
        run_main_context_until(|| widget_is_window_focus(source.upcast_ref())),
        "source focus: {:?}",
        source
            .root()
            .and_downcast::<gtk::Window>()
            .and_then(|window| gtk::prelude::RootExt::focus(&window))
            .map(|widget| widget.widget_name())
    );
    assert!(list.selected_row().is_some());
    source.buffer().place_cursor(&source.buffer().start_iter());
    assert!(list.selected_row().is_none());
    source
        .buffer()
        .place_cursor(&source.buffer().iter_at_offset(10));
    assert!(list.selected_row().is_some());
    rich_mode.set_active(true);
    assert!(run_main_context_until(|| rich.is_visible()));
    assert_web_script_should_be_true(rich, "Boolean(document.querySelector('#editor img'))");
    let button = widget_as::<gtk::Button>(root, "editor-media-item").ok_or("media button")?;
    button.emit_clicked();
    let selected = Rc::new(Cell::new(false));
    let result = Rc::clone(&selected);
    rich.evaluate_javascript(
        "!!document.querySelector('img.ProseMirror-selectednode')",
        None,
        None,
        None::<&gtk::gio::Cancellable>,
        move |value| result.set(value.is_ok_and(|value| value.to_boolean())),
    );
    assert!(run_main_context_until(|| selected.get()));
    assert!(list.selected_row().is_some());
    assert_rich_media_selection_should_update_sidebar(rich, &list, &path);
    assert!(widget_is_window_focus(rich.upcast_ref()));
    assert_eq!(
        source.buffer().text(
            &source.buffer().start_iter(),
            &source.buffer().end_iter(),
            false
        ),
        text
    );
    assert_attachment_card_should_focus_in_edit_and_preview(
        root,
        source,
        source_mode,
        rich_mode,
        rich,
        client,
        note_id,
    )?;
    toggle.set_active(false);
    Ok(())
}
pub(super) fn assert_rich_media_selection_should_update_sidebar(
    rich: &webkit6::WebView,
    list: &gtk::ListBox,
    path: &str,
) {
    assert_web_script_should_be_true(
        rich,
        "window.carverEditor.editor.commands.setTextSelection(1)",
    );
    assert!(run_main_context_until(|| list.selected_row().is_none()));
    assert_web_script_should_be_true(
        rich,
        &format!("window.carverEditor.focusMedia('{path}', 0)"),
    );
    assert!(run_main_context_until(|| list.selected_row().is_some()));
}
pub(super) fn assert_attachment_card_should_focus_in_edit_and_preview(
    root: &gtk::Widget,
    source: &gtk::TextView,
    source_mode: &gtk::ToggleButton,
    rich_mode: &gtk::ToggleButton,
    rich: &webkit6::WebView,
    client: &super::super::support::TestLibraryClient,
    note_id: carver_sdk::NoteId,
) -> TestResult {
    let bytes = vec![b'x'; 4096];
    let path = client.store_asset(note_id, "pdf", &bytes)?;
    source_mode.set_active(true);
    let text = format!("[External](https://example.test/{path})\n\n[Brief]({path})");
    source.buffer().set_text(&text);
    assert!(run_main_context_until(|| widget_as::<gtk::Label>(
        root,
        "editor-media-size"
    )
    .is_some_and(|label| label.text() == glib::format_size(4096))));
    let button = widget_as::<gtk::Button>(root, "editor-media-item").ok_or("attachment card")?;
    let icon = button
        .child()
        .and_then(|child| child.first_child())
        .and_then(|widget| widget.downcast::<gtk::Image>().ok())
        .ok_or("file icon")?;
    assert!(
        icon.gicon().is_some(),
        "attachments should display a file type icon"
    );
    rich_mode.set_active(true);
    button.emit_clicked();
    assert_web_script_should_be_true(rich, "window.getSelection().toString() === 'Brief'");
    let rendered_mode =
        widget_as::<gtk::ToggleButton>(root, "editor-mode-rendered").ok_or("preview mode")?;
    rendered_mode.set_active(true);
    let preview =
        widget_as::<webkit6::WebView>(root, "editor-rendered-preview").ok_or("preview")?;
    assert!(run_main_context_until(|| !preview.is_loading()));
    assert_web_script_should_be_true(&preview, "!!document.querySelector('a')");
    let button = widget_as::<gtk::Button>(root, "editor-media-item").ok_or("attachment card")?;
    button.emit_clicked();
    assert_web_script_should_be_true(&preview, "document.activeElement?.tagName === 'A'");
    let list = widget_as::<gtk::ListBox>(root, "editor-media-list").ok_or("media list")?;
    assert!(run_main_context_until(|| list.selected_row().is_some()));
    assert_web_script_should_be_true(
        &preview,
        "document.body.dispatchEvent(new MouseEvent('click', {bubbles:true})); true",
    );
    assert!(run_main_context_until(|| list.selected_row().is_none()));
    assert_web_script_should_be_true(
        &preview,
        "document.querySelector('a[href^=\"assets/\"]').dispatchEvent(new MouseEvent('click', {bubbles:true, cancelable:true})); true",
    );
    assert!(run_main_context_until(|| list.selected_row().is_some()));
    assert_web_script_should_be_true(
        &preview,
        &format!("document.activeElement?.getAttribute('href') === '{path}'"),
    );
    let add_files = widget_as::<gtk::Button>(root, "editor-media-add-files").ok_or("add files")?;
    assert!(!add_files.is_sensitive());
    assert!(widget_is_window_focus(preview.upcast_ref()));
    assert_eq!(
        source.buffer().text(
            &source.buffer().start_iter(),
            &source.buffer().end_iter(),
            false
        ),
        text
    );
    Ok(())
}
pub(super) fn assert_document_sidebar_visibility_should_restore_without_reentrant_toggles()
-> TestResult {
    use crate::mvu::{AppMsg, EditorMsg};
    let SidebarFixture {
        directory: _directory,
        surface,
        runtime,
        window,
        config_path,
        ..
    } = fixture()?;
    let toggle = widget_as::<gtk::ToggleButton>(&surface, "editor-document-sidebar-toggle")
        .ok_or("document sidebar toggle")?;
    let editor_view =
        widget_as::<adw::ToolbarView>(&surface, "editor-surface").ok_or("editor view")?;
    let controllers = editor_view.observe_controllers();
    let editor_shortcuts = (0..controllers.n_items())
        .filter_map(|index| controllers.item(index))
        .filter_map(|controller| controller.downcast::<gtk::EventControllerKey>().ok())
        .find(|controller| controller.name().as_deref() == Some("editor-window-shortcuts"))
        .ok_or("editor shortcuts")?;
    for source in [
        "Plain document",
        "![Photo](assets/missing.png)",
        "Another document",
    ] {
        runtime.dispatch(AppMsg::Editor(EditorMsg::Load {
            note_id: carver_sdk::NoteId::new(),
            revision: carver_sdk::Revision(1),
            source: source.to_owned(),
        }));
        assert!(toggle.is_active());
        assert!(runtime.model().config.editor.show_document_sidebar);
        let pages = widget_as::<gtk::Stack>(&surface, "editor-media-pages").ok_or("media pages")?;
        assert_eq!(
            pages.visible_child_name().as_deref(),
            Some(if source.starts_with("![") {
                "files"
            } else {
                "empty"
            })
        );
    }
    let sidebar = find_widget(&surface, "editor-document-sidebar").ok_or("document sidebar")?;
    assert!(run_main_context_until(|| sidebar.width() > 0));
    assert!(sidebar.width() <= 320, "sidebar width: {}", sidebar.width());
    let document_sidebar_split =
        widget_as::<adw::OverlaySplitView>(&surface, "editor-document-sidebar-split-view")
            .ok_or("media split")?;
    assert_document_sidebar_split_configuration(&document_sidebar_split);
    assert!(document_sidebar_split.shows_sidebar());
    window.set_default_size(700, 800);
    assert!(run_main_context_until(
        || document_sidebar_split.is_collapsed()
    ));
    window.set_default_size(1200, 800);
    assert!(run_main_context_until(
        || !document_sidebar_split.is_collapsed()
    ));
    assert_document_sidebar_shortcut(&editor_shortcuts, &config_path, &document_sidebar_split)?;
    toggle.set_active(false);
    assert!(
        !carver_config::load(&config_path)?
            .editor
            .show_document_sidebar
    );
    assert!(run_main_context_until(
        || !document_sidebar_split.shows_sidebar()
    ));
    runtime.dispatch(AppMsg::Editor(EditorMsg::Load {
        note_id: carver_sdk::NoteId::new(),
        revision: carver_sdk::Revision(1),
        source: String::from("![Image](assets/another.png)"),
    }));
    assert!(!toggle.is_active());
    toggle.set_active(true);
    assert!(
        carver_config::load(&config_path)?
            .editor
            .show_document_sidebar
    );
    assert_missing_media_preview_should_report_error(&surface, &runtime)?;
    window.close();
    Ok(())
}
pub(super) fn assert_document_sidebar_shortcut(
    shortcuts: &gtk::EventControllerKey,
    config_path: &std::path::Path,
    document_sidebar_split: &adw::OverlaySplitView,
) -> TestResult {
    let modifiers = gtk::gdk::ModifierType::empty();
    for expected_visible in [false, true] {
        let handled = shortcuts
            .emit_by_name::<bool>("key-pressed", &[&gtk::gdk::Key::F9, &0_u32, &modifiers]);
        assert!(handled);
        assert_eq!(
            carver_config::load(config_path)?
                .editor
                .show_document_sidebar,
            expected_visible
        );
        assert!(run_main_context_until(|| document_sidebar_split
            .shows_sidebar()
            == expected_visible));
    }
    Ok(())
}
pub(super) fn assert_document_sidebar_split_configuration(
    document_sidebar_split: &adw::OverlaySplitView,
) {
    assert_eq!(
        document_sidebar_split.sidebar_position(),
        gtk::PackType::End
    );
    assert!(document_sidebar_split.is_pin_sidebar());
    assert!(!document_sidebar_split.property::<bool>("enable-hide-gesture"));
    assert!(!document_sidebar_split.property::<bool>("enable-show-gesture"));
}
pub(super) fn assert_missing_media_preview_should_report_error(
    surface: &gtk::Widget,
    runtime: &crate::mvu::AppRuntime<carver_storage_sqlite::SqliteLibrary>,
) -> TestResult {
    let before = runtime.model().editor.ok_or("document")?.source;
    let button =
        widget_as::<gtk::Button>(surface, "editor-media-preview").ok_or("preview action")?;
    button.emit_clicked();
    assert!(run_main_context_until(|| runtime.model().notice.is_some()));
    assert_eq!(runtime.model().editor.ok_or("document")?.source, before);
    Ok(())
}
