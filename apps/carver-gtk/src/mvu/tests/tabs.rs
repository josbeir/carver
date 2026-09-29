use super::*;
use crate::mvu::TabsMsg;

fn load_note(model: &mut AppModel, note_id: NoteId) -> crate::mvu::TabId {
    let _ = update(
        model,
        AppMsg::Editor(EditorMsg::Load {
            note_id,
            revision: Revision(1),
            source: String::from("# Note"),
        }),
    );
    match model.note_tab_for_note(note_id) {
        Some(tab_id) => tab_id,
        None => panic!("loading a note should open a tab"),
    }
}

/// A loaded note summary value, as the library would return it.
fn loaded_note(id: NoteId) -> Note {
    Note {
        id,
        category_id: CategoryId::new(),
        source: String::from("# Note"),
        title: String::from("Note"),
        plain_text: String::from("Note"),
        revision: Revision(1),
        is_favorite: false,
        created_at: OffsetDateTime::UNIX_EPOCH,
        updated_at: OffsetDateTime::UNIX_EPOCH,
        trashed_at: None,
    }
}

#[test]
fn loading_a_second_note_should_open_a_new_tab_and_stash_the_first() {
    let mut model = AppModel::new(&Config::default());
    let first = NoteId::new();
    let second = NoteId::new();
    let first_tab = load_note(&mut model, first);
    let second_tab = load_note(&mut model, second);

    assert_eq!(model.tabs.open.len(), 2);
    assert_eq!(model.tabs.active, Some(second_tab));
    assert_eq!(model.route, Route::Editor);
    assert_eq!(
        model.editor.as_ref().map(|document| document.note_id),
        Some(second)
    );
    assert!(
        model
            .tabs
            .background
            .get(&first_tab)
            .is_some_and(|document| document.note_id == first)
    );
}

#[test]
fn opening_an_already_open_note_should_focus_its_tab() {
    let mut model = AppModel::new(&Config::default());
    let first = NoteId::new();
    let second = NoteId::new();
    let first_tab = load_note(&mut model, first);
    let _second_tab = load_note(&mut model, second);

    let effects = update(
        &mut model,
        AppMsg::Tabs(TabsMsg::OpenNote {
            note_id: first,
            intent: NoteOpenIntent::Default,
        }),
    );

    assert_eq!(model.tabs.open.len(), 2);
    assert_eq!(model.tabs.active, Some(first_tab));
    assert_eq!(
        model.editor.as_ref().map(|document| document.note_id),
        Some(first)
    );
    assert!(
        effects
            .iter()
            .any(|effect| matches!(effect, Effect::FocusEditor { .. }))
    );
}

#[test]
fn opening_a_note_in_the_background_should_keep_the_active_tab() {
    let mut model = AppModel::new(&Config::default());
    let first = NoteId::new();
    let second = NoteId::new();
    let first_tab = load_note(&mut model, first);

    let effects = update(
        &mut model,
        AppMsg::Tabs(TabsMsg::OpenNote {
            note_id: second,
            intent: NoteOpenIntent::Background,
        }),
    );
    let (request_id, second_tab) = match effects.as_slice() {
        [
            Effect::LoadEditorNote {
                request_id, tab_id, ..
            },
        ] => (*request_id, *tab_id),
        _ => panic!("background open should start one load"),
    };
    assert_eq!(model.tabs.active, Some(first_tab));

    let _ = update(
        &mut model,
        AppMsg::Library(LibraryReply::EditorLoaded {
            request_id,
            tab_id: second_tab,
            result: Ok(Note {
                id: second,
                category_id: CategoryId::new(),
                source: String::from("# Second"),
                title: String::from("Second"),
                plain_text: String::from("Second"),
                revision: Revision(1),
                is_favorite: false,
                created_at: OffsetDateTime::UNIX_EPOCH,
                updated_at: OffsetDateTime::UNIX_EPOCH,
                trashed_at: None,
            }),
        }),
    );
    assert_eq!(model.tabs.active, Some(first_tab));
    assert!(
        model
            .tabs
            .background
            .get(&second_tab)
            .is_some_and(|document| document.note_id == second)
    );
}

