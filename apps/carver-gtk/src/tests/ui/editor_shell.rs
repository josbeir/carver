//! Display-backed editor shell, modes, and responsive toolbar coverage.
use super::*;

pub(super) fn source_editor_should_configure_language_and_gutter(
    fixture: &WindowFixture,
    note: &carver_sdk::NoteSummary,
) -> TestResult {
    let root = fixture.root()?;
    let sidebar = fixture.sidebar()?;
    let note_list = fixture.note_list()?;
    assert!(activate_browser_note(&note_list, note.id));
    let route_stack = widget_as::<gtk::Stack>(&root, "content-route-stack").ok_or("route stack")?;
    assert!(run_main_context_until(|| {
        route_stack.visible_child_name().as_deref() == Some("editor")
            && sidebar_selected_badge(&sidebar).as_deref() == Some("all-notes-count")
    }));
    let controllers = route_stack.observe_controllers();
    let mouse_back = (0..controllers.n_items())
        .filter_map(|index| controllers.item(index))
        .find_map(|controller| controller.downcast::<gtk::EventControllerLegacy>().ok())
        .filter(|controller| controller.name().as_deref() == Some("page-mouse-back-controller"))
        .ok_or("page mouse back controller")?;
    assert_eq!(
        mouse_back.propagation_phase(),
        gtk::PropagationPhase::Capture
    );
    let touchpad_back = (0..controllers.n_items())
        .filter_map(|index| controllers.item(index))
        .find_map(|controller| controller.downcast::<gtk::EventControllerScroll>().ok())
        .filter(|controller| controller.name().as_deref() == Some("page-touchpad-back-controller"))
        .ok_or("editor touchpad back controller")?;
    assert_eq!(
        touchpad_back.propagation_phase(),
        gtk::PropagationPhase::Capture
    );
    assert_eq!(
        touchpad_back.flags(),
        gtk::EventControllerScrollFlags::BOTH_AXES
    );
    let source_view =
        widget_as::<sourceview5::View>(&root, "source-editor").ok_or("GtkSourceView")?;
    let source_buffer = source_view
        .buffer()
        .downcast::<sourceview5::Buffer>()
        .map_err(|_| "GtkSourceBuffer")?;
    assert_eq!(
        source_buffer
            .language()
            .map(|language| language.id().to_string()),
        Some(String::from("carve"))
    );
    let source_scheme = source_buffer
        .style_scheme()
        .map(|scheme| scheme.id().to_string());
    let expected_source_scheme = if adw::StyleManager::default().is_dark() {
        "carve-writing-focus-dark"
    } else {
        "carve-writing-focus-light"
    };
    assert_eq!(source_scheme.as_deref(), Some(expected_source_scheme));
    let source_style_scheme = source_buffer.style_scheme().ok_or("source style scheme")?;
    for (level, expected_scale) in [
        (1, "1.45"),
        (2, "1.30"),
        (3, "1.18"),
        (4, "1.10"),
        (5, "1.04"),
        (6, "1.00"),
    ] {
        let style = source_style_scheme
            .style(&format!("carve:heading-{level}"))
            .ok_or("source heading style")?;
        assert_eq!(style.scale().as_deref(), Some(expected_scale));
    }
    assert!(source_view.shows_line_numbers());
    assert!(source_view.is_highlight_current_line());
    assert!(source_buffer.is_highlight_syntax());
    assert_eq!(source_view.pixels_above_lines(), 3);
    assert_eq!(source_view.pixels_below_lines(), 3);
    Ok(())
}

