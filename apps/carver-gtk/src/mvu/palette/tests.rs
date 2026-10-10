use super::super::{EditorMsg, NavigationMsg, PreferencesMsg};
use super::*;

fn model() -> AppModel {
    AppModel::new(&carver_config::Config::default())
}

fn editor(mode: EditorMode) -> AppModel {
    let mut model = model();
    let _ = super::super::update(
        &mut model,
        AppMsg::Editor(EditorMsg::Load {
            note_id: NoteId::new(),
            revision: carver_sdk::Revision(1),
            source: "Selected text".into(),
        }),
    );
    let _ = super::super::update(
        &mut model,
        AppMsg::Preferences(PreferencesMsg::SetEditorMode(mode)),
    );
    model
}

fn label(command: CommandId, title: &str) -> CommandLabel {
    CommandLabel {
        command,
        title: title.into(),
        aliases: String::new(),
        icon: String::new(),
        shortcut: String::new(),
    }
}

fn open(model: &mut AppModel) -> (RequestId, RequestId) {
    update(
        model,
        PaletteMsg::Opened {
            labels: vec![
                label(CommandId::Preferences, "Preferences"),
                label(CommandId::Format(FormatCommand::Bold), "Bold"),
            ],
            source_selection: 0..8,
        },
    );
    let palette = palette(model);
    (palette.id, palette.request)
}

fn matched(model: &mut AppModel) {
    let palette = palette(model);
    let (id, request) = (palette.id, palette.request);
    let rows = match_rows(candidates(model, &palette.labels), &palette.query);
    update(model, PaletteMsg::Matched { id, request, rows });
}

#[test]
fn browser_should_offer_search_and_hide_formatting() {
    let model = model();
    assert!(available(&model, CommandId::SearchNotes));
    assert!(!available(&model, CommandId::Format(FormatCommand::Bold)));
}

#[test]
fn source_and_rich_should_offer_equivalent_formatting() {
    for mode in [EditorMode::Source, EditorMode::Rich] {
        let model = editor(mode);
        assert!(available(
            &model,
            CommandId::Format(FormatCommand::TaskList)
        ));
        assert!(available(&model, CommandId::Link));
    }
}

#[test]
fn preview_should_hide_formatting_and_keep_note_actions() {
    let model = editor(EditorMode::Rendered);
    assert!(!available(&model, CommandId::Heading(1)));
    assert!(available(&model, CommandId::Export));
}

#[test]
fn base_should_offer_configuration_without_ambiguous_creation() {
    let mut model = model();
    model.route = Route::Base;
    let id = BaseId::new();
    model.bases.selected = Some(id);
    model.bases.definitions.state = LoadState::Ready(vec![carver_sdk::BaseDefinition::defaults(
        id,
        "Projects".into(),
        Vec::new(),
        carver_sdk::Revision(1),
    )]);
    assert!(available(&model, CommandId::ConfigureBase));
    assert!(!available(&model, CommandId::NewNote));
}

#[test]
fn base_mutations_should_require_the_selected_definition_while_search_and_refresh_remain_available()
{
    let mut model = model();
    model.route = Route::Base;
    model.bases.selected = Some(BaseId::new());
    for state in [
        LoadState::Idle,
        LoadState::Loading(RequestId(1)),
        LoadState::Failed(UiError::new("offline")),
        LoadState::Ready(Vec::new()),
        LoadState::Ready(vec![carver_sdk::BaseDefinition::defaults(
            BaseId::new(),
            "Other".into(),
            Vec::new(),
            carver_sdk::Revision(1),
        )]),
    ] {
        model.bases.definitions.state = state;
        assert!(!available(&model, CommandId::ConfigureBase));
        assert!(!available(&model, CommandId::DeleteBase));
        assert!(available(&model, CommandId::SearchBase));
        assert!(available(&model, CommandId::RefreshBase));
    }
}

#[test]
fn trash_should_offer_refresh_without_item_actions() {
    let mut model = model();
    model.route = Route::Trash;
    assert!(available(&model, CommandId::RefreshTrash));
    assert!(!available(&model, CommandId::TrashNote));
}

#[test]
fn matching_should_accept_abbreviations_and_translated_labels() {
    let model = model();
    let labels = vec![label(CommandId::Preferences, "Préférences")];
    assert_eq!(match_rows(candidates(&model, &labels), "prfr").len(), 1);
}

