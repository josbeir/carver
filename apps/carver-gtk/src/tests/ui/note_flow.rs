//! Display-backed note lifecycle coverage from editor shortcuts and trash.
use super::*;

pub(super) fn note_should_delete_restore_and_favorite_from_shortcuts(
    fixture: &WindowFixture,
    note: &carver_sdk::NoteSummary,
) -> TestResult {
    let window = fixture.window.clone();
    let client = &fixture.client;
    let category = &fixture.category;
    let root = fixture.root()?;
    let sidebar = fixture.sidebar()?;
    let note_list = fixture.note_list()?;
    let route_stack = fixture.route_stack()?;
    let editor_shortcuts = fixture.editor_shortcuts()?;
    let trash_back =
        widget_as::<gtk::Button>(&root, "back-from-trash-button").ok_or("trash back")?;
    assert!(activate_browser_note(&note_list, note.id));
    assert!(run_main_context_until(|| note_tab_is_active(&root)));
    assert!(widget_as::<gtk::Button>(&root, "delete-note-button").is_none());
    let delete_handled = editor_shortcuts.emit_by_name::<bool>(
        "key-pressed",
        &[
            &gtk::gdk::Key::d,
            &0_u32,
            &gtk::gdk::ModifierType::CONTROL_MASK,
        ],
    );
    assert!(delete_handled);
    assert!(run_main_context_until(|| client
        .note(note.id)
        .ok()
        .flatten()
        .is_some_and(|deleted| deleted.trashed_at.is_some())));
    assert!(run_main_context_until(|| {
        widget_as::<gtk::Button>(&root, &format!("restore-note:{}", note.id)).is_some()
    }));
    let restore_note = widget_as::<gtk::Button>(&root, &format!("restore-note:{}", note.id))
        .ok_or("restore note")?;
    assert!(restore_note.is_visible());
    assert!(restore_note.has_css_class("flat"));
    assert_eq!(restore_note.tooltip_text().as_deref(), Some("Restore"));
    restore_note.emit_clicked();
    assert!(run_main_context_until(|| client
        .note(note.id)
        .ok()
        .flatten()
        .is_some_and(|restored| restored.trashed_at.is_none())));
    trash_back.emit_clicked();
    assert!(run_main_context_until(|| {
        route_stack.visible_child_name().as_deref() == Some("browser")
            && find_widget(&root, &format!("note:{}", note.id)).is_some()
    }));
    assert!(activate_browser_note(&note_list, note.id));
    assert!(run_main_context_until(|| note_tab_is_active(&root)));
    let favorite =
        widget_as::<gtk::ToggleButton>(&root, "favorite-note-button").ok_or("favorite button")?;
    favorite.emit_clicked();
    favorite.emit_clicked();
    // The Notes list is a pinned tab; select it directly instead of a back button.
    let tabs = widget_as::<adw::TabView>(&root, "workspace-tabs").ok_or("tabs")?;
    let notes = tabs.nth_page(0);
    tabs.set_selected_page(&notes);
    assert!(run_main_context_until(|| {
        route_stack.visible_child_name().as_deref() == Some("browser")
    }));
    assert!(!note_tab_is_active(&root));
    assert!(sidebar_select(
        &sidebar,
        &format!("category-count:{}", category.id)
    ));
    assert_eq!(route_stack.visible_child_name().as_deref(), Some("browser"));
    assert!(run_main_context_until(|| {
        route_stack.visible_child_name().as_deref() == Some("browser")
            && widget_as::<gtk::Label>(&root, "browser-hero-title")
                .is_some_and(|title| title.text() == "Notes")
            && client
                .note(note.id)
                .ok()
                .flatten()
                .is_some_and(|saved| saved.is_favorite)
    }));
    // Ctrl+N creates a note from the editor, and Ctrl+W closes the active tab.
    tab_shortcuts_should_create_and_close_tabs(fixture, note, &editor_shortcuts)?;
    window.close();
    Ok(())
}