pub(super) fn responsive_editor_should_switch_compact_and_desktop_toolbars(
    fixture: &WindowFixture,
) -> TestResult {
    let window = fixture.window.clone();
    let root = fixture.root()?;
    let source_mode = fixture.source_mode()?;
    window.set_default_size(360, 640);
    let responsive_editor = widget_as::<adw::BreakpointBin>(&root, "editor-responsive-container")
        .ok_or("responsive editor container")?;
    assert!(run_main_context_until(|| responsive_editor.width() <= 700));
    assert!(
        widget_as::<gtk::Label>(&root, "editor-mode-rich-label")
            .is_some_and(|label| !label.is_visible())
    );
    assert!(
        widget_as::<gtk::Box>(&root, "formatting-toolbar-desktop")
            .is_some_and(|toolbar| !toolbar.is_visible())
    );
    assert!(
        widget_as::<gtk::Box>(&root, "formatting-toolbar-compact")
            .is_some_and(|toolbar| toolbar.is_visible())
    );
    assert!(widget_as::<gtk::MenuButton>(&root, "formatting-toolbar-more").is_some());
    assert!(
        widget_as::<gtk::ToggleButton>(&root, "favorite-note-button")
            .is_some_and(|button| !button.is_visible())
    );
    assert!(
        widget_as::<gtk::Button>(&root, "back-to-notes-button")
            .is_some_and(|button| button.is_visible())
    );
    window.set_default_size(390, 844);
    assert!(run_main_context_until(|| responsive_editor.width() >= 360));
    window.set_default_size(1120, 760);
    assert!(run_main_context_until(|| {
        widget_as::<gtk::Box>(&root, "formatting-toolbar-desktop")
            .is_some_and(|toolbar| toolbar.is_visible())
    }));
    let toolbar =
        widget_as::<gtk::Box>(&root, "formatting-toolbar").ok_or("shared formatting toolbar")?;
    let toolbar_bar =
        widget_as::<gtk::Box>(&root, "formatting-toolbar-bar").ok_or("formatting toolbar bar")?;
    assert!(toolbar_bar.is_visible());
    assert_shared_toolbar_controls(&root)?;
    let split_toggle =
        widget_as::<gtk::ToggleButton>(&root, "source-split-toggle").ok_or("split toggle")?;
    assert!(
        !split_toggle.is_visible(),
        "the split preview control should be hidden outside Source mode"
    );
    source_mode.set_active(true);
    assert_eq!(
        toolbar,
        widget_as::<gtk::Box>(&root, "formatting-toolbar").ok_or("source toolbar")?
    );
    assert_shared_toolbar_controls(&root)?;
    assert!(split_toggle.is_sensitive());
    assert!(
        split_toggle.is_visible(),
        "the split preview control should appear in Source mode"
    );
    window.set_default_size(360, 640);
    assert!(run_main_context_until(|| {
        !split_toggle.is_sensitive() && !split_toggle.is_visible()
    }));
    let options_menu =
        widget_as::<gtk::MenuButton>(&root, "editor-options-menu").ok_or("editor options")?;
    assert!(run_main_context_until(|| options_menu
        .menu_model()
        .is_some_and(|model| model.n_items() == 5)));
    options_menu.popup();
    let options_popover = options_menu.popover().ok_or("editor options popover")?;
    let split_preview_label = run_main_context_until(|| {
        find_label(options_popover.upcast_ref(), "Show rendered preview").is_some()
    });
    assert!(split_preview_label);
    assert!(
        find_label(options_popover.upcast_ref(), "Show rendered preview")
            .is_some_and(|label| !label.is_sensitive())
    );
    assert!(find_label(options_popover.upcast_ref(), "Remove from Favorites").is_some());
    options_menu.popdown();
    assert!(!split_toggle.is_active());
    window.set_default_size(1120, 760);
    assert!(run_main_context_until(|| {
        split_toggle.is_sensitive() && split_toggle.is_visible()
    }));
    Ok(())
}

pub(super) fn editor_options_should_adapt_to_layout(fixture: &WindowFixture) -> TestResult {
    let root = fixture.root()?;
    let source = fixture.source()?;
    let source_view = fixture.source_view()?;
    let source_controllers = source_view.observe_controllers();
    let source_shortcuts = (0..source_controllers.n_items())
        .filter_map(|index| source_controllers.item(index))
        .filter_map(|controller| controller.downcast::<gtk::EventControllerKey>().ok())
        .find(|controller| controller.name().as_deref() == Some("source-format-shortcuts"))
        .ok_or("source format shortcuts")?;
    source.buffer().set_text("First line");
    source.buffer().place_cursor(&source.buffer().end_iter());
    let handled = source_shortcuts.emit_by_name::<bool>(
        "key-pressed",
        &[
            &gtk::gdk::Key::Return,
            &0_u32,
            &gtk::gdk::ModifierType::SHIFT_MASK,
        ],
    );
    assert!(handled);
    assert_eq!(
        source.buffer().text(
            &source.buffer().start_iter(),
            &source.buffer().end_iter(),
            false
        ),
        "First line\\\n"
    );
    let options_menu =
        widget_as::<gtk::MenuButton>(&root, "editor-options-menu").ok_or("editor options")?;
    assert!(run_main_context_until(|| options_menu
        .menu_model()
        .is_some_and(|model| model.n_items() == 3)));
    assert!(
        options_menu
            .popover()
            .and_downcast::<gtk::PopoverMenu>()
            .is_some()
    );
    options_menu.popup();
    let wide_popover = options_menu.popover().ok_or("editor options popover")?;
    assert!(
        run_main_context_until(|| find_label(wide_popover.upcast_ref(), "Move to Trash").is_some()),
        "wide menu should keep the always-available options"
    );
    assert!(
        find_label(wide_popover.upcast_ref(), "Copy note").is_some(),
        "wide menu should hold the copy action without a header button"
    );
    assert!(
        find_label(wide_popover.upcast_ref(), "Back to notes")
            .is_none_or(|label| !label.is_visible()),
        "wide menu should not show a Back to notes entry"
    );
    options_menu.popdown();
    Ok(())
}
