//! Display-backed document navigation and outline presentation.
use super::*;
use crate::mvu::{AppDispatcher, AppModel, AppMsg, AppRuntime, EditorMsg, PreferencesMsg};

pub(super) struct SidebarFixture {
    pub _directory: tempfile::TempDir,
    pub client: super::super::support::TestLibraryClient,
    pub surface: gtk::Widget,
    pub runtime: AppRuntime<carver_storage_sqlite::SqliteLibrary>,
    pub window: gtk::Window,
    pub config_path: std::path::PathBuf,
}

pub(super) fn fixture() -> Result<SidebarFixture, Box<dyn std::error::Error>> {
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
    let window = gtk::Window::builder()
        .default_width(1200)
        .default_height(800)
        .child(&stack)
        .build();
    let runtime = AppRuntime::new_with_config_path(
        client.clone(),
        AppModel::new(&config),
        crate::view::ViewRefs::new(stack, adw::StatusPage::new(), adw::StatusPage::new())
            .with_editor(refs),
        Some(config_path.clone()),
    );
    runtime.bind_dispatcher(&dispatcher);
    window.present();
    Ok(SidebarFixture {
        _directory: directory,
        client,
        surface,
        runtime,
        window,
        config_path,
    })
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
    assert_eq!(
        toggle.icon_name().as_deref(),
        Some("sidebar-show-right-symbolic")
    );
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
    fixture.window.close();
    Ok(())
}

fn selected_position(list: &gtk::ListView) -> Option<u32> {
    list.model()
        .and_downcast::<gtk::SingleSelection>()
        .map(|selection| selection.selected())
        .filter(|position| *position != gtk::INVALID_LIST_POSITION)
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