#[test]
fn matching_should_rank_exact_before_prefix_and_fuzzy() {
    let model = model();
    let labels = vec![
        label(CommandId::About, "Alphabet"),
        label(CommandId::Preferences, "All Notes"),
        label(CommandId::Notes, "All"),
    ];
    let rows = match_rows(candidates(&model, &labels), "all");
    assert_eq!(rows[0].title, "All");
    assert_eq!(rows[1].title, "All Notes");
}

#[test]
fn empty_query_should_prioritize_context_and_leave_deletion_last() {
    let model = editor(EditorMode::Source);
    let labels = vec![
        label(CommandId::Preferences, "Preferences"),
        label(CommandId::TrashNote, "Trash"),
        label(CommandId::Find, "Find"),
    ];
    let rows = match_rows(candidates(&model, &labels), "");
    assert_eq!(rows[0].target, Target::Command(CommandId::Find));
    assert_eq!(
        rows.last().map(|row| row.target),
        Some(Target::Command(CommandId::TrashNote))
    );
}

#[test]
fn query_change_should_reject_previous_results() {
    let mut model = model();
    let (id, request) = open(&mut model);
    update(
        &mut model,
        PaletteMsg::QueryChanged {
            id,
            query: "new".into(),
        },
    );
    update(
        &mut model,
        PaletteMsg::NotesLoaded {
            id,
            request,
            result: Err(UiError::new("old error")),
        },
    );
    assert!(matches!(palette(&model).notes, LoadState::Loading(_)));
}

fn removed_destination_should_not_activate(
    model: &mut AppModel,
    target: Target,
    reply: super::super::LibraryReply,
) {
    let (id, _) = open(model);
    update(
        model,
        PaletteMsg::QueryChanged {
            id,
            query: "Projects".into(),
        },
    );
    matched(model);
    let request = palette(model).request;
    let rows = palette(model).local_rows.clone();
    assert_eq!(palette(model).selected, Some(target));

    match &reply {
        super::super::LibraryReply::SidebarLoaded { request_id, .. } => {
            model.sidebar.begin_reload(*request_id);
        }
        super::super::LibraryReply::BasesLoaded { request_id, .. } => {
            model.bases.definitions.begin_reload(*request_id);
        }
        _ => panic!("expected a catalog reply"),
    }
    let effects = super::super::update(model, AppMsg::Library(reply));
    assert!(
        effects
            .iter()
            .any(|effect| matches!(effect, Effect::MatchPalette { .. }))
    );
    assert_eq!(palette(model).local_rows, Vec::new());
    // A completion already queued before the catalog reply cannot restore the removed row.
    update(model, PaletteMsg::Matched { id, request, rows });
    assert_eq!(
        update(
            model,
            PaletteMsg::Activate {
                id,
                target: Some(target)
            }
        ),
        Vec::new()
    );
    assert!(model.palette.is_some());
    matched(model);
    assert!(palette(model).rows().iter().all(|row| row.target != target));
    assert_eq!(palette(model).selected, None);
}

#[test]
fn external_wakeup_should_defer_rematching_until_catalogs_reload() {
    let mut model = model();
    open(&mut model);
    matched(&mut model);
    let before = model.palette.clone();
    let effects = super::super::update(&mut model, AppMsg::LibraryChangedExternally);
    assert!(matches!(
        effects.as_slice(),
        [Effect::LoadLibraryRevision { .. }]
    ));
    assert_eq!(model.palette, before);
}

#[test]
fn sidebar_refresh_should_reject_removed_categories_before_matching_finishes() {
    let mut model = model();
    let id = CategoryId::new();
    model.sidebar.state = LoadState::Ready(vec![carver_sdk::CategorySummary {
        category: carver_sdk::Category {
            id,
            name: "Projects".into(),
            default_template_id: None,
            appearance: carver_sdk::CategoryAppearance::default(),
            position: 0,
            created_at: time::OffsetDateTime::UNIX_EPOCH,
            updated_at: time::OffsetDateTime::UNIX_EPOCH,
            trashed_at: None,
        },
        note_count: 0,
    }]);
    let request_id = model.next_request_id();
    let reply = super::super::LibraryReply::SidebarLoaded {
        request_id,
        result: Ok(Vec::new()),
    };
    removed_destination_should_not_activate(&mut model, Target::Category(id), reply);
}

