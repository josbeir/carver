//! Display-backed browser note cards, favorites, move picker, and search coverage.
use super::*;

pub(super) fn browser_actions_should_import_and_create_a_note(
    fixture: &WindowFixture,
) -> Result<carver_sdk::NoteSummary, Box<dyn std::error::Error>> {
    let window = fixture.window.clone();
    let client = &fixture.client;
    let root = fixture.root()?;
    let sidebar = fixture.sidebar()?;
    let browser_view =
        widget_as::<adw::ToolbarView>(&root, "browser-surface").ok_or("browser view")?;
    let browser_controllers = browser_view.observe_controllers();
    let browser_shortcuts = (0..browser_controllers.n_items())
        .filter_map(|index| browser_controllers.item(index))
        .filter_map(|controller| controller.downcast::<gtk::EventControllerKey>().ok())
        .find(|controller| controller.name().as_deref() == Some("browser-shortcuts"))
        .ok_or("browser shortcuts")?;
    assert_eq!(
        browser_shortcuts.propagation_phase(),
        gtk::PropagationPhase::Capture
    );
    assert!(widget_as::<gtk::Button>(&root, "new-note-button").is_some());
    let browser_menu = widget_as::<gtk::MenuButton>(&root, "browser-menu-button")
        .ok_or("browser overflow menu")?;
    assert_eq!(
        browser_menu.menu_model().map(|model| model.n_items()),
        Some(1)
    );
    assert!(
        browser_menu
            .popover()
            .and_downcast::<gtk::PopoverMenu>()
            .is_some()
    );
    assert!(window.lookup_action("import-note").is_some());
    assert_eq!(
        crate::ui::dialogs::import_format_for_file(&gtk::gio::File::for_path("import.crv")),
        Some(carver_sdk::DocumentImportFormat::Carve)
    );
    assert_eq!(
        crate::ui::dialogs::import_format_for_file(&gtk::gio::File::for_path("import.md")),
        Some(carver_sdk::DocumentImportFormat::Markdown)
    );
    crate::ui::dialogs::read_import_file(
        &gtk::gio::File::for_path("unsupported.txt"),
        crate::mvu::AppDispatcher::default(),
    );
    assert_eq!(
        crate::ui::dialogs::import_message_from_bytes(
            carver_sdk::DocumentImportFormat::Markdown,
            b"# Imported",
        ),
        crate::mvu::NavigationMsg::ImportNote {
            format: carver_sdk::DocumentImportFormat::Markdown,
            source: String::from("# Imported"),
        }
    );
    assert!(matches!(
        crate::ui::dialogs::import_message_from_bytes(
            carver_sdk::DocumentImportFormat::Carve,
            &[0xff],
        ),
        crate::mvu::NavigationMsg::ImportFailed(_)
    ));
    crate::ui::dialogs::show_import_file_dialog(
        window.upcast_ref(),
        crate::mvu::AppDispatcher::default(),
    );
    let new_note_handled = browser_shortcuts.emit_by_name::<bool>(
        "key-pressed",
        &[
            &gtk::gdk::Key::n,
            &0_u32,
            &gtk::gdk::ModifierType::CONTROL_MASK,
        ],
    );
    assert!(new_note_handled);
    assert!(run_main_context_until(|| client
        .recent_notes(
            None,
            carver_sdk::PageRequest {
                limit: 10,
                offset: 0
            }
        )
        .is_ok_and(|notes| notes.items.len() == 1)));
    let note = client
        .recent_notes(
            None,
            carver_sdk::PageRequest {
                limit: 10,
                offset: 0,
            },
        )?
        .items
        .pop()
        .ok_or("created note")?;
    assert!(run_main_context_until(|| {
        sidebar_item_index(&sidebar, "all-notes-count").is_some()
    }));
    assert!(sidebar_select(&sidebar, "all-notes-count"));
    Ok(note)
}

