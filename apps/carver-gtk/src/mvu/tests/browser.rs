use super::*;

#[test]
fn stale_browser_reply_should_not_replace_a_newer_request() {
    let mut model = AppModel::new(&Config::default());
    let first = update(&mut model, AppMsg::Browser(BrowserMsg::Reload));
    let timer = update(
        &mut model,
        AppMsg::Browser(BrowserMsg::SearchChanged("new query".to_owned())),
    );
    let first_request = match first.as_slice() {
        [Effect::LoadBrowser { request_id, .. }] => *request_id,
        _ => panic!("browser reload should produce one browser effect"),
    };
    let timer_id = match timer.as_slice() {
        [Effect::ScheduleSearch { timer_id }] => *timer_id,
        _ => panic!("search should schedule one timer"),
    };

    assert!(
        update(
            &mut model,
            AppMsg::Browser(BrowserMsg::SearchTimerFired(timer_id)),
        )
        .is_empty()
    );

    let follow_up = update(
        &mut model,
        AppMsg::Library(LibraryReply::BrowserLoaded {
            favorites: Ok(Vec::new()),
            request_id: first_request,
            result: Ok(Vec::new()),
        }),
    );
    let second_request = match follow_up.as_slice() {
        [Effect::LoadBrowser { request_id, .. }] => *request_id,
        _ => panic!("a queued reload should start after the active request"),
    };
    assert_eq!(
        model.browser.notes.state,
        LoadState::Loading(second_request)
    );

    let _ = update(
        &mut model,
        AppMsg::Library(LibraryReply::BrowserLoaded {
            favorites: Ok(Vec::new()),
            request_id: second_request,
            result: Err(UiError::new("search failed")),
        }),
    );
    assert_eq!(
        model.browser.notes.state,
        LoadState::Failed(UiError::new("search failed"))
    );
}

#[test]
fn browser_loading_indicator_should_wait_for_its_delay_and_clear_after_loading() {
    let mut model = AppModel::new(&Config::default());
    let initial_load = update(&mut model, AppMsg::Browser(BrowserMsg::Reload));
    let initial_request = match initial_load.as_slice() {
        [Effect::LoadBrowser { request_id, .. }] => *request_id,
        _ => panic!("browser reload should produce one browser effect"),
    };
    let _ = update(
        &mut model,
        AppMsg::Library(LibraryReply::BrowserLoaded {
            favorites: Ok(Vec::new()),
            request_id: initial_request,
            result: Ok(Vec::new()),
        }),
    );

    let reload = update(&mut model, AppMsg::Browser(BrowserMsg::Reload));
    let request_id = match reload.as_slice() {
        [Effect::LoadBrowser { request_id, .. }] => *request_id,
        _ => panic!("browser reload should produce one browser effect"),
    };
    assert_eq!(model.browser.loading_indicator_request, Some(request_id));
    assert!(!model.browser.loading_indicator_visible);

    let indicator_effects = update(
        &mut model,
        AppMsg::Browser(BrowserMsg::LoadingIndicatorElapsed(request_id)),
    );
    assert!(indicator_effects.is_empty());
    assert!(model.browser.loading_indicator_visible);

    let _ = update(
        &mut model,
        AppMsg::Library(LibraryReply::BrowserLoaded {
            favorites: Ok(Vec::new()),
            request_id,
            result: Ok(Vec::new()),
        }),
    );
    assert_eq!(model.browser.loading_indicator_request, None);
    assert!(!model.browser.loading_indicator_visible);
}

#[test]
fn opening_note_search_should_update_only_the_browser_snapshot() {
    let mut model = AppModel::new(&Config::default());

    let effects = update(&mut model, AppMsg::Browser(BrowserMsg::SearchOpened));

    assert!(effects.is_empty());
    assert!(model.browser.search_open);
}

#[test]
fn browser_search_shortcut_should_open_search_from_the_browser_route() {
    let mut model = AppModel::new(&Config::default());

    let effects = update(
        &mut model,
        AppMsg::Browser(BrowserMsg::SearchShortcutRequested),
    );

    assert!(effects.is_empty());
    assert!(model.browser.search_open);
}

#[test]
fn browser_search_shortcut_should_not_change_the_editor_route() {
    let mut model = AppModel::new(&Config::default());
    model.route = Route::Editor;
    let effects = update(
        &mut model,
        AppMsg::Browser(BrowserMsg::SearchShortcutRequested),
    );

    assert!(effects.is_empty());
    assert!(!model.browser.search_open);
}

