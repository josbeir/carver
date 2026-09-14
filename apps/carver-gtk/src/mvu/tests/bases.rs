use super::*;
use carver_sdk::{BaseSort, BaseSortDirection};

#[test]
fn configure_should_prepare_unfiltered_rows_and_ignore_stale_replies() {
    let mut model = AppModel::new(&Config::default());
    model.bases.property_descriptors.state = LoadState::Ready(Vec::new());
    let definition = BaseDefinition {
        id: BaseId::new(),
        name: "Projects".into(),
        columns: vec![BaseColumn::Name],
        filter_mode: BaseFilterMode::All,
        filters: Vec::new(),
        sorts: Vec::new(),
        revision: Revision(1),
        row_count: 0,
    };
    model.route = Route::Base;
    model.bases.selected = Some(definition.id);
    model.bases.definitions.state = LoadState::Ready(vec![definition.clone()]);
    let first = update(&mut model, AppMsg::Bases(BasesMsg::Configure));
    let [
        Effect::PrepareBaseConfiguration {
            request_id: old_id, ..
        },
    ] = first.as_slice()
    else {
        panic!("prepare effect");
    };
    let second = update(&mut model, AppMsg::Bases(BasesMsg::Configure));
    let [Effect::PrepareBaseConfiguration { request_id, .. }] = second.as_slice() else {
        panic!("prepare effect");
    };
    assert!(
        update(
            &mut model,
            AppMsg::Library(LibraryReply::BaseConfigurationLoaded {
                request_id: *old_id,
                definition: definition.clone(),
                result: Ok(())
            })
        )
        .is_empty()
    );
    assert!(matches!(
        update(
            &mut model,
            AppMsg::Library(LibraryReply::BaseConfigurationLoaded {
                request_id: *request_id,
                definition,
                result: Ok(())
            })
        )
        .as_slice(),
        [Effect::ShowBaseConfiguration { .. }]
    ));
}

#[test]
fn base_search_should_debounce_and_reload_the_selected_base() {
    let mut model = AppModel::new(&Config::default());
    let base_id = BaseId::new();
    model.route = Route::Base;
    model.bases.selected = Some(base_id);
    model.bases.rows.state = LoadState::Ready(Vec::new());

    assert!(update(&mut model, AppMsg::Bases(BasesMsg::SearchOpened)).is_empty());
    let effects = update(
        &mut model,
        AppMsg::Bases(BasesMsg::SearchChanged("roadmap".to_owned())),
    );
    let [Effect::ScheduleBaseSearch { timer_id }] = effects.as_slice() else {
        panic!("Base search should schedule one debounce");
    };

    assert!(matches!(
        update(
            &mut model,
            AppMsg::Bases(BasesMsg::SearchTimerFired(*timer_id)),
        )
        .as_slice(),
        [Effect::LoadBaseRows { base_id: loaded, query, .. }]
            if *loaded == base_id && query == "roadmap"
    ));
}

#[test]
fn closing_base_search_should_clear_its_query_and_reload_all_rows() {
    let mut model = AppModel::new(&Config::default());
    let base_id = BaseId::new();
    model.route = Route::Base;
    model.bases.selected = Some(base_id);
    model.bases.rows.state = LoadState::Ready(Vec::new());
    model.bases.search_open = true;
    model.bases.search_query = "roadmap".to_owned();

    assert!(matches!(
        update(
            &mut model,
            AppMsg::Bases(BasesMsg::SearchVisibilityChanged(false)),
        )
        .as_slice(),
        [Effect::LoadBaseRows { base_id: loaded, query, .. }]
            if *loaded == base_id && query.is_empty()
    ));
    assert!(!model.bases.search_open);
    assert!(model.bases.search_query.is_empty());
}

#[test]
fn configuring_a_new_base_should_prepare_the_shared_dialog_and_reject_stale_rows() {
    let mut model = AppModel::new(&Config::default());
    model.bases.property_descriptors.state = LoadState::Ready(Vec::new());
    let effects = update(&mut model, AppMsg::Bases(BasesMsg::ConfigureNew));
    let [Effect::PrepareNewBaseConfiguration { request_id }] = effects.as_slice() else {
        panic!("new Base configuration should prepare its rows");
    };
    assert!(update(&mut model, AppMsg::Bases(BasesMsg::ConfigureNew)).is_empty());
    assert!(
        update(
            &mut model,
            AppMsg::Library(LibraryReply::NewBaseConfigurationLoaded {
                request_id: RequestId(request_id.0 + 1),
                result: Ok(()),
            }),
        )
        .is_empty()
    );
    assert!(matches!(
        update(
            &mut model,
            AppMsg::Library(LibraryReply::NewBaseConfigurationLoaded {
                request_id: *request_id,
                result: Ok(()),
            }),
        )
        .as_slice(),
        [Effect::ShowNewBaseConfiguration { .. }]
    ));
}