pub(super) fn note_cards_should_group_and_favorite(
    fixture: &WindowFixture,
    note: &carver_sdk::NoteSummary,
) -> TestResult {
    let client = &fixture.client;
    let root = fixture.root()?;
    assert!(run_main_context_until(|| {
        find_widget(&root, &format!("note-category:{}", note.id)).is_some()
            && find_widget(&root, &format!("note-updated:{}", note.id)).is_some()
            && find_widget(&root, &format!("note-excerpt:{}", note.id)).is_none()
            && widget_as::<gtk::Label>(&root, &format!("note-category:{}", note.id))
                .is_some_and(|category| category.has_css_class("category-color-rose"))
    }));
    let updated = widget_as::<gtk::Label>(&root, &format!("note-updated:{}", note.id))
        .ok_or("note update time")?;
    assert!(
        run_main_context_until(|| { updated.width() > 0 && updated.layout_offsets().0 <= 1 }),
        "update text should stay beside the category when the card expands"
    );
    let note_menu = widget_as::<gtk::MenuButton>(&root, &format!("note-menu:{}", note.id))
        .ok_or("note actions")?;
    assert!(
        note_menu
            .popover()
            .and_then(|popover| popover.child())
            .and_then(|actions| {
                find_widget(&actions, &format!("export-note-button:{}", note.id))
            })
            .and_downcast::<gtk::Button>()
            .is_some(),
        "note actions should expose export"
    );
    let favorite_button = note_menu
        .popover()
        .and_then(|popover| popover.child())
        .and_then(|actions| {
            find_widget(&actions, &format!("favorite-note-menu-button:{}", note.id))
        })
        .and_downcast::<gtk::Button>()
        .ok_or("favorite note action")?;
    favorite_button.emit_clicked();
    assert!(run_main_context_until(|| client
        .note(note.id)
        .ok()
        .flatten()
        .is_some_and(|saved| saved.is_favorite)));
    let favorite_row = run_main_context_until(|| {
        find_widget(&root, &format!("favorite-note:{}", note.id)).is_some()
    });
    assert!(favorite_row);
    let favorite_row = find_widget(&root, &format!("favorite-note:{}", note.id))
        .and_downcast::<gtk::Box>()
        .ok_or("favorite note row")?;
    assert!(widget_as::<gtk::Image>(&root, "favorites-heading-icon").is_some());
    assert!(favorite_row.has_css_class("card"));
    assert!(favorite_row.has_css_class("note-card"));
    let favorite_menu =
        widget_as::<gtk::MenuButton>(favorite_row.upcast_ref(), &format!("note-menu:{}", note.id))
            .ok_or("favorite note actions")?;
    let favorite_remove = favorite_menu
        .popover()
        .and_then(|popover| popover.child())
        .and_then(|actions| {
            find_widget(&actions, &format!("favorite-note-menu-button:{}", note.id))
        })
        .and_downcast::<gtk::Button>()
        .ok_or("favorite card removal")?;
    favorite_remove.emit_clicked();
    assert!(run_main_context_until(|| client
        .note(note.id)
        .ok()
        .flatten()
        .is_some_and(|saved| !saved.is_favorite)));
    assert!(run_main_context_until(|| {
        find_widget(&root, &format!("favorite-note:{}", note.id)).is_none()
    }));
    let refreshed_note_menu =
        widget_as::<gtk::MenuButton>(&root, &format!("note-menu:{}", note.id))
            .ok_or("refreshed note actions")?;
    let refavorite_button = refreshed_note_menu
        .popover()
        .and_then(|popover| popover.child())
        .and_then(|actions| {
            find_widget(&actions, &format!("favorite-note-menu-button:{}", note.id))
        })
        .and_downcast::<gtk::Button>()
        .ok_or("re-favorite note action")?;
    refavorite_button.emit_clicked();
    assert!(run_main_context_until(|| client
        .note(note.id)
        .ok()
        .flatten()
        .is_some_and(|saved| saved.is_favorite)));
    assert!(run_main_context_until(|| {
        find_widget(&root, &format!("favorite-note:{}", note.id)).is_some()
    }));
    Ok(())
}