#[test]
fn closing_the_active_tab_should_activate_a_neighbor() {
    let mut model = AppModel::new(&Config::default());
    let first = NoteId::new();
    let second = NoteId::new();
    let first_tab = load_note(&mut model, first);
    let second_tab = load_note(&mut model, second);

    let _ = update(&mut model, AppMsg::Tabs(TabsMsg::Close(second_tab)));

    assert_eq!(model.tabs.open.len(), 1);
    assert_eq!(model.tabs.open[0].id, first_tab);
    assert_eq!(model.tabs.active, Some(first_tab));
    assert_eq!(
        model.editor.as_ref().map(|document| document.note_id),
        Some(first)
    );
}

#[test]
fn closing_the_last_tab_should_return_to_the_notes_list() {
    let mut model = AppModel::new(&Config::default());
    let note = NoteId::new();
    let tab = load_note(&mut model, note);

    let effects = update(&mut model, AppMsg::Tabs(TabsMsg::Close(tab)));

    assert!(model.tabs.open.is_empty());
    assert_eq!(model.tabs.active, None);
    assert_eq!(model.route, Route::Browser);
    assert!(model.editor.is_none());
    assert!(
        effects
            .iter()
            .any(|effect| matches!(effect, Effect::LoadBrowser { .. }))
    );
}

#[test]
fn closing_a_dirty_tab_should_save_it_without_keeping_the_tab() {
    let mut model = AppModel::new(&Config::default());
    let tab = open_and_load(&mut model, NoteId::new());
    let _ = update(
        &mut model,
        AppMsg::Editor(EditorMsg::SourceChanged(String::from("Dirty"))),
    );

    let effects = update(&mut model, AppMsg::Tabs(TabsMsg::Close(tab)));
    let Some(request) = effects.iter().find_map(|effect| match effect {
        Effect::SaveNote { request } => Some(request.clone()),
        _ => None,
    }) else {
        panic!("closing a dirty tab should start a save");
    };
    // The tab is gone immediately; the document keeps saving in the background.
    assert!(model.tabs.open.is_empty());
    assert!(model.tabs.active.is_none());
    assert_eq!(model.tabs.closing.len(), 1);

    let _ = update(
        &mut model,
        AppMsg::Library(LibraryReply::EditorSaved {
            request,
            result: Ok(Revision(2)),
            move_error: None,
        }),
    );
    assert!(model.tabs.closing.is_empty());
}

#[test]
fn a_failed_final_save_for_a_closed_tab_should_reopen_the_draft() {
    let mut model = AppModel::new(&Config::default());
    let note_id = NoteId::new();
    let tab = open_and_load(&mut model, note_id);
    let _ = update(
        &mut model,
        AppMsg::Editor(EditorMsg::SourceChanged(String::from("Dirty"))),
    );
    let effects = update(&mut model, AppMsg::Tabs(TabsMsg::Close(tab)));
    let Some(request) = effects.iter().find_map(|effect| match effect {
        Effect::SaveNote { request } => Some(request.clone()),
        _ => None,
    }) else {
        panic!("closing a dirty tab should start a save");
    };

    let _ = update(
        &mut model,
        AppMsg::Library(LibraryReply::EditorSaved {
            request,
            result: Err(UiError::new("disk full")),
            move_error: None,
        }),
    );

    assert!(model.tabs.closing.is_empty());
    assert_eq!(
        model.notice.as_ref().map(|error| error.message.as_str()),
        Some("Could not save note: disk full")
    );
    // The unsaved draft is reachable again rather than dropped.
    assert!(
        model.tabs.open.iter().any(|open| open.note_id == note_id),
        "a failed close-time save should reopen the draft as a tab"
    );
}