#[test]
fn base_refresh_should_reject_removed_bases_before_matching_finishes() {
    let mut model = model();
    let id = BaseId::new();
    model.bases.definitions.state = LoadState::Ready(vec![carver_sdk::BaseDefinition::defaults(
        id,
        "Projects".into(),
        Vec::new(),
        carver_sdk::Revision(1),
    )]);
    let request_id = model.next_request_id();
    removed_destination_should_not_activate(
        &mut model,
        Target::Base(id),
        super::super::LibraryReply::BasesLoaded {
            request_id,
            result: Ok(Vec::new()),
        },
    );
}

#[test]
fn reopened_palette_should_reject_previous_lifetime() {
    let mut model = model();
    let (id, request) = open(&mut model);
    update(&mut model, PaletteMsg::Dismissed(id));
    open(&mut model);
    update(
        &mut model,
        PaletteMsg::Matched {
            id,
            request,
            rows: Vec::new(),
        },
    );
    assert!(!palette(&model).matched);
}

#[test]
fn navigation_should_clamp_at_list_boundaries() {
    let mut model = editor(EditorMode::Source);
    let (id, _) = open(&mut model);
    matched(&mut model);
    update(&mut model, PaletteMsg::Move { id, delta: -1 });
    assert_eq!(
        palette(&model).selected,
        Some(Target::Command(CommandId::Format(FormatCommand::Bold)))
    );
    for _ in 0..5 {
        update(&mut model, PaletteMsg::Move { id, delta: 1 });
    }
    assert_eq!(
        palette(&model).selected,
        Some(Target::Command(CommandId::Preferences))
    );
}

#[test]
fn activation_should_reject_changed_source_generation() {
    let mut model = editor(EditorMode::Source);
    let (id, _) = open(&mut model);
    matched(&mut model);
    let effects = update(&mut model, PaletteMsg::Activate { id, target: None });
    let Effect::FinishPalette {
        activation: Some(activation),
    } = effects[0].clone()
    else {
        panic!("activation")
    };
    model
        .editor
        .as_mut()
        .unwrap_or_else(|| panic!("expected editor fixture"))
        .source_generation += 1;
    assert_eq!(
        update(&mut model, PaletteMsg::Execute(activation)),
        Vec::new()
    );
}

#[test]
fn opening_and_dismissing_should_preserve_document_and_revision() {
    let mut model = editor(EditorMode::Source);
    let before = model.editor.clone();
    let (id, _) = open(&mut model);
    update(&mut model, PaletteMsg::Dismissed(id));
    assert_eq!(model.editor, before);
}

#[test]
fn failed_note_search_should_keep_commands_and_offer_retry() {
    let mut model = model();
    let (id, request) = open(&mut model);
    matched(&mut model);
    update(
        &mut model,
        PaletteMsg::NotesLoaded {
            id,
            request,
            result: Err(UiError::new("search failed")),
        },
    );
    assert!(
        palette(&model)
            .rows()
            .iter()
            .any(|row| row.target == Target::Retry)
    );
    assert_ne!(
        update(
            &mut model,
            PaletteMsg::Activate {
                id,
                target: Some(Target::Retry)
            }
        ),
        Vec::new()
    );
}

#[test]
fn move_should_be_disabled_until_categories_are_ready() {
    let mut model = editor(EditorMode::Source);
    for state in [
        LoadState::Idle,
        LoadState::Loading(RequestId(1)),
        LoadState::Failed(UiError::new("offline")),
    ] {
        model.sidebar.state = state;
        assert!(disabled(&model, CommandId::Move));
    }
    model.sidebar.state = LoadState::Ready(Vec::new());
    assert!(!disabled(&model, CommandId::Move));
}

#[test]
fn sidebar_reload_should_refresh_and_disable_the_move_command() {
    let mut model = editor(EditorMode::Source);
    model.sidebar.state = LoadState::Ready(Vec::new());
    update(
        &mut model,
        PaletteMsg::Opened {
            labels: vec![label(CommandId::Move, "Move note")],
            source_selection: 0..0,
        },
    );
    matched(&mut model);
    let id = palette(&model).id;
    assert!(!palette(&model).local_rows[0].disabled);
    let effects = super::super::update(
        &mut model,
        AppMsg::Sidebar(super::super::SidebarMsg::Reload),
    );
    assert!(
        effects
            .iter()
            .any(|effect| matches!(effect, Effect::MatchPalette { .. }))
    );
    assert_eq!(
        update(&mut model, PaletteMsg::Activate { id, target: None }),
        Vec::new()
    );
    matched(&mut model);
    assert!(palette(&model).local_rows[0].disabled);
    assert_eq!(palette(&model).selected, None);
    let LoadState::Loading(request_id) = model.sidebar.state else {
        panic!("loading sidebar")
    };
    let _ = super::super::update(
        &mut model,
        AppMsg::Library(super::super::LibraryReply::SidebarLoaded {
            request_id,
            result: Ok(Vec::new()),
        }),
    );
    matched(&mut model);
    assert!(!palette(&model).local_rows[0].disabled);
}