/// Exercises the editor's tab shortcuts against the shared window.
fn tab_shortcuts_should_create_and_close_tabs(
    fixture: &WindowFixture,
    note: &carver_sdk::NoteSummary,
    editor_shortcuts: &gtk::EventControllerKey,
) -> TestResult {
    let root = fixture.root()?;
    let sidebar = fixture.sidebar()?;
    let note_list = fixture.note_list()?;
    let route_stack = fixture.route_stack()?;
    let tabs = widget_as::<adw::TabView>(&root, "workspace-tabs").ok_or("tabs")?;
    // The note was moved out of the category the browser was showing.
    assert!(sidebar_select(&sidebar, "all-notes-count"));
    assert!(run_main_context_until(|| {
        route_stack.visible_child_name().as_deref() == Some("browser")
    }));
    assert!(run_main_context_until(|| activate_browser_note(
        &note_list, note.id
    )));
    assert!(run_main_context_until(|| note_tab_is_active(&root)));

    let pages_before = tabs.n_pages();
    assert!(editor_shortcuts.emit_by_name::<bool>(
        "key-pressed",
        &[
            &gtk::gdk::Key::n,
            &0_u32,
            &gtk::gdk::ModifierType::CONTROL_MASK
        ],
    ));
    assert!(run_main_context_until(|| tabs.n_pages() == pages_before + 1));

    let pages_with_new = tabs.n_pages();
    assert!(editor_shortcuts.emit_by_name::<bool>(
        "key-pressed",
        &[
            &gtk::gdk::Key::w,
            &0_u32,
            &gtk::gdk::ModifierType::CONTROL_MASK
        ],
    ));
    assert!(run_main_context_until(|| {
        tabs.n_pages() == pages_with_new - 1
    }));
    Ok(())
}

/// Verifies the tab-navigation shortcut controller cycles and reorders tabs.
pub(super) fn tab_shortcuts_should_cycle_tabs(
    fixture: &WindowFixture,
    note: &carver_sdk::NoteSummary,
) -> TestResult {
    let root = fixture.root()?;
    let note_list = fixture.note_list()?;
    assert!(activate_browser_note(&note_list, note.id));
    assert!(run_main_context_until(|| note_tab_is_active(&root)));
    // Open a second note so navigation has somewhere to go.
    if let Some(second) = browser_note_ids(&note_list)
        .into_iter()
        .find(|note_id| *note_id != note.id)
    {
        assert!(activate_browser_note(&note_list, second));
        assert!(run_main_context_until(|| note_tab_is_active(&root)));
    }
    let tabs = widget_as::<adw::TabView>(&root, "workspace-tabs").ok_or("tabs")?;
    let controller = tabs
        .observe_controllers()
        .iter::<glib::Object>()
        .filter_map(Result::ok)
        .find_map(|object| object.downcast::<gtk::EventControllerKey>().ok())
        .filter(|controller| controller.name().as_deref() == Some("workspace-tab-shortcuts"))
        .ok_or("tab shortcuts controller")?;

    let before = tabs.selected_page();
    let handled = controller.emit_by_name::<bool>(
        "key-pressed",
        &[
            &gtk::gdk::Key::Page_Down,
            &0_u32,
            &gtk::gdk::ModifierType::CONTROL_MASK,
        ],
    );
    assert!(handled);
    assert!(run_main_context_until(|| {
        tabs.selected_page().as_ref() != before.as_ref()
    }));

    // Ctrl+Shift+Page_Down moves the selected note tab one position right.
    if let Some(page) = tabs.selected_page()
        && !page.is_pinned()
        && tabs.n_pages() > 2
    {
        let position = tabs.page_position(&page);
        let handled = controller.emit_by_name::<bool>(
            "key-pressed",
            &[
                &gtk::gdk::Key::Page_Down,
                &0_u32,
                &(gtk::gdk::ModifierType::CONTROL_MASK | gtk::gdk::ModifierType::SHIFT_MASK),
            ],
        );
        assert!(handled);
        assert!(run_main_context_until(|| {
            tabs.page_position(&page) != position
        }));
    }
    Ok(())
}