#[test]
fn reordering_tabs_should_update_the_model_order() {
    let mut model = AppModel::new(&Config::default());
    let first_tab = load_note(&mut model, NoteId::new());
    let second_tab = load_note(&mut model, NoteId::new());

    let _ = update(
        &mut model,
        AppMsg::Tabs(TabsMsg::Reordered {
            tab_id: second_tab,
            position: 0,
        }),
    );

    assert_eq!(
        model.tabs.open.iter().map(|tab| tab.id).collect::<Vec<_>>(),
        vec![second_tab, first_tab]
    );
}

/// Opens a note through the tab flow and completes its asynchronous load.
fn open_and_load(model: &mut AppModel, note_id: NoteId) -> crate::mvu::TabId {
    let effects = update(
        model,
        AppMsg::Tabs(TabsMsg::OpenNote {
            note_id,
            intent: NoteOpenIntent::Default,
        }),
    );
    let (request_id, tab_id) = match effects.as_slice() {
        [
            Effect::LoadEditorNote {
                request_id, tab_id, ..
            },
        ] => (*request_id, *tab_id),
        _ => panic!("opening a note should start one load"),
    };
    let _ = update(
        model,
        AppMsg::Library(LibraryReply::EditorLoaded {
            request_id,
            tab_id,
            result: Ok(Note {
                id: note_id,
                category_id: CategoryId::new(),
                source: String::from("# Note"),
                title: String::from("Note"),
                plain_text: String::from("Note"),
                revision: Revision(1),
                is_favorite: false,
                created_at: OffsetDateTime::UNIX_EPOCH,
                updated_at: OffsetDateTime::UNIX_EPOCH,
                trashed_at: None,
            }),
        }),
    );
    tab_id
}

#[test]
fn creating_a_note_should_open_a_new_tab() {
    let mut model = AppModel::new(&Config::default());
    let note_id = NoteId::new();
    let _ = update(
        &mut model,
        AppMsg::Library(LibraryReply::NoteCreated {
            result: Ok(Note {
                id: note_id,
                category_id: CategoryId::new(),
                source: String::from("# Fresh"),
                title: String::from("Fresh"),
                plain_text: String::from("Fresh"),
                revision: Revision(1),
                is_favorite: false,
                created_at: OffsetDateTime::UNIX_EPOCH,
                updated_at: OffsetDateTime::UNIX_EPOCH,
                trashed_at: None,
            }),
        }),
    );

    assert_eq!(model.tabs.open.len(), 1);
    assert_eq!(model.tabs.open[0].note_id, note_id);
    assert_eq!(model.tabs.active, Some(model.tabs.open[0].id));
    assert_eq!(model.route, Route::Editor);
    assert_eq!(
        model.editor.as_ref().map(|document| document.note_id),
        Some(note_id)
    );
}

#[test]
fn back_should_return_to_the_previously_active_tab() {
    let mut model = AppModel::new(&Config::default());
    let first = NoteId::new();
    let second = NoteId::new();
    let first_tab = open_and_load(&mut model, first);
    let second_tab = open_and_load(&mut model, second);
    assert_eq!(model.tabs.active, Some(second_tab));

    // Back returns to the note the current one was opened from.
    let _ = update(&mut model, AppMsg::Editor(EditorMsg::BackRequested));
    assert_eq!(model.tabs.active, Some(first_tab));
    assert_eq!(model.route, Route::Editor);

    // Back again falls through to the pinned Notes list.
    let _ = update(&mut model, AppMsg::Editor(EditorMsg::BackRequested));
    assert_eq!(model.tabs.active, None);
    assert_eq!(model.route, Route::Browser);
}

#[test]
fn closing_a_tab_should_drop_it_from_the_back_history() {
    let mut model = AppModel::new(&Config::default());
    let first_tab = open_and_load(&mut model, NoteId::new());
    let second_tab = open_and_load(&mut model, NoteId::new());

    let _ = update(&mut model, AppMsg::Tabs(TabsMsg::Close(first_tab)));
    // The closed tab must not be reachable through Back.
    let _ = update(&mut model, AppMsg::Editor(EditorMsg::BackRequested));
    assert_ne!(model.tabs.active, Some(first_tab));
    assert_ne!(model.tabs.active, Some(second_tab));
    assert_eq!(model.tabs.active, None);
    assert_eq!(model.tabs.open.len(), 1);
    assert_eq!(model.tabs.open[0].id, second_tab);
}