pub(super) fn move_picker_should_filter_and_move_notes(
    fixture: &WindowFixture,
    note: &carver_sdk::NoteSummary,
) -> TestResult {
    let client = &fixture.client;
    let category = &fixture.category;
    let destination = &fixture.destination;
    let root = fixture.root()?;
    let sidebar = fixture.sidebar()?;
    let refreshed_note_menu =
        widget_as::<gtk::MenuButton>(&root, &format!("note-menu:{}", note.id))
            .ok_or("second refreshed note actions")?;
    let move_button = refreshed_note_menu
        .popover()
        .and_then(|popover| popover.child())
        .and_then(|actions| find_widget(&actions, &format!("move-note-button:{}", note.id)))
        .and_downcast::<gtk::Button>()
        .ok_or("move note action")?;
    move_button.emit_clicked();
    let move_search =
        widget_as::<gtk::SearchEntry>(&root, "move-note-search").ok_or("move picker search")?;
    let source_row = find_widget(&root, &format!("move-note-category:{}", category.id))
        .and_downcast::<gtk::ListBoxRow>()
        .ok_or("current move category")?;
    assert!(
        source_row
            .child()
            .and_downcast::<gtk::Button>()
            .is_some_and(|button| !button.is_sensitive())
    );
    move_search.set_text("missing");
    assert!(run_main_context_until(|| {
        find_widget(&root, &format!("move-note-category:{}", destination.id)).is_none()
    }));
    move_search.set_text("pro");
    let destination_row = find_widget(&root, &format!("move-note-category:{}", destination.id))
        .and_downcast::<gtk::ListBoxRow>()
        .ok_or("filtered move destination")?;
    let destination_button = destination_row
        .child()
        .and_downcast::<gtk::Button>()
        .ok_or("move destination button")?;
    assert!(destination_button.is_sensitive());
    destination_button.emit_clicked();
    assert!(run_main_context_until(|| client
        .note(note.id)
        .ok()
        .flatten()
        .is_some_and(|moved| moved.category_id == destination.id)));
    assert!(run_main_context_until(|| {
        sidebar_item_index(&sidebar, &format!("category-count:{}", category.id)).is_some()
    }));
    Ok(())
}

pub(super) fn category_selection_should_show_empty_state(
    fixture: &WindowFixture,
    note: &carver_sdk::NoteSummary,
) -> TestResult {
    let category = &fixture.category;
    let root = fixture.root()?;
    let sidebar = fixture.sidebar()?;
    assert!(sidebar_select(
        &sidebar,
        &format!("category-count:{}", category.id)
    ));
    assert!(run_main_context_until(|| {
        widget_as::<gtk::Label>(&root, "browser-hero-title")
            .is_some_and(|title| title.text() == "Notes")
            && widget_as::<gtk::Button>(&root, "edit-selected-category-button")
                .is_some_and(|button| button.is_visible())
            && widget_as::<gtk::Button>(&root, "trash-selected-category-button")
                .is_some_and(|button| button.is_visible())
            && widget_as::<gtk::Box>(&root, "browser-category-empty-card")
                .is_some_and(|card| card.is_visible())
            && widget_as::<gtk::Button>(&root, "browser-category-empty-new-note-button")
                .is_some_and(|button| button.is_visible())
            && widget_as::<gtk::Stack>(&root, "browser-content-pages")
                .is_some_and(|pages| pages.visible_child_name().as_deref() == Some("contents"))
            && sidebar_item_index(&sidebar, &format!("category-actions:{}", category.id)).is_none()
            && find_widget(&root, &format!("note-category:{}", note.id)).is_none()
    }));
    let browser_scroll = widget_as::<gtk::ScrolledWindow>(&root, "browser-content-scroll")
        .ok_or("browser content scroll")?;
    let browser_clamp =
        widget_as::<adw::ClampScrollable>(&root, "browser-content-clamp").ok_or("browser clamp")?;
    assert!(
        browser_scroll
            .child()
            .is_some_and(|child| child == browser_clamp)
    );
    let note_list = widget_as::<gtk::ListView>(&root, "note-list").ok_or("note list")?;
    assert!(
        note_list
            .model()
            .and_downcast::<gtk::NoSelection>()
            .is_some(),
        "note cards should not retain a selected style after pointer hover"
    );
    assert!(
        note_list
            .parent()
            .is_some_and(|parent| parent == browser_clamp)
    );
    assert!(
        note_list.vadjustment().is_some(),
        "the virtual feed must expose its scroll adjustment for paging"
    );
    assert_eq!(
        note_list.vadjustment().as_ref(),
        Some(&browser_scroll.vadjustment()),
        "the viewport must forward its adjustment to the virtual feed"
    );
    Ok(())
}