#[test]
fn closing_note_search_should_clear_the_query_and_reload_the_browser() {
    let mut model = AppModel::new(&Config::default());
    let _ = update(
        &mut model,
        AppMsg::Browser(BrowserMsg::SearchChanged("roadmap".to_owned())),
    );
    model.browser.search_open = true;

    let effects = update(
        &mut model,
        AppMsg::Browser(BrowserMsg::SearchVisibilityChanged(false)),
    );

    assert_eq!(
        effects,
        vec![Effect::LoadBrowser {
            request_id: RequestId(1),
            category_id: None,
            query: String::new(),
        }]
    );
    assert!(!model.browser.search_open);
    assert!(model.browser.search_query.is_empty());
    assert!(model.browser.search_timer.is_none());
}

#[test]
fn closing_note_search_should_reload_after_the_native_entry_clears_its_text() {
    let mut model = AppModel::new(&Config::default());
    model.browser.search_open = true;

    let effects = update(
        &mut model,
        AppMsg::Browser(BrowserMsg::SearchVisibilityChanged(false)),
    );

    assert_eq!(
        effects,
        vec![Effect::LoadBrowser {
            request_id: RequestId(1),
            category_id: None,
            query: String::new(),
        }]
    );
}

#[test]
fn selecting_a_category_should_reload_the_browser_for_that_category() {
    let mut model = AppModel::new(&Config::default());
    let category_id = CategoryId::new();

    let effects = update(
        &mut model,
        AppMsg::Navigation(NavigationMsg::SelectCategory(Some(category_id))),
    );

    assert_eq!(model.route, Route::Browser);
    assert_eq!(model.selected_category, Some(category_id));
    assert_eq!(
        effects,
        vec![Effect::LoadBrowser {
            request_id: RequestId(1),
            category_id: Some(category_id),
            query: String::new(),
        }]
    );
}

#[test]
fn loaded_category_should_publish_notes_and_favorites_together() {
    let mut model = AppModel::new(&Config::default());
    let category_id = CategoryId::new();
    let favorite = carver_sdk::NoteSummary {
        id: NoteId::new(),
        category_id,
        category_name: String::from("Projects"),
        title: String::from("Favorite"),
        excerpt: String::new(),
        revision: Revision(1),
        is_favorite: true,
        updated_at: OffsetDateTime::UNIX_EPOCH,
        has_images: false,
    };
    let request_id = match update(
        &mut model,
        AppMsg::Navigation(NavigationMsg::SelectCategory(Some(category_id))),
    )
    .as_slice()
    {
        [Effect::LoadBrowser { request_id, .. }] => *request_id,
        _ => panic!("selecting a category should load its notes"),
    };

    let effects = update(
        &mut model,
        AppMsg::Library(LibraryReply::BrowserLoaded {
            favorites: Ok(vec![favorite.clone()]),
            request_id,
            result: Ok(vec![favorite.clone()]),
        }),
    );

    assert!(effects.is_empty());
    assert_eq!(
        model.browser.notes.state,
        LoadState::Ready(vec![favorite.clone()])
    );
    assert_eq!(
        model.browser.favorites.state,
        LoadState::Ready(vec![favorite])
    );
}

#[test]
fn opening_a_note_from_all_notes_should_preserve_the_all_notes_context() {
    let mut model = AppModel::new(&Config::default());
    let category_id = CategoryId::new();
    let note_id = NoteId::new();
    let request_id = match update(
        &mut model,
        AppMsg::Navigation(NavigationMsg::OpenNote(note_id)),
    )
    .as_slice()
    {
        [Effect::LoadEditorNote { request_id, .. }] => *request_id,
        _ => panic!("opening a note should start one load"),
    };

    let _ = update(
        &mut model,
        AppMsg::Library(LibraryReply::EditorLoaded {
            request_id,
            result: Ok(Note {
                id: note_id,
                category_id,
                source: String::from("# Categorized note"),
                title: String::from("Categorized note"),
                plain_text: String::from("Categorized note"),
                revision: Revision(1),
                is_favorite: false,
                created_at: OffsetDateTime::UNIX_EPOCH,
                updated_at: OffsetDateTime::UNIX_EPOCH,
                trashed_at: None,
            }),
        }),
    );

    assert_eq!(model.selected_category, None);
}

#[test]
fn opening_a_note_should_ignore_an_older_load_completion() {
    let mut model = AppModel::new(&Config::default());
    let first_note_id = NoteId::new();
    let first_request = match update(
        &mut model,
        AppMsg::Navigation(NavigationMsg::OpenNote(first_note_id)),
    )
    .as_slice()
    {
        [Effect::LoadEditorNote { request_id, .. }] => *request_id,
        _ => panic!("opening a note should start one load"),
    };
    let second_note_id = NoteId::new();
    let second_request = match update(
        &mut model,
        AppMsg::Navigation(NavigationMsg::OpenNote(second_note_id)),
    )
    .as_slice()
    {
        [Effect::LoadEditorNote { request_id, .. }] => *request_id,
        _ => panic!("opening a second note should supersede the first load"),
    };

    let _ = update(
        &mut model,
        AppMsg::Library(LibraryReply::EditorLoaded {
            request_id: first_request,
            result: Err(UiError::new("stale")),
        }),
    );
    assert_eq!(model.editor_load_request, Some(second_request));
    assert_eq!(model.editor, None);
}