#[test]
fn default_intent_should_reuse_the_active_tab_when_configured() {
    let mut config = Config::default();
    config.editor.note_open_behavior = carver_config::NoteOpenBehavior::CurrentTab;
    let mut model = AppModel::new(&config);
    let first = NoteId::new();
    let second = NoteId::new();
    let _first_tab = open_and_load(&mut model, first);

    let effects = update(
        &mut model,
        AppMsg::Tabs(TabsMsg::OpenNote {
            note_id: second,
            intent: NoteOpenIntent::Default,
        }),
    );

    // Replacing the active tab leaves a single tab showing the new note.
    assert_eq!(model.tabs.open.len(), 1);
    assert_eq!(model.tabs.open[0].note_id, second);
    assert!(
        effects
            .iter()
            .any(|effect| matches!(effect, Effect::LoadEditorNote { .. }))
    );
}

#[test]
fn default_intent_should_reuse_the_last_active_tab_from_the_notes_list() {
    let mut config = Config::default();
    config.editor.note_open_behavior = carver_config::NoteOpenBehavior::CurrentTab;
    let mut model = AppModel::new(&config);
    let first = NoteId::new();
    let second = NoteId::new();
    let first_tab = open_and_load(&mut model, first);
    // Show the Notes list; the tab is remembered as the last active one.
    let _ = update(&mut model, AppMsg::Tabs(TabsMsg::ActivateNotes));
    assert_eq!(model.tabs.active, None);

    let _ = update(
        &mut model,
        AppMsg::Tabs(TabsMsg::OpenNote {
            note_id: second,
            intent: NoteOpenIntent::Default,
        }),
    );

    assert!(!model.tabs.open.iter().any(|tab| tab.id == first_tab));
    assert_eq!(model.tabs.open.len(), 1);
    assert_eq!(model.tabs.open[0].note_id, second);
}

#[test]
fn forced_new_tab_intent_should_ignore_the_current_tab_preference() {
    let mut config = Config::default();
    config.editor.note_open_behavior = carver_config::NoteOpenBehavior::CurrentTab;
    let mut model = AppModel::new(&config);
    let first = NoteId::new();
    let second = NoteId::new();
    let first_tab = open_and_load(&mut model, first);

    let effects = update(
        &mut model,
        AppMsg::Tabs(TabsMsg::OpenNote {
            note_id: second,
            intent: NoteOpenIntent::NewTab,
        }),
    );

    assert_eq!(model.tabs.open.len(), 2);
    assert!(model.tabs.open.iter().any(|tab| tab.id == first_tab));
    assert!(
        effects
            .iter()
            .any(|effect| matches!(effect, Effect::LoadEditorNote { .. }))
    );
}

#[test]
fn reactivating_a_note_tab_after_showing_notes_should_restore_the_editor() {
    let mut model = AppModel::new(&Config::default());
    let note = NoteId::new();
    let tab = open_and_load(&mut model, note);
    // Showing the Notes list keeps the remembered active tab.
    let _ = update(&mut model, AppMsg::Navigation(NavigationMsg::ShowBrowser));
    assert_eq!(model.route, Route::Browser);
    assert_eq!(model.tabs.active, Some(tab));

    let effects = update(
        &mut model,
        AppMsg::Tabs(TabsMsg::OpenNote {
            note_id: note,
            intent: NoteOpenIntent::Default,
        }),
    );

    assert_eq!(model.route, Route::Editor);
    assert_eq!(model.tabs.active, Some(tab));
    assert!(
        effects
            .iter()
            .any(|effect| matches!(effect, Effect::FocusEditor { .. }))
    );
}