#[test]
fn external_change_should_disable_formatting() {
    let mut model = editor(EditorMode::Source);
    model
        .editor
        .as_mut()
        .unwrap_or_else(|| panic!("expected editor fixture"))
        .external_change = Some(super::super::model::ExternalChange::Deleted);
    assert!(disabled(&model, CommandId::Format(FormatCommand::Bold)));
}

#[test]
fn save_failure_should_offer_retry_without_changing_the_open_palette_query() {
    let mut model = editor(EditorMode::Source);
    let document = model.editor.as_mut().unwrap_or_else(|| panic!("editor"));
    document.source_changed("Changed text".into());
    let request = document.begin_save().unwrap_or_else(|| panic!("save"));
    update(
        &mut model,
        PaletteMsg::Opened {
            labels: vec![label(CommandId::RetrySave, "Retry save")],
            source_selection: 0..0,
        },
    );
    let id = palette(&model).id;
    update(
        &mut model,
        PaletteMsg::QueryChanged {
            id,
            query: "retry".into(),
        },
    );
    matched(&mut model);
    assert_eq!(palette(&model).local_rows, []);
    let effects = super::super::update(
        &mut model,
        AppMsg::Library(super::super::LibraryReply::EditorSaved {
            request,
            move_error: None,
            result: Err(UiError::new("offline")),
        }),
    );
    assert!(
        effects
            .iter()
            .any(|effect| matches!(effect, Effect::MatchPalette { .. }))
    );
    matched(&mut model);
    assert_eq!(palette(&model).query, "retry");
    assert_eq!(
        palette(&model).local_rows[0].target,
        Target::Command(CommandId::RetrySave)
    );
}

#[test]
fn asset_completion_should_enable_formatting_without_reopening_the_palette() {
    let mut model = editor(EditorMode::Source);
    let document = model.editor.as_mut().unwrap_or_else(|| panic!("editor"));
    document.pending_assets = 1;
    let session = document.session;
    open(&mut model);
    matched(&mut model);
    let bold = Target::Command(CommandId::Format(FormatCommand::Bold));
    assert!(
        palette(&model)
            .local_rows
            .iter()
            .find(|row| row.target == bold)
            .is_some_and(|row| row.disabled)
    );
    let effects = super::super::update(
        &mut model,
        AppMsg::Library(super::super::LibraryReply::EditorAssetStored {
            image: true,
            session,
            alt: String::new(),
            source_target: None,
            result: Ok("assets/image.png".into()),
        }),
    );
    assert!(
        effects
            .iter()
            .any(|effect| matches!(effect, Effect::MatchPalette { .. }))
    );
    matched(&mut model);
    assert!(
        palette(&model)
            .local_rows
            .iter()
            .find(|row| row.target == bold)
            .is_some_and(|row| !row.disabled)
    );
}

#[test]
fn source_selection_should_reflect_existing_marks() {
    let mut model = editor(EditorMode::Source);
    let _ = super::super::update(
        &mut model,
        AppMsg::Editor(EditorMsg::SourceChanged("*Selected* text".into())),
    );
    open(&mut model);
    assert!(active(&model, CommandId::Format(FormatCommand::Bold)));
}

#[test]
fn activation_should_reject_another_route() {
    let mut model = model();
    let (id, _) = open(&mut model);
    matched(&mut model);
    let effects = update(&mut model, PaletteMsg::Activate { id, target: None });
    let Effect::FinishPalette {
        activation: Some(activation),
    } = effects[0].clone()
    else {
        panic!("activation")
    };
    let _ = super::super::update(&mut model, AppMsg::Navigation(NavigationMsg::ShowTrash));
    assert_eq!(
        update(&mut model, PaletteMsg::Execute(activation)),
        Vec::new()
    );
}