#[test]
fn exporting_a_browser_note_should_open_its_export_options_after_loading() {
    let mut model = AppModel::new(&Config::default());
    let category_id = CategoryId::new();
    let note_id = NoteId::new();
    let request_id = match update(
        &mut model,
        AppMsg::Navigation(NavigationMsg::ExportNote(note_id)),
    )
    .as_slice()
    {
        [
            Effect::LoadEditorNote {
                request_id,
                note_id: effect_note_id,
            },
        ] if *effect_note_id == note_id => *request_id,
        _ => panic!("exporting a browser note should load its editor snapshot"),
    };

    let effects = update(
        &mut model,
        AppMsg::Library(LibraryReply::EditorLoaded {
            request_id,
            result: Ok(Note {
                id: note_id,
                category_id,
                source: String::from("# Browser note"),
                title: String::from("Browser note"),
                plain_text: String::from("Browser note"),
                revision: Revision(1),
                is_favorite: false,
                created_at: OffsetDateTime::UNIX_EPOCH,
                updated_at: OffsetDateTime::UNIX_EPOCH,
                trashed_at: None,
            }),
        }),
    );

    assert!(matches!(
        effects.as_slice(),
        [Effect::ShowEditorExportDialog { .. }]
    ));
    assert_eq!(model.route, Route::Editor);
    assert_eq!(
        model.editor.as_ref().map(|document| document.note_id),
        Some(note_id)
    );
    assert!(model.editor_export_after_load.is_none());
    assert_eq!(
        model
            .editor_export_dialog_request
            .as_ref()
            .map(|request| (&request.note_id, request.source.as_str())),
        Some((&note_id, "# Browser note"))
    );
}

#[test]
fn stale_browser_reply_should_not_replace_favorites() {
    let mut model = AppModel::new(&Config::default());
    let _ = update(&mut model, AppMsg::Browser(BrowserMsg::Reload));
    model.browser.favorites.state = LoadState::Ready(Vec::new());
    let effects = update(
        &mut model,
        AppMsg::Library(LibraryReply::BrowserLoaded {
            request_id: RequestId(999),
            result: Ok(Vec::new()),
            favorites: Err(UiError::new("stale favorites")),
        }),
    );
    assert!(effects.is_empty());
    assert_eq!(model.browser.favorites.state, LoadState::Ready(Vec::new()));
}

#[test]
fn queued_category_switch_should_keep_previous_favorites_until_latest_reply() {
    let mut model = AppModel::new(&Config::default());
    model.browser.favorites.state = LoadState::Ready(Vec::new());
    let first = update(&mut model, AppMsg::Browser(BrowserMsg::Reload));
    let [Effect::LoadBrowser { request_id, .. }] = first.as_slice() else {
        panic!("browser load expected");
    };
    let _ = update(
        &mut model,
        AppMsg::Navigation(NavigationMsg::SelectCategory(Some(CategoryId::new()))),
    );
    let effects = update(
        &mut model,
        AppMsg::Library(LibraryReply::BrowserLoaded {
            request_id: *request_id,
            result: Ok(Vec::new()),
            favorites: Err(UiError::new("previous category favorites")),
        }),
    );
    assert!(matches!(effects.as_slice(), [Effect::LoadBrowser { .. }]));
    assert_eq!(model.browser.favorites.state, LoadState::Ready(Vec::new()));
    assert_eq!(model.browser.last_ready_notes, None);
}

#[test]
fn failed_favorites_should_not_discard_successful_browser_notes() {
    let mut model = AppModel::new(&Config::default());
    let effects = update(&mut model, AppMsg::Browser(BrowserMsg::Reload));
    let [Effect::LoadBrowser { request_id, .. }] = effects.as_slice() else {
        panic!("browser load expected");
    };
    let _ = update(
        &mut model,
        AppMsg::Library(LibraryReply::BrowserLoaded {
            request_id: *request_id,
            result: Ok(Vec::new()),
            favorites: Err(UiError::new("favorites unavailable")),
        }),
    );
    assert_eq!(model.browser.notes.state, LoadState::Ready(Vec::new()));
    assert_eq!(
        model.browser.favorites.state,
        LoadState::Failed(UiError::new("favorites unavailable"))
    );
    assert!(!model.browser.loading_indicator_visible);
}