pub(super) fn category_switch_should_retain_previous_browser(
    fixture: &WindowFixture,
    note: &carver_sdk::NoteSummary,
) -> TestResult {
    let destination = &fixture.destination;
    let root = fixture.root()?;
    let sidebar = fixture.sidebar()?;
    let note_list = fixture.note_list()?;
    let notes_without_favorites = Rc::new(Cell::new(false));
    let observed_rows = note_list.model().ok_or("note list model")?;
    let note_id = note.id;
    let observer = observed_rows.connect_items_changed({
        let root = root.clone();
        let notes_without_favorites = Rc::clone(&notes_without_favorites);
        move |_, _, _, added| {
            if added > 0
                && find_widget(&root, &format!("note:{note_id}")).is_some()
                && find_widget(&root, &format!("favorite-note:{note_id}")).is_none()
            {
                notes_without_favorites.set(true);
            }
        }
    });
    assert!(sidebar_select(
        &sidebar,
        &format!("category-count:{}", destination.id)
    ));
    assert!(run_main_context_until(|| {
        widget_as::<gtk::Label>(&root, "browser-hero-title")
            .is_some_and(|title| title.text() == "Projects")
            && find_widget(&root, &format!("note:{}", note.id)).is_some()
            && find_widget(&root, &format!("favorite-note:{}", note.id)).is_some()
            && widget_as::<gtk::Box>(&root, "favorites-section")
                .is_some_and(|section| section.is_visible())
            && find_widget(&root, "note-group:today").is_some()
            && find_widget(&root, &format!("note-category:{}", note.id)).is_none()
    }));
    observed_rows.disconnect(observer);
    assert!(
        !notes_without_favorites.get(),
        "category notes should never appear before their favorites"
    );
    assert!(run_main_context_until(|| {
        sidebar_item_index(&sidebar, "all-notes-count").is_some()
    }));
    let previous_favorite = find_widget(&root, &format!("favorite-note:{}", note.id))
        .ok_or("previous category favorite")?;
    assert!(sidebar_select(&sidebar, "all-notes-count"));
    assert_eq!(
        find_widget(&root, &format!("favorite-note:{}", note.id)),
        Some(previous_favorite),
        "fast category switches should retain the complete previous browser until loaded"
    );
    assert!(run_main_context_until(|| {
        widget_as::<gtk::Label>(&root, "browser-hero-title")
            .is_some_and(|title| title.text() == "All notes")
            && widget_as::<gtk::Button>(&root, "edit-selected-category-button").is_none()
            && find_widget(&root, &format!("note-category:{}", note.id)).is_some()
            && find_widget(&root, "note-group:today").is_some()
    }));
    let note_card = find_widget(&root, &format!("note:{}", note.id)).ok_or("note card")?;
    let card_surface = note_card
        .ancestor(gtk::ListBoxRow::static_type())
        .unwrap_or_else(|| note_card.clone());
    assert!(card_surface.has_css_class("card"));
    assert!(card_surface.has_css_class("activatable"));
    Ok(())
}