#[test]
fn closing_a_tab_with_an_unresolved_external_change_should_keep_it_open() {
    let mut model = AppModel::new(&Config::default());
    let tab = open_and_load(&mut model, NoteId::new());
    if let Some(document) = model.editor.as_mut() {
        document.external_change = Some(crate::mvu::model::ExternalChange::Edited(Revision(2)));
    }

    let effects = update(&mut model, AppMsg::Tabs(TabsMsg::Close(tab)));

    assert!(
        effects
            .iter()
            .any(|effect| matches!(effect, Effect::ShowExternalEdit { .. }))
    );
    assert!(model.note_tab(tab).is_some());
    assert_eq!(model.tabs.active, Some(tab));
}

#[test]
fn a_failed_favorite_on_a_background_tab_should_clear_its_in_flight_state() {
    let mut model = AppModel::new(&Config::default());
    let note_id = NoteId::new();
    let first = open_and_load(&mut model, note_id);
    let _second = open_and_load(&mut model, NoteId::new());
    if let Some(document) = model.tabs.background.get_mut(&first) {
        document.favorite_mutation_in_flight = true;
    }

    let _ = update(
        &mut model,
        AppMsg::Library(LibraryReply::FavoriteChanged {
            action: ActionKey::SetNoteFavorite(note_id),
            result: Err(UiError::new("failed")),
        }),
    );

    assert!(
        model
            .tabs
            .background
            .get(&first)
            .is_some_and(|document| !document.favorite_mutation_in_flight)
    );
}

#[test]
fn storing_an_asset_for_a_background_tab_should_schedule_its_save() {
    let mut model = AppModel::new(&Config::default());
    let _first = open_and_load(&mut model, NoteId::new());
    let _second = open_and_load(&mut model, NoteId::new());
    let Some(session) = model
        .tabs
        .background
        .values()
        .next()
        .map(|document| document.session)
    else {
        panic!("first tab should be background");
    };

    let effects = update(
        &mut model,
        AppMsg::Library(LibraryReply::EditorAssetStored {
            image: true,
            session,
            alt: "Photo".to_owned(),
            source_target: None,
            result: Ok("assets/photo.png".to_owned()),
        }),
    );

    assert!(effects.iter().any(|effect| matches!(
        effect,
        Effect::ScheduleEditorSave { session: scheduled, .. } if *scheduled == session
    )));
}

#[test]
fn a_link_reply_for_a_background_tab_should_settle_its_loading_state() {
    let mut model = AppModel::new(&Config::default());
    let first_id = NoteId::new();
    let first_tab = open_and_load(&mut model, first_id);

    // Open a second note in the background so its links start idle.
    let second_id = NoteId::new();
    let effects = update(
        &mut model,
        AppMsg::Tabs(TabsMsg::OpenNote {
            note_id: second_id,
            intent: NoteOpenIntent::Background,
        }),
    );
    let (load_request, second_tab) = match effects.as_slice() {
        [
            Effect::LoadEditorNote {
                request_id, tab_id, ..
            },
        ] => (*request_id, *tab_id),
        _ => panic!("a background open should start one load"),
    };
    let _ = update(
        &mut model,
        AppMsg::Library(LibraryReply::EditorLoaded {
            request_id: load_request,
            tab_id: second_tab,
            result: Ok(loaded_note(second_id)),
        }),
    );

    // Activating the tab starts its link load, then switching away leaves it in flight.
    let effects = update(&mut model, AppMsg::Tabs(TabsMsg::Activate(second_tab)));
    let request_id = effects
        .iter()
        .find_map(|effect| match effect {
            Effect::LoadNoteLinks {
                request_id,
                note_id,
            } if *note_id == second_id => Some(*request_id),
            _ => None,
        })
        .unwrap_or_else(|| panic!("activating a tab should load its links"));
    let _ = update(&mut model, AppMsg::Tabs(TabsMsg::Activate(first_tab)));

    let _ = update(
        &mut model,
        AppMsg::Library(LibraryReply::NoteLinksLoaded {
            request_id,
            note_id: second_id,
            result: Ok(carver_sdk::NoteLinks::default()),
        }),
    );

    let Some(document) = model.document_for_note(second_id) else {
        panic!("background document should remain");
    };
    assert!(
        matches!(document.links.state, LoadState::Ready(_)),
        "a reply for an inactive tab must still settle its loading state"
    );
}

