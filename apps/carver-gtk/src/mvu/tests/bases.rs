use super::*;

#[test]
fn sidebar_selection_should_follow_the_editor_origin_and_current_route() {
    use crate::mvu::SidebarSelection;
    let mut model = AppModel::new(&Config::default());
    assert_eq!(model.sidebar_selection(), SidebarSelection::Category(None));
    let base = BaseId::new();
    model.bases.selected = Some(base);
    model.route = Route::Base;
    assert_eq!(model.sidebar_selection(), SidebarSelection::Base(base));
    model.route = Route::Editor;
    model.editor_return_route = Route::Base;
    assert_eq!(model.sidebar_selection(), SidebarSelection::Base(base));
    model.route = Route::Browser;
    assert_eq!(model.sidebar_selection(), SidebarSelection::Category(None));
    model.route = Route::Trash;
    assert_eq!(model.sidebar_selection(), SidebarSelection::None);
}

#[test]
fn base_loading_delay_should_ignore_completed_and_superseded_requests() {
    let mut model = AppModel::new(&Config::default());
    let request = RequestId(20);
    model.bases.rows.state = LoadState::Loading(request);
    let _ = update(
        &mut model,
        AppMsg::Bases(BasesMsg::LoadingIndicatorElapsed(RequestId(19))),
    );
    assert_eq!(model.bases.rows_loading_elapsed, None);
    let _ = update(
        &mut model,
        AppMsg::Bases(BasesMsg::LoadingIndicatorElapsed(request)),
    );
    assert_eq!(model.bases.rows_loading_elapsed, Some(request));
    model.bases.definitions.state = LoadState::Ready(Vec::new());
    let _ = update(
        &mut model,
        AppMsg::Bases(BasesMsg::LoadingIndicatorElapsed(RequestId(21))),
    );
    assert_eq!(model.bases.definitions_loading_elapsed, None);
}

#[test]
fn creating_a_base_should_keep_a_dirty_editor_open_when_saving_fails() {
    let mut model = AppModel::new(&Config::default());
    let note_id = NoteId::new();
    let _ = update(
        &mut model,
        AppMsg::Editor(EditorMsg::Load {
            note_id,
            revision: Revision(1),
            source: "Initial".to_owned(),
        }),
    );
    let _ = update(
        &mut model,
        AppMsg::Editor(EditorMsg::SourceChanged("Unsaved".to_owned())),
    );
    let base = BaseDefinition {
        id: BaseId::new(),
        name: "Projects".to_owned(),
        columns: Vec::new(),
        revision: Revision(1),
        row_count: 0,
    };
    let effects = update(
        &mut model,
        AppMsg::Library(LibraryReply::BaseCreated { result: Ok(base) }),
    );
    let request = match effects.as_slice() {
        [Effect::LoadBases { .. }, Effect::SaveNote { request }] => request.clone(),
        _ => panic!("creating a base should save before navigating"),
    };
    assert_eq!(request.source, "Unsaved");
    assert_eq!(model.route, Route::Editor);
    let _ = update(
        &mut model,
        AppMsg::Library(LibraryReply::EditorSaved {
            request,
            result: Err(UiError::new("save failed")),
        }),
    );
    assert_eq!(model.route, Route::Editor);
    assert!(model.editor.is_some());
}

#[test]
fn opening_a_base_should_retry_failed_definitions() {
    let mut model = AppModel::new(&Config::default());
    model.bases.definitions.state = LoadState::Failed(UiError::new("offline"));
    let effects = update(&mut model, AppMsg::Bases(BasesMsg::Open(BaseId::new())));
    assert!(matches!(
        effects.as_slice(),
        [Effect::LoadBaseRows { .. }, Effect::LoadBases { .. }]
    ));
}

#[test]
fn opening_a_base_should_change_route_and_load_its_rows() {
    let mut model = AppModel::new(&Config::default());
    let base_id = BaseId::new();

    let effects = update(&mut model, AppMsg::Bases(BasesMsg::Open(base_id)));

    assert_eq!(model.route, Route::Base);
    assert_eq!(model.bases.selected, Some(base_id));
    assert!(
        matches!(effects.as_slice(), [Effect::LoadBaseRows { base_id: loaded, .. }] if *loaded == base_id)
    );
}

#[test]
fn created_base_should_reload_definitions_and_open_its_grid() {
    let mut model = AppModel::new(&Config::default());
    let base = BaseDefinition {
        id: BaseId::new(),
        name: "Projects".to_owned(),
        columns: vec![BaseColumn::Category],
        revision: Revision(1),
        row_count: 0,
    };

    let effects = update(
        &mut model,
        AppMsg::Library(LibraryReply::BaseCreated {
            result: Ok(base.clone()),
        }),
    );

    assert_eq!(model.route, Route::Base);
    assert_eq!(model.bases.selected, Some(base.id));
    assert!(
        matches!(effects.as_slice(), [Effect::LoadBases { .. }, Effect::LoadBaseRows { base_id, .. }] if *base_id == base.id)
    );
}