pub(super) fn browser_search_should_show_and_clear_empty_state(
    fixture: &WindowFixture,
    note: &carver_sdk::NoteSummary,
) -> TestResult {
    let client = &fixture.client;
    let root = fixture.root()?;
    let route_stack = fixture.route_stack()?;
    let source = fixture.source()?;
    let source_mode = fixture.source_mode()?;
    let sidebar_search_shortcut = fixture.sidebar_search_shortcut()?;
    source_mode.set_active(true);
    source
        .buffer()
        .set_text("# Searchable note\n\nBrowser grouping");
    assert!(run_main_context_until(|| client
        .note(note.id)
        .ok()
        .flatten()
        .is_some_and(
            |saved| saved.source == "# Searchable note\n\nBrowser grouping"
        )));
    let open_trash = widget_as::<gtk::Button>(&root, "open-trash-button").ok_or("open trash")?;
    open_trash.emit_clicked();
    assert!(run_main_context_until(|| {
        route_stack.visible_child_name().as_deref() == Some("trash")
    }));
    let trash_back =
        widget_as::<gtk::Button>(&root, "back-from-trash-button").ok_or("trash back")?;
    trash_back.emit_clicked();
    assert!(run_main_context_until(|| {
        route_stack.visible_child_name().as_deref() == Some("browser")
    }));
    let search_bar = widget_as::<gtk::SearchBar>(&root, "note-search-bar").ok_or("search bar")?;
    let search = widget_as::<gtk::SearchEntry>(&root, "note-search-entry").ok_or("search")?;
    let search_toggle =
        widget_as::<gtk::ToggleButton>(&root, "note-search-toggle").ok_or("search toggle")?;
    assert!(!search_bar.is_search_mode());
    assert!(!search_toggle.is_active());
    let _ = open_trash.grab_focus();
    let search_handled = sidebar_search_shortcut.emit_by_name::<bool>(
        "key-pressed",
        &[
            &gtk::gdk::Key::f,
            &0_u32,
            &gtk::gdk::ModifierType::CONTROL_MASK,
        ],
    );
    assert!(search_handled);
    assert!(run_main_context_until(|| {
        search_bar.is_search_mode() && search_toggle.is_active()
    }));
    assert!(run_main_context_until(|| {
        find_widget(&root, &format!("note:{}", note.id)).is_some()
            && find_widget(&root, "note-group:today").is_some()
    }));
    search.set_text("Searchable");
    assert!(run_main_context_until(|| {
        find_widget(&root, &format!("note:{}", note.id)).is_some()
            && find_widget(&root, "note-group:today").is_none()
    }));
    search.set_text("not-present");
    assert!(run_main_context_until(|| widget_as::<gtk::Box>(
        &root,
        "browser-search-empty-card"
    )
    .is_some_and(|card| card.is_visible())));
    // Refining the query to a matching term must clear the empty-state card.
    search.set_text("Searchable");
    assert!(run_main_context_until(|| {
        find_widget(&root, &format!("note:{}", note.id)).is_some()
            && find_widget(&root, "browser-search-empty-card").is_none()
    }));
    search.emit_stop_search();
    assert!(run_main_context_until(|| {
        !search_bar.is_search_mode() && !search_toggle.is_active() && search.text().is_empty()
    }));
    assert!(run_main_context_until(|| {
        find_widget(&root, "browser-search-empty-card").is_none()
    }));
    let restored_groups = run_main_context_until(|| {
        find_widget(&root, &format!("note:{}", note.id)).is_some()
            && find_widget(&root, "note-group:today").is_some()
    });
    if !restored_groups {
        return Err(format!(
            "search restore state: note={}, group={}, empty-card={}",
            find_widget(&root, &format!("note:{}", note.id)).is_some(),
            find_widget(&root, "note-group:today").is_some(),
            widget_as::<gtk::Box>(&root, "browser-search-empty-card")
                .is_some_and(|card| card.is_visible()),
        )
        .into());
    }
    Ok(())
}