#[test]
fn editing_the_page_title_should_update_the_tab_title() {
    let mut model = AppModel::new(&Config::default());
    let tab = open_and_load(&mut model, NoteId::new());
    assert_eq!(
        model.note_tab(tab).map(|tab| tab.title.as_str()),
        Some("Note")
    );

    let _ = update(
        &mut model,
        AppMsg::Editor(EditorMsg::SourceChanged(String::from("# Renamed"))),
    );

    assert_eq!(
        model.note_tab(tab).map(|tab| tab.title.as_str()),
        Some("Renamed")
    );
}

#[test]
fn a_background_open_of_an_open_note_should_do_nothing() {
    let mut model = AppModel::new(&Config::default());
    let note = NoteId::new();
    let tab = open_and_load(&mut model, note);

    let effects = update(
        &mut model,
        AppMsg::Tabs(TabsMsg::OpenNote {
            note_id: note,
            intent: NoteOpenIntent::Background,
        }),
    );

    assert!(effects.is_empty());
    assert_eq!(model.tabs.active, Some(tab));
}

#[test]
fn a_background_tab_title_should_follow_its_document() {
    let mut model = AppModel::new(&Config::default());
    let first = open_and_load(&mut model, NoteId::new());
    let _second = open_and_load(&mut model, NoteId::new());
    if let Some(document) = model.tabs.background.get_mut(&first) {
        document.source_changed(String::from("# Background renamed"));
    }

    // Any message re-syncs titles from the documents.
    let _ = update(&mut model, AppMsg::Editor(EditorMsg::ThemeChanged));

    assert_eq!(
        model.note_tab(first).map(|tab| tab.title.as_str()),
        Some("Background renamed")
    );
}

#[test]
fn activating_a_dirty_background_tab_should_schedule_a_save() {
    let mut model = AppModel::new(&Config::default());
    let first = open_and_load(&mut model, NoteId::new());
    let _second = open_and_load(&mut model, NoteId::new());
    if let Some(document) = model.tabs.background.get_mut(&first) {
        document.source_changed(String::from("# Dirty"));
    }

    let effects = update(&mut model, AppMsg::Tabs(TabsMsg::Activate(first)));

    assert!(
        effects
            .iter()
            .any(|effect| matches!(effect, Effect::ScheduleEditorSave { .. })),
        "a dirty background tab should autosave when activated"
    );
}

#[test]
fn a_failed_load_should_remove_the_tab_and_activate_a_neighbor() {
    let mut model = AppModel::new(&Config::default());
    let first = open_and_load(&mut model, NoteId::new());
    let effects = update(
        &mut model,
        AppMsg::Tabs(TabsMsg::OpenNote {
            note_id: NoteId::new(),
            intent: NoteOpenIntent::Default,
        }),
    );
    let (request_id, tab_id) = match effects.as_slice() {
        [
            Effect::LoadEditorNote {
                request_id, tab_id, ..
            },
        ] => (*request_id, *tab_id),
        _ => panic!("opening a note should start one load"),
    };

    let _ = update(
        &mut model,
        AppMsg::Library(LibraryReply::EditorLoaded {
            request_id,
            tab_id,
            result: Err(UiError::new("missing")),
        }),
    );

    assert!(model.note_tab(tab_id).is_none());
    assert_eq!(model.tabs.active, Some(first));
}

#[test]
fn tab_history_should_be_capped() {
    let mut model = AppModel::new(&Config::default());
    for _ in 0..70 {
        let _ = open_and_load(&mut model, NoteId::new());
    }
    assert!(
        model.tabs.history.len() <= 64,
        "tab history must stay bounded"
    );
}