#[test]
fn configuring_a_new_base_should_wait_for_property_descriptors() {
    let mut model = AppModel::new(&Config::default());
    let descriptor_request = RequestId(8);
    model.bases.property_descriptors.state = LoadState::Loading(descriptor_request);

    assert!(update(&mut model, AppMsg::Bases(BasesMsg::ConfigureNew)).is_empty());
    assert!(matches!(
        model.bases.configuration_request,
        Some(RequestId(1))
    ));

    let descriptor = carver_sdk::PropertyDescriptor {
        path: carver_sdk::PropertyPath("/project/status".to_owned()),
        kind: carver_sdk::PropertyKind::Text,
        example: Some("planned".to_owned()),
    };
    assert!(matches!(
        update(
            &mut model,
            AppMsg::Library(LibraryReply::PropertyDescriptorsLoaded {
                request_id: descriptor_request,
                result: Ok(vec![descriptor.clone()]),
            }),
        )
        .as_slice(),
        [Effect::ShowNewBaseConfiguration { descriptors }] if descriptors == &vec![descriptor]
    ));
    assert!(model.bases.configuration_request.is_none());
}

#[test]
fn base_preview_count_should_debounce_and_ignore_a_superseded_draft() {
    let mut model = AppModel::new(&Config::default());
    let first_filter = BaseFilter {
        field: BaseColumn::Category,
        operator: carver_sdk::BaseFilterOperator::Equals,
        value: Some(serde_json::json!("Work")),
    };
    let second_filter = BaseFilter {
        field: BaseColumn::Category,
        operator: carver_sdk::BaseFilterOperator::Equals,
        value: Some(serde_json::json!("Personal")),
    };
    let first = update(
        &mut model,
        AppMsg::Bases(BasesMsg::PreviewCount {
            filter_mode: BaseFilterMode::All,
            filters: vec![first_filter],
        }),
    );
    let [
        Effect::ScheduleBasePreview {
            timer_id: first_timer,
        },
    ] = first.as_slice()
    else {
        panic!("first preview should schedule a debounce");
    };
    let second = update(
        &mut model,
        AppMsg::Bases(BasesMsg::PreviewCount {
            filter_mode: BaseFilterMode::Any,
            filters: vec![second_filter.clone()],
        }),
    );
    let [
        Effect::ScheduleBasePreview {
            timer_id: second_timer,
        },
    ] = second.as_slice()
    else {
        panic!("second preview should replace the debounce");
    };
    assert!(
        update(
            &mut model,
            AppMsg::Bases(BasesMsg::PreviewCountTimerFired(*first_timer)),
        )
        .is_empty()
    );
    assert!(matches!(
        update(
            &mut model,
            AppMsg::Bases(BasesMsg::PreviewCountTimerFired(*second_timer)),
        )
        .as_slice(),
        [Effect::PreviewBaseRowCount {
            filter_mode: BaseFilterMode::Any,
            filters,
            ..
        }] if filters == &vec![second_filter]
    ));
}

#[test]
fn configured_base_creation_should_trim_its_name_and_preserve_its_query() {
    let mut model = AppModel::new(&Config::default());
    let filter = BaseFilter {
        field: BaseColumn::Category,
        operator: carver_sdk::BaseFilterOperator::Equals,
        value: Some(serde_json::json!("Work")),
    };
    let sort = carver_sdk::BaseSort {
        field: BaseColumn::Updated,
        direction: carver_sdk::BaseSortDirection::Descending,
    };

    assert_eq!(
        update(
            &mut model,
            AppMsg::Bases(BasesMsg::CreateConfigured {
                name: "  Project tracker  ".to_owned(),
                columns: vec![BaseColumn::Category],
                filter_mode: BaseFilterMode::Any,
                filters: vec![filter.clone()],
                sorts: vec![sort.clone()],
            }),
        ),
        vec![Effect::CreateConfiguredBase {
            name: "Project tracker".to_owned(),
            columns: vec![BaseColumn::Category],
            filter_mode: BaseFilterMode::Any,
            filters: vec![filter],
            sorts: vec![sort],
        }]
    );
    assert!(model.bases.saving_configuration);
}

