use super::*;

#[test]
fn startup_should_initialize_then_request_sidebar_and_browser_data() {
    let mut model = AppModel::new(&Config::default());

    let effects = update(&mut model, AppMsg::Navigation(NavigationMsg::Started));

    assert_eq!(effects, vec![Effect::EnsureDefaultCategory]);
    let effects = update(
        &mut model,
        AppMsg::Library(LibraryReply::DefaultCategoryEnsured { result: Ok(()) }),
    );
    assert_eq!(
        effects,
        vec![
            Effect::LoadSidebar {
                request_id: RequestId(1),
            },
            Effect::LoadBases {
                request_id: RequestId(2)
            },
            Effect::LoadBrowser {
                request_id: RequestId(3),
                category_id: None,
                query: String::new(),
            },
            Effect::LoadLibraryRevision {
                request_id: RequestId(4),
            },
        ]
    );
    assert_eq!(model.sidebar.state, LoadState::Loading(RequestId(1)));
    assert_eq!(model.browser.notes.state, LoadState::Loading(RequestId(3)));
}

#[test]
fn external_library_change_should_reload_visible_resources_after_the_revision_changes() {
    let mut model = AppModel::new(&Config::default());
    model.library_revision = Some(LibraryRevision(4));

    let effects = update(&mut model, AppMsg::LibraryChangedExternally);

    assert_eq!(
        effects,
        vec![Effect::LoadLibraryRevision {
            request_id: RequestId(1),
        }]
    );
    let effects = update(
        &mut model,
        AppMsg::Library(LibraryReply::LibraryRevisionLoaded {
            request_id: RequestId(1),
            result: Ok(LibraryRevision(5)),
        }),
    );
    assert_eq!(
        effects,
        vec![
            Effect::LoadSidebar {
                request_id: RequestId(2),
            },
            Effect::LoadBases {
                request_id: RequestId(3)
            },
            Effect::LoadBrowser {
                request_id: RequestId(4),
                category_id: None,
                query: String::new(),
            },
            Effect::LoadTrash {
                request_id: RequestId(5),
            },
        ]
    );
}

#[test]
fn unchanged_external_library_wakeup_should_not_reload_resources() {
    let mut model = AppModel::new(&Config::default());
    model.library_revision = Some(LibraryRevision(4));
    let _ = update(&mut model, AppMsg::LibraryChangedExternally);

    let effects = update(
        &mut model,
        AppMsg::Library(LibraryReply::LibraryRevisionLoaded {
            request_id: RequestId(1),
            result: Ok(LibraryRevision(4)),
        }),
    );

    assert!(effects.is_empty());
}

#[test]
fn new_note_should_create_in_the_selected_category() {
    let mut model = AppModel::new(&Config::default());
    let category_id = CategoryId::new();
    model.selected_category = Some(category_id);

    assert_eq!(
        update(&mut model, AppMsg::Navigation(NavigationMsg::CreateNote)),
        vec![Effect::CreateNote { category_id }]
    );
}

#[test]
fn import_should_create_in_the_selected_category() {
    let mut model = AppModel::new(&Config::default());
    let category_id = CategoryId::new();
    model.selected_category = Some(category_id);

    assert_eq!(
        update(
            &mut model,
            AppMsg::Navigation(NavigationMsg::ImportNote {
                format: DocumentImportFormat::Markdown,
                source: String::from("# Imported"),
            }),
        ),
        vec![Effect::ImportNote {
            category_id,
            format: DocumentImportFormat::Markdown,
            source: String::from("# Imported"),
        }]
    );
}

#[test]
fn import_should_use_the_first_category_when_all_notes_is_selected() {
    let mut model = AppModel::new(&Config::default());
    let category_id = CategoryId::new();
    model.sidebar.state = LoadState::Ready(vec![carver_sdk::CategorySummary {
        category: carver_sdk::Category {
            id: category_id,
            name: String::from("Notes"),
            appearance: carver_sdk::CategoryAppearance::default(),
            position: 0,
            created_at: time::OffsetDateTime::UNIX_EPOCH,
            updated_at: time::OffsetDateTime::UNIX_EPOCH,
            trashed_at: None,
        },
        note_count: 0,
    }]);

    let effects = update(
        &mut model,
        AppMsg::Navigation(NavigationMsg::ImportNote {
            format: DocumentImportFormat::Carve,
            source: String::from("# Imported"),
        }),
    );

    assert_eq!(
        effects,
        vec![Effect::ImportNote {
            category_id,
            format: DocumentImportFormat::Carve,
            source: String::from("# Imported"),
        }]
    );
}

#[test]
fn import_failure_should_not_change_the_active_route() {
    let mut model = AppModel::new(&Config::default());

    let effects = update(
        &mut model,
        AppMsg::Navigation(NavigationMsg::ImportFailed(String::from("Invalid UTF-8"))),
    );

    assert!(effects.is_empty());
    assert_eq!(model.route, Route::Browser);
    assert_eq!(model.notice, Some(UiError::new("Invalid UTF-8")));
}

#[test]
fn unbound_dispatcher_should_not_dispatch_messages() {
    let dispatcher = AppDispatcher::default();

    assert!(!dispatcher.dispatch(AppMsg::Navigation(NavigationMsg::ShowBrowser)));
}