#[test]
fn closing_a_conflicted_background_tab_should_show_the_resolver() {
    let mut model = AppModel::new(&Config::default());
    let first = open_and_load(&mut model, NoteId::new());
    let _second = open_and_load(&mut model, NoteId::new());
    if let Some(document) = model.tabs.background.get_mut(&first) {
        document.external_change = Some(crate::mvu::model::ExternalChange::Edited(Revision(2)));
    }

    let effects = update(&mut model, AppMsg::Tabs(TabsMsg::Close(first)));

    assert!(
        effects
            .iter()
            .any(|effect| matches!(effect, Effect::ShowExternalEdit { .. }))
    );
    assert!(model.note_tab(first).is_some());
    // The conflicted tab is brought forward so the resolver targets a live document.
    assert_eq!(model.tabs.active, Some(first));
}

#[test]
fn current_tab_intent_should_not_replace_a_tab_that_refuses_to_close() {
    let mut config = Config::default();
    config.editor.note_open_behavior = carver_config::NoteOpenBehavior::CurrentTab;
    let mut model = AppModel::new(&config);
    let first = open_and_load(&mut model, NoteId::new());
    if let Some(document) = model.editor.as_mut() {
        document.external_change = Some(crate::mvu::model::ExternalChange::Edited(Revision(2)));
    }

    let effects = update(
        &mut model,
        AppMsg::Tabs(TabsMsg::OpenNote {
            note_id: NoteId::new(),
            intent: NoteOpenIntent::Default,
        }),
    );

    assert!(
        effects
            .iter()
            .any(|effect| matches!(effect, Effect::ShowExternalEdit { .. }))
    );
    assert_eq!(model.tabs.open.len(), 1);
    assert_eq!(model.tabs.active, Some(first));
}

#[test]
fn closing_a_tab_with_a_pending_asset_should_keep_it_until_the_store_completes() {
    let mut model = AppModel::new(&Config::default());
    let tab = open_and_load(&mut model, NoteId::new());
    let Some(session) = model.editor.as_ref().map(|document| document.session) else {
        panic!("editor session");
    };
    if let Some(document) = model.editor.as_mut() {
        document.pending_assets = 1;
    }

    let _ = update(&mut model, AppMsg::Tabs(TabsMsg::Close(tab)));
    assert!(model.tabs.closing.contains_key(&session));

    let effects = update(
        &mut model,
        AppMsg::Library(LibraryReply::EditorAssetStored {
            image: true,
            session,
            alt: "Photo".to_owned(),
            source_target: None,
            result: Ok("assets/photo.png".to_owned()),
        }),
    );
    assert!(effects.iter().any(|effect| matches!(
        effect,
        Effect::ScheduleEditorSave { session: scheduled, .. } if *scheduled == session
    )));
}

#[test]
fn activating_a_background_tab_should_refresh_it() {
    let mut model = AppModel::new(&Config::default());
    let first = open_and_load(&mut model, NoteId::new());
    let _second = open_and_load(&mut model, NoteId::new());

    let effects = update(&mut model, AppMsg::Tabs(TabsMsg::Activate(first)));

    assert!(
        effects
            .iter()
            .any(|effect| matches!(effect, Effect::RefreshEditorNote { .. })),
        "restoring a background tab should refresh it from the library"
    );
}

#[test]
fn next_and_previous_should_wrap_the_strip_including_the_root_page() {
    let mut model = AppModel::new(&Config::default());
    let first = open_and_load(&mut model, NoteId::new());
    let second = open_and_load(&mut model, NoteId::new());

    // From the last note tab, next wraps to the root (Notes).
    let _ = update(&mut model, AppMsg::Tabs(TabsMsg::ActivateNext));
    assert_eq!(model.tabs.active, None);
    assert_eq!(model.route, Route::Browser);
    // Next again moves into the first note tab.
    let _ = update(&mut model, AppMsg::Tabs(TabsMsg::ActivateNext));
    assert_eq!(model.tabs.active, Some(first));
    // Previous from the first note tab wraps back to the root.
    let _ = update(&mut model, AppMsg::Tabs(TabsMsg::ActivatePrevious));
    assert_eq!(model.tabs.active, None);
    // Previous from the root goes to the last note tab.
    let _ = update(&mut model, AppMsg::Tabs(TabsMsg::ActivatePrevious));
    assert_eq!(model.tabs.active, Some(second));
}