#[test]
fn closing_a_note_opened_from_a_base_should_restore_the_base_route() {
    let mut model = AppModel::new(&Config::default());
    let base_id = BaseId::new();
    model.route = Route::Base;
    model.bases.selected = Some(base_id);
    let note_id = NoteId::new();

    let _ = update(
        &mut model,
        AppMsg::Navigation(NavigationMsg::OpenNote(note_id)),
    );
    let _ = update(
        &mut model,
        AppMsg::Editor(EditorMsg::Load {
            note_id,
            revision: Revision(1),
            source: String::new(),
        }),
    );
    let effects = update(&mut model, AppMsg::Editor(EditorMsg::BackRequested));

    assert!(matches!(
        effects.as_slice(),
        [Effect::LoadBaseRows { base_id: loaded, .. }, Effect::LoadBrowser { .. }] if *loaded == base_id
    ));
    assert_eq!(model.route, Route::Base);
    assert_eq!(model.bases.selected, Some(base_id));
}

#[test]
fn opening_a_base_from_a_dirty_editor_should_save_before_navigating() {
    let mut model = AppModel::new(&Config::default());
    let note_id = NoteId::new();
    let base_id = BaseId::new();
    let _ = update(
        &mut model,
        AppMsg::Editor(EditorMsg::Load {
            note_id,
            revision: Revision(1),
            source: "Initial".to_owned(),
        }),
    );
    let _ = update(
        &mut model,
        AppMsg::Editor(EditorMsg::SourceChanged("Unsaved".to_owned())),
    );

    let effects = update(&mut model, AppMsg::Bases(BasesMsg::Open(base_id)));
    let request = match effects.as_slice() {
        [Effect::SaveNote { request }] => request.clone(),
        _ => panic!("opening a base should first save the dirty editor"),
    };
    assert_eq!(model.route, Route::Editor);

    let effects = update(
        &mut model,
        AppMsg::Library(LibraryReply::EditorSaved {
            request,
            result: Ok(Revision(2)),
        }),
    );
    assert_eq!(model.route, Route::Base);
    assert!(effects.iter().any(
        |effect| matches!(effect, Effect::LoadBaseRows { base_id: loaded, .. } if *loaded == base_id)
    ));
}

#[test]
fn rapidly_switching_bases_should_load_the_latest_selection_after_in_flight_rows_settle() {
    let mut model = AppModel::new(&Config::default());
    let first = BaseId::new();
    let second = BaseId::new();
    let first_effects = update(&mut model, AppMsg::Bases(BasesMsg::Open(first)));
    let request_id = match first_effects.as_slice() {
        [Effect::LoadBaseRows { request_id, .. }] => *request_id,
        _ => panic!("first base should start loading"),
    };
    assert!(update(&mut model, AppMsg::Bases(BasesMsg::Open(second))).is_empty());

    let effects = update(
        &mut model,
        AppMsg::Library(LibraryReply::BaseRowsLoaded {
            request_id,
            base_id: first,
            result: Ok(Vec::new()),
        }),
    );
    assert!(matches!(
        effects.as_slice(),
        [Effect::LoadBaseRows { base_id, .. }] if *base_id == second
    ));
}

#[test]
fn invalidating_base_definitions_in_flight_should_schedule_one_follow_up_load() {
    let mut model = AppModel::new(&Config::default());
    let first = update(&mut model, AppMsg::Bases(BasesMsg::Reload));
    let request_id = match first.as_slice() {
        [Effect::LoadBases { request_id }] => *request_id,
        _ => panic!("definitions should start loading"),
    };
    assert!(update(&mut model, AppMsg::Bases(BasesMsg::Reload)).is_empty());

    let effects = update(
        &mut model,
        AppMsg::Library(LibraryReply::BasesLoaded {
            request_id,
            result: Ok(Vec::new()),
        }),
    );
    assert!(matches!(effects.as_slice(), [Effect::LoadBases { .. }]));
}

#[test]
fn external_library_change_should_reload_selected_base_rows() {
    let mut model = AppModel::new(&Config::default());
    let base_id = BaseId::new();
    model.bases.selected = Some(base_id);
    model.library_revision = Some(LibraryRevision(1));
    let request = update(&mut model, AppMsg::LibraryChangedExternally);
    let request_id = match request.as_slice() {
        [Effect::LoadLibraryRevision { request_id }] => *request_id,
        _ => panic!("external wakeup should check the revision"),
    };

    let effects = update(
        &mut model,
        AppMsg::Library(LibraryReply::LibraryRevisionLoaded {
            request_id,
            result: Ok(LibraryRevision(2)),
        }),
    );
    assert!(effects.iter().any(
        |effect| matches!(effect, Effect::LoadBaseRows { base_id: loaded, .. } if *loaded == base_id)
    ));
}
