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
    assert!(run_main_context_until(|| {
        route_stack.visible_child_name().as_deref() == Some("editor")
    }));
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
    assert!(run_main_context_until(|| {
        route_stack.visible_child_name().as_deref() == Some("editor")
    }));
    let favorite =
        widget_as::<gtk::ToggleButton>(&root, "favorite-note-button").ok_or("favorite button")?;
    let back = widget_as::<gtk::Button>(&root, "back-to-notes-button").ok_or("back button")?;
    favorite.emit_clicked();
    favorite.emit_clicked();
    back.emit_clicked();
    assert_eq!(route_stack.visible_child_name().as_deref(), Some("editor"));
    assert!(sidebar_select(
        &sidebar,
        &format!("category-count:{}", category.id)
    ));
    assert_eq!(route_stack.visible_child_name().as_deref(), Some("editor"));
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
    window.close();
    Ok(())
}