#[test]
fn next_from_a_note_opened_from_a_base_should_return_to_the_base() {
    let mut model = AppModel::new(&Config::default());
    let base_id = BaseId::new();
    model.route = Route::Base;
    model.bases.selected = Some(base_id);
    let _note = open_and_load(&mut model, NoteId::new());

    let _ = update(&mut model, AppMsg::Tabs(TabsMsg::ActivateNext));

    assert_eq!(model.tabs.active, None);
    assert_eq!(model.route, Route::Base);
    assert_eq!(model.bases.selected, Some(base_id));
}

#[test]
fn navigating_tabs_should_not_record_back_history() {
    let mut model = AppModel::new(&Config::default());
    let _first = open_and_load(&mut model, NoteId::new());
    let _second = open_and_load(&mut model, NoteId::new());
    let before = model.tabs.history.clone();

    let _ = update(&mut model, AppMsg::Tabs(TabsMsg::ActivateNext));
    let _ = update(&mut model, AppMsg::Tabs(TabsMsg::ActivatePrevious));

    assert_eq!(model.tabs.history, before);
}

#[test]
fn moving_an_open_note_should_rebase_its_document_revision() {
    let mut model = AppModel::new(&Config::default());
    let note_id = NoteId::new();
    let _tab = open_and_load(&mut model, note_id);
    let category_id = CategoryId::new();

    let _ = update(
        &mut model,
        AppMsg::Library(LibraryReply::NoteMoved {
            action: ActionKey::MoveNote {
                note_id,
                source_category_id: CategoryId::new(),
            },
            result: Ok(Note {
                id: note_id,
                category_id,
                source: String::from("# Note"),
                title: String::from("Note"),
                plain_text: String::from("Note"),
                revision: Revision(2),
                is_favorite: false,
                created_at: OffsetDateTime::UNIX_EPOCH,
                updated_at: OffsetDateTime::UNIX_EPOCH,
                trashed_at: None,
            }),
        }),
    );

    let document = model.editor.as_ref();
    assert_eq!(
        document.map(|document| document.revision),
        Some(Revision(2))
    );
    assert_eq!(
        document.map(|document| document.category_id),
        Some(category_id)
    );
}

#[test]
fn closing_the_active_tab_message_should_close_it() {
    let mut model = AppModel::new(&Config::default());
    let first = open_and_load(&mut model, NoteId::new());
    let second = open_and_load(&mut model, NoteId::new());
    assert_eq!(model.tabs.active, Some(second));

    let _ = update(&mut model, AppMsg::Tabs(TabsMsg::CloseActive));

    assert!(model.note_tab(second).is_none());
    assert!(model.note_tab(first).is_some());
}

#[test]
fn opening_a_note_from_a_base_should_attribute_the_tab_to_that_base() {
    let mut model = AppModel::new(&Config::default());
    let base_id = BaseId::new();
    model.route = Route::Base;
    model.bases.selected = Some(base_id);
    let note_id = NoteId::new();

    let tab_id = open_and_load(&mut model, note_id);

    assert_eq!(
        model.note_tab(tab_id).map(|tab| tab.origin),
        Some(TabOrigin::Base(base_id))
    );
}

#[test]
fn closing_tabs_from_a_base_should_keep_tabs_from_other_surfaces() {
    let mut model = AppModel::new(&Config::default());
    let base_id = BaseId::new();
    model.route = Route::Browser;
    let browser_tab = open_and_load(&mut model, NoteId::new());
    model.route = Route::Base;
    model.bases.selected = Some(base_id);
    let base_tab = open_and_load(&mut model, NoteId::new());
    assert_eq!(
        model.note_tab(base_tab).map(|tab| tab.origin),
        Some(TabOrigin::Base(base_id))
    );

    let _ = update(
        &mut model,
        AppMsg::Tabs(TabsMsg::CloseOrigin(TabOrigin::Base(base_id))),
    );

    assert!(model.note_tab(base_tab).is_none());
    assert!(model.note_tab(browser_tab).is_some());
}