#[test]
fn failed_configured_base_creation_should_reenable_the_new_base_draft() {
    let mut model = AppModel::new(&Config::default());
    let _ = update(
        &mut model,
        AppMsg::Bases(BasesMsg::CreateConfigured {
            name: "Projects".to_owned(),
            columns: Vec::new(),
            filter_mode: BaseFilterMode::All,
            filters: Vec::new(),
            sorts: Vec::new(),
        }),
    );

    assert_eq!(
        update(
            &mut model,
            AppMsg::Library(LibraryReply::BaseCreated {
                result: Err(UiError::new("duplicate Base")),
            }),
        ),
        vec![Effect::FinishBaseConfiguration { success: false }]
    );
    assert!(!model.bases.saving_configuration);
    assert_eq!(model.notice, Some(UiError::new("duplicate Base")));
}

#[test]
fn failed_configuration_save_should_reenable_the_existing_draft() {
    let mut model = AppModel::new(&Config::default());
    model.bases.saving_configuration = true;
    assert_eq!(
        update(
            &mut model,
            AppMsg::Library(LibraryReply::BaseUpdated {
                result: Err(UiError::new("conflict"))
            })
        ),
        vec![Effect::FinishBaseConfiguration { success: false }]
    );
    assert!(!model.bases.saving_configuration);
}

#[test]
fn deleting_a_base_should_preserve_an_open_draft_and_redirect_its_return_route() {
    let mut model = AppModel::new(&Config::default());
    let base = BaseId::new();
    let _ = update(
        &mut model,
        AppMsg::Editor(EditorMsg::Load {
            note_id: NoteId::new(),
            revision: Revision(1),
            source: "Draft".to_owned(),
        }),
    );
    model.editor_return_route = Route::Base;
    model.bases.selected = Some(base);
    let original = model.editor.clone();
    assert!(matches!(
        update(&mut model, AppMsg::Bases(BasesMsg::Delete(base))).as_slice(),
        [Effect::DeleteBase { .. }]
    ));
    assert!(update(&mut model, AppMsg::Bases(BasesMsg::Delete(base))).is_empty());
    let _ = update(
        &mut model,
        AppMsg::Library(LibraryReply::BaseDeleted {
            base_id: base,
            result: Ok(()),
        }),
    );
    assert_eq!(model.route, Route::Editor);
    assert_eq!(model.editor, original);
    assert_eq!(model.editor_return_route, Route::Browser);
    assert_eq!(model.bases.selected, None);
}