fn palette(model: &AppModel) -> &PaletteModel {
    let Some(palette) = &model.palette else {
        panic!("expected palette fixture");
    };
    palette
}

#[test]
fn cancellation_should_restore_the_captured_source_selection() {
    let mut model = editor(EditorMode::Source);
    let (id, _) = open(&mut model);
    assert!(
        update(&mut model, PaletteMsg::Dismissed(id))
            .iter()
            .any(|effect| matches!(
                effect,
                Effect::SelectEditorSource { selection, .. } if selection == &(0..8)
            ))
    );
}

#[test]
fn cancellation_should_not_restore_selection_after_the_document_changes() {
    let mut model = editor(EditorMode::Source);
    let (id, _) = open(&mut model);
    let _ = super::super::update(
        &mut model,
        AppMsg::Editor(EditorMsg::SourceChanged("Changed".into())),
    );
    assert_eq!(
        update(&mut model, PaletteMsg::Dismissed(id)),
        vec![Effect::FinishPalette { activation: None }]
    );
}

#[test]
fn image_width_should_use_the_captured_source_selection_after_the_palette_closes() {
    let mut model = editor(EditorMode::Source);
    let _ = super::super::update(
        &mut model,
        AppMsg::Editor(EditorMsg::SourceChanged(
            "![Image](assets/example.png)".into(),
        )),
    );
    update(
        &mut model,
        PaletteMsg::Opened {
            labels: vec![label(CommandId::ImageWidth(Some(50)), "Image width: 50%")],
            source_selection: 3..3,
        },
    );
    matched(&mut model);
    let id = palette(&model).id;
    let effects = update(&mut model, PaletteMsg::Activate { id, target: None });
    let Effect::FinishPalette {
        activation: Some(activation),
    } = effects[0].clone()
    else {
        panic!("activation")
    };
    assert!(model.palette.is_none());
    assert_eq!(
        update(&mut model, PaletteMsg::Execute(activation)),
        vec![Effect::ExecutePaletteCommand {
            command: CommandId::ImageWidth(Some(50)),
            source_selection: 3..3
        }]
    );
}

#[test]
fn explicit_search_should_select_a_destructive_command_without_running_it() {
    let mut model = editor(EditorMode::Rich);
    update(
        &mut model,
        PaletteMsg::Opened {
            labels: vec![label(CommandId::TrashNote, "Move note to Trash")],
            source_selection: 0..0,
        },
    );
    let id = palette(&model).id;
    update(
        &mut model,
        PaletteMsg::QueryChanged {
            id,
            query: "Move note to Trash".into(),
        },
    );
    matched(&mut model);
    assert_eq!(
        palette(&model).selected,
        Some(Target::Command(CommandId::TrashNote))
    );
    assert!(model.editor.is_some());
}

#[test]
fn rich_selection_should_reflect_marks_from_the_current_projection() {
    let mut model = editor(EditorMode::Rich);
    let doc = model
        .editor
        .as_ref()
        .unwrap_or_else(|| panic!("editor fixture"));
    let message = EditorMsg::DocumentSelectionChanged {
        session: doc.session,
        mode: EditorMode::Rich,
        source: std::rc::Rc::from(doc.source.as_str()),
        media: None,
        heading: None,
        formatting: Some(carver_editor_protocol::SelectionState {
            active: vec!["bold".into()],
            ..Default::default()
        }),
    };
    let _ = super::super::update(&mut model, AppMsg::Editor(message));
    assert!(active(&model, CommandId::Format(FormatCommand::Bold)));
}

#[test]
fn rich_selection_should_ignore_marks_from_a_stale_source_projection() {
    let mut model = editor(EditorMode::Rich);
    let doc = model
        .editor
        .as_ref()
        .unwrap_or_else(|| panic!("editor fixture"));
    let message = EditorMsg::DocumentSelectionChanged {
        session: doc.session,
        mode: EditorMode::Rich,
        source: std::rc::Rc::from("Old source"),
        media: None,
        heading: None,
        formatting: Some(carver_editor_protocol::SelectionState {
            active: vec!["bold".into()],
            ..Default::default()
        }),
    };
    let _ = super::super::update(&mut model, AppMsg::Editor(message));
    assert!(!active(&model, CommandId::Format(FormatCommand::Bold)));
}
