//! Display-backed find bar and editor shortcut coverage.
use super::*;

pub(super) fn find_bar_and_shortcuts_should_navigate_matches(
    fixture: &WindowFixture,
    note: &carver_sdk::NoteSummary,
) -> TestResult {
    let client = &fixture.client;
    let root = fixture.root()?;
    let source = fixture.source()?;
    let find_bar = widget_as::<gtk::SearchBar>(&root, "editor-find-bar").ok_or("find bar")?;
    let find_entry =
        widget_as::<gtk::SearchEntry>(&root, "editor-find-entry").ok_or("find entry")?;
    let find_count = widget_as::<gtk::Label>(&root, "editor-find-count").ok_or("find count")?;
    let find_next = widget_as::<gtk::Button>(&root, "editor-find-next").ok_or("find next")?;
    let find_previous =
        widget_as::<gtk::Button>(&root, "editor-find-previous").ok_or("find previous")?;
    let find_close = widget_as::<gtk::Button>(&root, "editor-find-close").ok_or("find close")?;
    let editor_view =
        widget_as::<adw::ToolbarView>(&root, "editor-surface").ok_or("editor view")?;
    let controllers = editor_view.observe_controllers();
    let editor_shortcuts = (0..controllers.n_items())
        .filter_map(|index| controllers.item(index))
        .filter_map(|controller| controller.downcast::<gtk::EventControllerKey>().ok())
        .find(|controller| controller.name().as_deref() == Some("editor-window-shortcuts"))
        .ok_or("editor shortcuts")?;
    assert_eq!(
        editor_shortcuts.propagation_phase(),
        gtk::PropagationPhase::Capture
    );
    let favorite_handled = editor_shortcuts.emit_by_name::<bool>(
        "key-pressed",
        &[
            &gtk::gdk::Key::f,
            &0_u32,
            &(gtk::gdk::ModifierType::CONTROL_MASK | gtk::gdk::ModifierType::SHIFT_MASK),
        ],
    );
    assert!(favorite_handled);
    let favorite_restored = editor_shortcuts.emit_by_name::<bool>(
        "key-pressed",
        &[
            &gtk::gdk::Key::f,
            &0_u32,
            &(gtk::gdk::ModifierType::CONTROL_MASK | gtk::gdk::ModifierType::SHIFT_MASK),
        ],
    );
    assert!(favorite_restored);
    assert!(run_main_context_until(|| client
        .note(note.id)
        .ok()
        .flatten()
        .is_some_and(|saved| saved.is_favorite)));
    let find_shortcut = (0..controllers.n_items())
        .filter_map(|index| controllers.item(index))
        .filter_map(|controller| controller.downcast::<gtk::EventControllerKey>().ok())
        .find(|controller| controller.name().as_deref() == Some("editor-find-shortcuts"))
        .ok_or("find shortcut controller")?;
    assert_eq!(
        find_shortcut.propagation_phase(),
        gtk::PropagationPhase::Capture
    );
    source.buffer().set_text("needle one\nNeedle two");
    source.buffer().place_cursor(&source.buffer().start_iter());
    let handled = find_shortcut.emit_by_name::<bool>(
        "key-pressed",
        &[
            &gtk::gdk::Key::f,
            &0_u32,
            &gtk::gdk::ModifierType::CONTROL_MASK,
        ],
    );
    assert!(handled);
    assert!(find_bar.is_search_mode());
    find_entry.set_text("needle");
    assert!(run_main_context_until(|| find_count.text() == "2 matches"));
    assert!(find_next.is_sensitive() && find_previous.is_sensitive());
    assert_eq!(
        source
            .buffer()
            .selection_bounds()
            .map(|(start, end)| source.buffer().text(&start, &end, false).to_string()),
        Some("needle".to_owned())
    );
    find_next.emit_clicked();
    assert_eq!(
        source
            .buffer()
            .selection_bounds()
            .map(|(start, end)| source.buffer().text(&start, &end, false).to_string()),
        Some("Needle".to_owned())
    );
    find_previous.emit_clicked();
    assert_eq!(
        source
            .buffer()
            .selection_bounds()
            .map(|(start, end)| source.buffer().text(&start, &end, false).to_string()),
        Some("needle".to_owned())
    );
    find_close.emit_clicked();
    assert!(!find_bar.is_search_mode());
    Ok(())
}