#[test]
fn failed_base_deletion_should_leave_the_selection_and_allow_retry() {
    let mut model = AppModel::new(&Config::default());
    let base = BaseId::new();
    model.route = Route::Base;
    model.bases.selected = Some(base);
    let _ = update(&mut model, AppMsg::Bases(BasesMsg::Delete(base)));
    let error = UiError::new("read only");
    let _ = update(
        &mut model,
        AppMsg::Library(LibraryReply::BaseDeleted {
            base_id: base,
            result: Err(error.clone()),
        }),
    );
    assert_eq!(model.notice, Some(error));
    assert_eq!(model.bases.selected, Some(base));
    assert_eq!(model.route, Route::Base);
    assert!(!update(&mut model, AppMsg::Bases(BasesMsg::Delete(base))).is_empty());
}

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
fn stale_property_descriptor_reply_should_not_replace_a_newer_request() {
    let mut model = AppModel::new(&Config::default());
    let current = RequestId(8);
    model.bases.property_descriptors.state = LoadState::Loading(current);
    let stale = carver_sdk::PropertyDescriptor {
        path: carver_sdk::PropertyPath("/stale".to_owned()),
        kind: carver_sdk::PropertyKind::Text,
        example: Some("old".to_owned()),
    };
    assert!(
        update(
            &mut model,
            AppMsg::Library(LibraryReply::PropertyDescriptorsLoaded {
                request_id: RequestId(7),
                result: Ok(vec![stale]),
            }),
        )
        .is_empty()
    );
    assert_eq!(
        model.bases.property_descriptors.state,
        LoadState::Loading(current)
    );

    let descriptor = carver_sdk::PropertyDescriptor {
        path: carver_sdk::PropertyPath("/status".to_owned()),
        kind: carver_sdk::PropertyKind::Text,
        example: Some("ready".to_owned()),
    };
    assert!(
        update(
            &mut model,
            AppMsg::Library(LibraryReply::PropertyDescriptorsLoaded {
                request_id: current,
                result: Ok(vec![descriptor.clone()]),
            }),
        )
        .is_empty()
    );
    assert_eq!(
        model.bases.property_descriptors.state,
        LoadState::Ready(vec![descriptor])
    );
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
        filter_mode: carver_sdk::BaseFilterMode::All,
        filters: Vec::new(),
        sorts: Vec::new(),
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
        filter_mode: carver_sdk::BaseFilterMode::All,
        filters: Vec::new(),
        sorts: Vec::new(),
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
            result: Ok(Page {
                items: Vec::new(),
                has_more: false,
            }),
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

#[test]
fn external_base_deletion_should_return_the_window_to_the_browser() {
    let mut model = AppModel::new(&Config::default());
    let base_id = BaseId::new();
    model.route = Route::Base;
    model.bases.selected = Some(base_id);
    model.editor_return_route = Route::Base;
    model.library_revision = Some(LibraryRevision(1));

    let effects = update(&mut model, AppMsg::LibraryChangedExternally);
    let request_id = match effects.as_slice() {
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
    let definitions_request = effects
        .iter()
        .find_map(|effect| match effect {
            Effect::LoadBases { request_id } => Some(*request_id),
            _ => None,
        })
        .unwrap_or_else(|| panic!("external refresh should reload Bases"));

    assert!(
        update(
            &mut model,
            AppMsg::Library(LibraryReply::BasesLoaded {
                request_id: definitions_request,
                result: Ok(Vec::new()),
            }),
        )
        .is_empty()
    );
    assert_eq!(model.route, Route::Browser);
    assert_eq!(model.bases.selected, None);
    assert_eq!(model.editor_return_route, Route::Browser);
}

#[test]
fn coalesced_base_reload_should_wait_for_the_latest_definitions_before_clearing_selection() {
    let mut model = AppModel::new(&Config::default());
    let base_id = BaseId::new();
    model.route = Route::Base;
    model.bases.selected = Some(base_id);

    let initial = update(&mut model, AppMsg::Bases(BasesMsg::Reload));
    let initial_request = match initial.as_slice() {
        [Effect::LoadBases { request_id }] => *request_id,
        _ => panic!("initial Base reload should start"),
    };
    assert!(update(&mut model, AppMsg::Bases(BasesMsg::Reload)).is_empty());

    let reload = update(
        &mut model,
        AppMsg::Library(LibraryReply::BasesLoaded {
            request_id: initial_request,
            result: Ok(Vec::new()),
        }),
    );
    assert!(matches!(reload.as_slice(), [Effect::LoadBases { .. }]));
    assert_eq!(model.route, Route::Base);
    assert_eq!(model.bases.selected, Some(base_id));
}

#[test]
fn base_update_should_forward_configuration_and_reload_rows() {
    let mut model = AppModel::new(&Config::default());
    let base_id = BaseId::new();
    model.route = Route::Base;
    model.bases.selected = Some(base_id);
    let effects = update(
        &mut model,
        AppMsg::Bases(BasesMsg::Update {
            base_id,
            revision: Revision(3),
            name: "Projects".to_owned(),
            columns: vec![BaseColumn::Category],
            filter_mode: BaseFilterMode::All,
            filters: Vec::new(),
            sorts: Vec::new(),
        }),
    );
    assert!(
        matches!(effects.as_slice(), [Effect::UpdateBase { base_id: id, revision: Revision(3), .. }] if *id == base_id)
    );
}

#[test]
fn base_header_sorts_should_forward_the_native_sort_order() {
    let mut model = AppModel::new(&Config::default());
    let base_id = BaseId::new();
    let definition = BaseDefinition {
        id: base_id,
        name: "Projects".to_owned(),
        columns: vec![BaseColumn::Name, BaseColumn::Category],
        filter_mode: BaseFilterMode::All,
        filters: Vec::new(),
        sorts: Vec::new(),
        revision: Revision(3),
        row_count: 0,
    };
    model.route = Route::Base;
    model.bases.selected = Some(base_id);
    model.bases.definitions.state = LoadState::Ready(vec![definition.clone()]);

    let effects = update(
        &mut model,
        AppMsg::Bases(BasesMsg::SetSorts {
            sorts: vec![
                BaseSort {
                    field: BaseColumn::Name,
                    direction: BaseSortDirection::Descending,
                },
                BaseSort {
                    field: BaseColumn::Category,
                    direction: BaseSortDirection::Ascending,
                },
            ],
        }),
    );
    assert!(matches!(
        effects.as_slice(),
        [Effect::UpdateBase { sorts, .. }]
            if sorts == &vec![
                BaseSort { field: BaseColumn::Name, direction: BaseSortDirection::Descending },
                BaseSort { field: BaseColumn::Category, direction: BaseSortDirection::Ascending },
            ]
    ));
}

#[test]
fn stale_base_update_should_leave_a_notice_without_reloading() {
    let mut model = AppModel::new(&Config::default());
    let effects = update(
        &mut model,
        AppMsg::Library(LibraryReply::BaseUpdated {
            result: Err(UiError::new("base changed")),
        }),
    );
    assert!(effects.is_empty());
    assert_eq!(
        model.notice.as_ref().map(|error| error.message.as_str()),
        Some("base changed")
    );
}
