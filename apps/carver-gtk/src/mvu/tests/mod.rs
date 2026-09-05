use carver_config::{AppPaths, Config};
use carver_sdk::{CategoryId, DocumentImportFormat, LibraryRevision, NoteId, Revision};
use carver_sdk::{LibraryBackend, LibraryClient};
use carver_storage_sqlite::SqliteLibrary;
use gtk::gio::prelude::FileExt;

use super::{
    ActionKey, ActionMsg, AppDispatcher, AppModel, AppMsg, AppRuntime, BrowserMsg,
    EditorExportFormat, EditorMsg, Effect, LibraryReply, LoadState, NavigationMsg, RequestId,
    Route, SidebarMsg, TrashMsg, TrashMutation, UiError, update,
};

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
            Effect::LoadBrowser {
                request_id: RequestId(2),
                category_id: None,
                query: String::new(),
            },
            Effect::LoadLibraryRevision {
                request_id: RequestId(3),
            },
        ]
    );
    assert_eq!(model.sidebar.state, LoadState::Loading(RequestId(1)));
    assert_eq!(model.browser.notes.state, LoadState::Loading(RequestId(2)));
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
            Effect::LoadBrowser {
                request_id: RequestId(3),
                category_id: None,
                query: String::new(),
            },
            Effect::LoadTrash {
                request_id: RequestId(4),
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
fn trashing_the_open_editor_note_should_close_its_session_before_the_effect_runs() {
    let mut model = AppModel::new(&Config::default());
    let note_id = NoteId::new();
    let _ = update(
        &mut model,
        AppMsg::Editor(EditorMsg::Load {
            note_id,
            revision: Revision(1),
            source: String::from("Open note"),
        }),
    );

    let effects = update(&mut model, AppMsg::Action(ActionMsg::TrashNote(note_id)));

    assert_eq!(model.route, Route::Browser);
    assert_eq!(model.editor, None);
    assert_eq!(effects, vec![Effect::TrashNote { note_id }]);
}

#[test]
fn a_successful_note_trash_should_offer_undo_until_the_note_is_restored() {
    let mut model = AppModel::new(&Config::default());
    let note_id = NoteId::new();
    let _ = update(&mut model, AppMsg::Action(ActionMsg::TrashNote(note_id)));
    let _ = update(
        &mut model,
        AppMsg::Library(LibraryReply::ActionFinished {
            action: ActionKey::TrashNote(note_id),
            result: Ok(()),
        }),
    );
    assert_eq!(model.undo_trash_note, Some(note_id));

    assert_eq!(
        update(&mut model, AppMsg::Trash(TrashMsg::RestoreNote(note_id))),
        vec![Effect::RestoreNote { note_id }]
    );
    assert_eq!(model.undo_trash_note, None);
}

#[test]
fn stale_editor_close_should_not_close_a_newer_document() {
    let mut model = AppModel::new(&Config::default());
    let first_note_id = NoteId::new();
    let _ = update(
        &mut model,
        AppMsg::Editor(EditorMsg::Load {
            note_id: first_note_id,
            revision: Revision(1),
            source: "first".to_owned(),
        }),
    );
    let Some(first_session) = model.editor.as_ref().map(|document| document.session) else {
        panic!("editor should be open");
    };
    let _ = update(
        &mut model,
        AppMsg::Editor(EditorMsg::Load {
            note_id: NoteId::new(),
            revision: Revision(2),
            source: "second".to_owned(),
        }),
    );

    let _ = update(&mut model, AppMsg::Editor(EditorMsg::Close(first_session)));

    assert_eq!(model.route, Route::Editor);
    assert_ne!(
        model.editor.as_ref().map(|document| document.session),
        Some(first_session)
    );
}

#[test]
fn source_event_after_editor_close_should_be_ignored() {
    let mut model = AppModel::new(&Config::default());
    let _ = update(
        &mut model,
        AppMsg::Editor(EditorMsg::Load {
            note_id: NoteId::new(),
            revision: Revision(1),
            source: "active".to_owned(),
        }),
    );
    let Some(session) = model.editor.as_ref().map(|document| document.session) else {
        panic!("editor should be open");
    };
    let _ = update(&mut model, AppMsg::Editor(EditorMsg::Close(session)));

    assert!(
        update(
            &mut model,
            AppMsg::Editor(EditorMsg::SourceChanged("stale widget event".to_owned())),
        )
        .is_empty()
    );
    assert_eq!(model.route, Route::Browser);
    assert_eq!(model.editor, None);
}

#[test]
fn pasted_image_should_store_an_asset_and_update_the_current_document() {
    let mut model = AppModel::new(&Config::default());
    let note_id = NoteId::new();
    let _ = update(
        &mut model,
        AppMsg::Editor(EditorMsg::Load {
            note_id,
            revision: Revision(1),
            source: "Before".to_owned(),
        }),
    );
    let Some(session) = model.editor.as_ref().map(|document| document.session) else {
        panic!("editor should be open");
    };

    assert_eq!(
        update(
            &mut model,
            AppMsg::Editor(EditorMsg::PasteImage {
                extension: "png".to_owned(),
                bytes: vec![1, 2, 3],
            }),
        ),
        vec![Effect::StoreEditorAsset {
            session,
            note_id,
            extension: "png".to_owned(),
            bytes: vec![1, 2, 3],
            alt: "Pasted image".to_owned(),
            source_selection: None,
        }]
    );
    let effects = update(
        &mut model,
        AppMsg::Library(LibraryReply::EditorAssetStored {
            session,
            alt: "Pasted image".to_owned(),
            source_selection: None,
            result: Ok("assets/pasted.png".to_owned()),
        }),
    );

    assert!(
        matches!(effects.as_slice(), [Effect::SchedulePreview { session: preview_session, .. }, Effect::ScheduleEditorSave { session: save_session, .. }] if *preview_session == session && *save_session == session)
    );
    assert_eq!(
        model
            .editor
            .as_ref()
            .map(|document| document.source.as_str()),
        Some("Before\n![Pasted image](assets/pasted.png)\n")
    );
    let _ = update(&mut model, AppMsg::Editor(EditorMsg::Close(session)));
    assert!(
        update(
            &mut model,
            AppMsg::Library(LibraryReply::EditorAssetStored {
                session,
                alt: "Pasted image".to_owned(),
                source_selection: None,
                result: Ok("assets/stale.png".to_owned()),
            }),
        )
        .is_empty()
    );
    assert_eq!(model.editor, None);
}

#[test]
fn source_image_paste_should_replace_the_captured_cursor_selection() {
    let mut model = AppModel::new(&Config::default());
    let note_id = NoteId::new();
    let _ = update(
        &mut model,
        AppMsg::Editor(EditorMsg::Load {
            note_id,
            revision: Revision(1),
            source: "Before remove After".to_owned(),
        }),
    );
    let Some(session) = model.editor.as_ref().map(|document| document.session) else {
        panic!("editor should be open");
    };
    let selection = 7..13;

    let effects = update(
        &mut model,
        AppMsg::Editor(EditorMsg::ImportImage {
            extension: "png".to_owned(),
            bytes: vec![1, 2, 3],
            alt: "Pasted image".to_owned(),
            source_selection: Some(selection.clone()),
        }),
    );
    assert!(
        matches!(effects.as_slice(), [Effect::StoreEditorAsset { source_selection: Some(actual), .. }] if actual == &selection)
    );

    let _ = update(
        &mut model,
        AppMsg::Library(LibraryReply::EditorAssetStored {
            session,
            alt: "Pasted image".to_owned(),
            source_selection: Some(selection),
            result: Ok("assets/pasted.png".to_owned()),
        }),
    );
    assert_eq!(
        model
            .editor
            .as_ref()
            .map(|document| document.source.as_str()),
        Some("Before ![Pasted image](assets/pasted.png) After")
    );
}

#[test]
fn source_change_should_update_the_canonical_document_and_mark_it_dirty() {
    let mut model = AppModel::new(&Config::default());
    let note_id = NoteId::new();
    let unsupported_source = "::: unsupported Carve block\\nverbatim".to_owned();
    let _ = update(
        &mut model,
        AppMsg::Editor(EditorMsg::Load {
            note_id,
            revision: Revision(4),
            source: "Initial source".to_owned(),
        }),
    );

    let effects = update(
        &mut model,
        AppMsg::Editor(EditorMsg::SourceChanged(unsupported_source.clone())),
    );

    assert_eq!(
        effects,
        vec![Effect::SchedulePreview {
            session: super::EditorSessionId(1),
            timer_id: super::TimerId(1),
        }]
    );
    let effects = update(&mut model, AppMsg::Editor(EditorMsg::AutosaveRequested));

    assert_eq!(
        effects,
        vec![Effect::ScheduleEditorSave {
            session: super::EditorSessionId(1),
            timer_id: super::TimerId(1),
            delay_ms: 500,
        }]
    );
    let Some(document) = model.editor.as_ref() else {
        panic!("editor should remain open");
    };
    assert_eq!(document.session, super::EditorSessionId(1));
    assert_eq!(document.note_id, note_id);
    assert_eq!(document.revision, Revision(4));
    assert_eq!(document.source, unsupported_source);
    assert_eq!(document.mode, carver_config::EditorMode::Rich);
    assert_eq!(document.save_state, super::EditorSaveState::Dirty);
}

#[test]
fn copy_request_should_snapshot_unsaved_canonical_source_and_report_omissions() {
    let mut model = AppModel::new(&Config::default());
    let _ = update(
        &mut model,
        AppMsg::Editor(EditorMsg::Load {
            note_id: NoteId::new(),
            revision: Revision(1),
            source: "Saved source".to_owned(),
        }),
    );
    let _ = update(
        &mut model,
        AppMsg::Editor(EditorMsg::SourceChanged("Unsaved source".to_owned())),
    );

    assert!(update(&mut model, AppMsg::Editor(EditorMsg::CopyRequested)).is_empty());
    assert_eq!(
        model
            .editor_copy_request
            .as_ref()
            .map(|request| request.source.as_str()),
        Some("Unsaved source")
    );
    let request_id = model
        .editor_copy_request
        .as_ref()
        .map(|request| request.request_id)
        .unwrap_or_default();

    assert!(
        update(
            &mut model,
            AppMsg::Editor(EditorMsg::CopyCompleted {
                request_id,
                omitted_images: 2,
            }),
        )
        .is_empty()
    );
    assert_eq!(model.editor_copy_request, None);
    assert_eq!(
        model.notice.as_ref().map(|notice| notice.message.as_str()),
        Some("Note copied; 2 images were omitted.")
    );
}

#[test]
fn stale_copy_completion_should_not_replace_the_current_copy_request() {
    let mut model = AppModel::new(&Config::default());
    let _ = update(
        &mut model,
        AppMsg::Editor(EditorMsg::Load {
            note_id: NoteId::new(),
            revision: Revision(1),
            source: String::new(),
        }),
    );
    let _ = update(&mut model, AppMsg::Editor(EditorMsg::CopyRequested));
    let request_id = model
        .editor_copy_request
        .as_ref()
        .map(|request| request.request_id)
        .unwrap_or_default();

    assert!(
        update(
            &mut model,
            AppMsg::Editor(EditorMsg::CopyCompleted {
                request_id: request_id.wrapping_add(1),
                omitted_images: 0,
            }),
        )
        .is_empty()
    );
    assert_eq!(
        model
            .editor_copy_request
            .as_ref()
            .map(|request| request.request_id),
        Some(request_id)
    );
}

#[test]
fn export_should_prepare_the_unsaved_editor_snapshot_before_writing() {
    let mut model = AppModel::new(&Config::default());
    let note_id = NoteId::new();
    let _ = update(
        &mut model,
        AppMsg::Editor(EditorMsg::Load {
            note_id,
            revision: Revision(1),
            source: String::from("Saved"),
        }),
    );
    let _ = update(
        &mut model,
        AppMsg::Editor(EditorMsg::SourceChanged(String::from("# Unsaved draft"))),
    );
    let _ = update(&mut model, AppMsg::Editor(EditorMsg::ExportDialogRequested));
    let dialog = model
        .editor_export_dialog_request
        .clone()
        .unwrap_or_else(|| panic!("export dialog should capture a snapshot"));

    let effects = update(
        &mut model,
        AppMsg::Editor(EditorMsg::ExportRequested {
            request_id: dialog.request_id,
            format: EditorExportFormat::Carve,
            include_assets: false,
            target_uri: String::from("file:///tmp/draft.carve"),
        }),
    );

    assert!(matches!(
        effects.as_slice(),
        [Effect::PrepareEditorExport { source, note_id: effect_note_id, .. }]
            if source == "# Unsaved draft" && effect_note_id == &note_id
    ));
    let effects = update(
        &mut model,
        AppMsg::Library(LibraryReply::EditorExportPrepared {
            request_id: dialog.request_id,
            session: dialog.session,
            result: Ok(Vec::new()),
        }),
    );
    assert_eq!(
        effects,
        vec![Effect::WriteEditorExport {
            request_id: dialog.request_id
        }]
    );
}

#[test]
fn export_warning_should_require_confirmation_before_writing() {
    let mut model = AppModel::new(&Config::default());
    let _ = update(
        &mut model,
        AppMsg::Editor(EditorMsg::Load {
            note_id: NoteId::new(),
            revision: Revision(1),
            source: String::from("# Note"),
        }),
    );
    let _ = update(&mut model, AppMsg::Editor(EditorMsg::ExportDialogRequested));
    let dialog = model
        .editor_export_dialog_request
        .clone()
        .unwrap_or_else(|| panic!("export dialog should be present"));
    let _ = update(
        &mut model,
        AppMsg::Editor(EditorMsg::ExportRequested {
            request_id: dialog.request_id,
            format: EditorExportFormat::Markdown,
            include_assets: false,
            target_uri: String::from("file:///tmp/note.md"),
        }),
    );
    let effects = update(
        &mut model,
        AppMsg::Library(LibraryReply::EditorExportPrepared {
            request_id: dialog.request_id,
            session: dialog.session,
            result: Ok(vec![String::from(
                "Markdown cannot represent one construct exactly.",
            )]),
        }),
    );

    assert!(effects.is_empty());
    assert_eq!(
        model
            .editor_export_warning_request
            .as_ref()
            .map(|request| request.request_id),
        Some(dialog.request_id)
    );
    assert_eq!(
        update(
            &mut model,
            AppMsg::Editor(EditorMsg::ExportCancelled {
                request_id: dialog.request_id,
            }),
        ),
        vec![Effect::DiscardEditorExport {
            request_id: dialog.request_id,
        }]
    );
}

#[test]
fn closing_print_dialog_should_clear_the_request_without_an_error_notice() {
    let mut model = AppModel::new(&Config::default());
    let note_id = NoteId::new();
    let _ = update(
        &mut model,
        AppMsg::Editor(EditorMsg::Load {
            note_id,
            revision: Revision(1),
            source: "print this".to_owned(),
        }),
    );
    let _ = update(&mut model, AppMsg::Editor(EditorMsg::PrintRequested));
    let Some(request_id) = model
        .editor_pdf_export_request
        .as_ref()
        .map(|request| request.request_id)
    else {
        panic!("print request should be pending");
    };

    let effects = update(
        &mut model,
        AppMsg::Editor(EditorMsg::PdfExportCancelled { request_id }),
    );

    assert!(effects.is_empty());
    assert!(model.editor_pdf_export_request.is_none());
    assert!(model.notice.is_none());
}

#[test]
fn stale_export_preparation_should_discard_its_artifact() {
    let mut model = AppModel::new(&Config::default());

    let effects = update(
        &mut model,
        AppMsg::Library(LibraryReply::EditorExportPrepared {
            request_id: 42,
            session: super::EditorSessionId(7),
            result: Ok(Vec::new()),
        }),
    );

    assert_eq!(
        effects,
        vec![Effect::DiscardEditorExport { request_id: 42 }]
    );
}

#[test]
fn latest_preview_timer_should_reject_a_superseded_source_snapshot() {
    let mut model = AppModel::new(&Config::default());
    let _ = update(
        &mut model,
        AppMsg::Editor(EditorMsg::Load {
            note_id: NoteId::new(),
            revision: Revision(1),
            source: "Initial".to_owned(),
        }),
    );
    let session = super::EditorSessionId(1);

    let _ = update(
        &mut model,
        AppMsg::Editor(EditorMsg::SourceChanged("First".to_owned())),
    );
    let _ = update(
        &mut model,
        AppMsg::Editor(EditorMsg::SourceChanged("Final".to_owned())),
    );

    assert!(
        update(
            &mut model,
            AppMsg::Editor(EditorMsg::PreviewElapsed {
                session,
                timer_id: super::TimerId(1),
            }),
        )
        .is_empty()
    );
    assert_eq!(
        model
            .editor_preview
            .as_ref()
            .map(|preview| preview.source.as_str()),
        Some("Initial")
    );
    let _ = update(
        &mut model,
        AppMsg::Editor(EditorMsg::PreviewElapsed {
            session,
            timer_id: super::TimerId(2),
        }),
    );
    assert_eq!(
        model
            .editor_preview
            .as_ref()
            .map(|preview| preview.source.as_str()),
        Some("Final")
    );
}

#[test]
fn theme_change_should_request_an_editor_projection_refresh() {
    let mut model = AppModel::new(&Config::default());

    assert!(update(&mut model, AppMsg::Editor(EditorMsg::ThemeChanged)).is_empty());

    assert_eq!(model.editor_theme_revision, 1);
}

#[test]
fn source_line_number_preference_should_persist_a_complete_config_snapshot() {
    let mut model = AppModel::new(&Config::default());

    let effects = update(
        &mut model,
        AppMsg::Preferences(super::PreferencesMsg::SetSourceLineNumbers(true)),
    );

    assert!(
        matches!(effects.as_slice(), [Effect::PersistConfig { config }] if config.editor.source_line_numbers)
    );
    assert!(model.preferences.source_editor.show_line_numbers);
}

#[test]
fn formatting_toolbar_preference_should_persist_a_complete_config_snapshot() {
    let mut model = AppModel::new(&Config::default());

    let effects = update(
        &mut model,
        AppMsg::Preferences(super::PreferencesMsg::SetFormattingToolbarVisible(false)),
    );

    assert!(
        matches!(effects.as_slice(), [Effect::PersistConfig { config }] if !config.editor.show_formatting_toolbar)
    );
    assert!(!model.preferences.show_formatting_toolbar);
}

#[test]
fn current_line_preference_should_persist_a_complete_config_snapshot() {
    let mut model = AppModel::new(&Config::default());

    let effects = update(
        &mut model,
        AppMsg::Preferences(super::PreferencesMsg::SetSourceHighlightCurrentLine(true)),
    );

    assert!(
        matches!(effects.as_slice(), [Effect::PersistConfig { config }] if config.editor.source_highlight_current_line)
    );
    assert!(model.preferences.source_editor.highlight_current_line);
}

#[test]
fn writing_focus_syntax_style_should_persist_a_complete_config_snapshot() {
    let mut model = AppModel::new(&Config::default());

    let effects = update(
        &mut model,
        AppMsg::Preferences(super::PreferencesMsg::SetSourceSyntaxStyle(
            carver_config::SourceSyntaxStyle::WritingFocus,
        )),
    );

    assert!(
        matches!(effects.as_slice(), [Effect::PersistConfig { config }] if config.editor.source_syntax_style == carver_config::SourceSyntaxStyle::WritingFocus)
    );
    assert_eq!(
        model.preferences.source_editor.syntax_style,
        carver_config::SourceSyntaxStyle::WritingFocus
    );
}

#[test]
fn source_font_preference_should_persist_the_selected_font() {
    let mut model = AppModel::new(&Config::default());
    let font = "Adwaita Mono 13".to_owned();

    let effects = update(
        &mut model,
        AppMsg::Preferences(super::PreferencesMsg::SetSourceFont(Some(font.clone()))),
    );

    assert!(
        matches!(effects.as_slice(), [Effect::PersistConfig { config }] if config.editor.source_font.as_deref() == Some("Adwaita Mono 13"))
    );
    assert_eq!(model.preferences.source_editor.font, Some(font));
}

#[test]
fn source_font_reset_should_restore_the_system_font_setting() {
    let mut config = Config::default();
    config.editor.source_font = Some("Adwaita Mono 13".to_owned());
    let mut model = AppModel::new(&config);

    let effects = update(
        &mut model,
        AppMsg::Preferences(super::PreferencesMsg::SetSourceFont(None)),
    );

    assert!(
        matches!(effects.as_slice(), [Effect::PersistConfig { config }] if config.editor.source_font.is_none())
    );
    assert_eq!(model.preferences.source_editor.font, None);
}

#[test]
fn close_geometry_should_persist_a_complete_config_snapshot() {
    let mut model = AppModel::new(&Config::default());

    let effects = update(
        &mut model,
        AppMsg::Window(super::WindowMsg::SaveGeometry {
            width: 900,
            height: 640,
            maximized: true,
        }),
    );

    assert!(
        matches!(effects.as_slice(), [Effect::PersistConfig { config }] if config.window.width == 900 && config.window.height == 640 && config.window.maximized)
    );
}

#[test]
fn source_change_while_saving_should_start_one_follow_up_save() {
    let mut model = AppModel::new(&Config::default());
    let note_id = NoteId::new();
    let _ = update(
        &mut model,
        AppMsg::Editor(EditorMsg::Load {
            note_id,
            revision: Revision(4),
            source: "Initial".to_owned(),
        }),
    );
    let _ = update(
        &mut model,
        AppMsg::Editor(EditorMsg::SourceChanged("First save".to_owned())),
    );
    let _ = update(&mut model, AppMsg::Editor(EditorMsg::AutosaveRequested));
    let effects = update(
        &mut model,
        AppMsg::Editor(EditorMsg::AutosaveElapsed {
            session: super::EditorSessionId(1),
            timer_id: super::TimerId(1),
        }),
    );
    let first_request = match effects.as_slice() {
        [Effect::SaveNote { request }] => request.clone(),
        _ => panic!("the autosave timer should begin one save"),
    };

    let effects = update(
        &mut model,
        AppMsg::Editor(EditorMsg::SourceChanged("Final source".to_owned())),
    );
    assert!(matches!(
        effects.as_slice(),
        [Effect::SchedulePreview { .. }]
    ));
    let effects = update(
        &mut model,
        AppMsg::Library(LibraryReply::EditorSaved {
            request: first_request,
            result: Ok(Revision(5)),
        }),
    );

    assert_eq!(
        effects,
        vec![Effect::SaveNote {
            request: super::EditorSaveRequest {
                session: super::EditorSessionId(1),
                note_id,
                expected_revision: Revision(5),
                source: "Final source".to_owned(),
            },
        }]
    );
}

#[test]
fn stale_editor_save_completion_should_not_replace_a_newer_document() {
    let mut model = AppModel::new(&Config::default());
    let first_note_id = NoteId::new();
    let _ = update(
        &mut model,
        AppMsg::Editor(EditorMsg::Load {
            note_id: first_note_id,
            revision: Revision(1),
            source: "First".to_owned(),
        }),
    );
    let _ = update(
        &mut model,
        AppMsg::Editor(EditorMsg::SourceChanged("First changed".to_owned())),
    );
    let _ = update(&mut model, AppMsg::Editor(EditorMsg::AutosaveRequested));
    let effects = update(
        &mut model,
        AppMsg::Editor(EditorMsg::AutosaveElapsed {
            session: super::EditorSessionId(1),
            timer_id: super::TimerId(1),
        }),
    );
    let first_request = match effects.as_slice() {
        [Effect::SaveNote { request }] => request.clone(),
        _ => panic!("the autosave timer should begin one save"),
    };
    let second_note_id = NoteId::new();
    let _ = update(
        &mut model,
        AppMsg::Editor(EditorMsg::Load {
            note_id: second_note_id,
            revision: Revision(8),
            source: "Second".to_owned(),
        }),
    );

    let effects = update(
        &mut model,
        AppMsg::Library(LibraryReply::EditorSaved {
            request: first_request,
            result: Ok(Revision(2)),
        }),
    );

    assert!(effects.is_empty());
    let Some(document) = model.editor.as_ref() else {
        panic!("second editor should remain open");
    };
    assert_eq!(document.note_id, second_note_id);
    assert_eq!(document.revision, Revision(8));
    assert_eq!(document.source, "Second");
    assert_eq!(document.save_state, super::EditorSaveState::Clean);
}

#[test]
fn failed_editor_save_should_preserve_source_and_retry_on_request() {
    let mut model = AppModel::new(&Config::default());
    let note_id = NoteId::new();
    let _ = update(
        &mut model,
        AppMsg::Editor(EditorMsg::Load {
            note_id,
            revision: Revision(3),
            source: "Initial".to_owned(),
        }),
    );
    let _ = update(
        &mut model,
        AppMsg::Editor(EditorMsg::SourceChanged("Unsaved source".to_owned())),
    );
    let _ = update(&mut model, AppMsg::Editor(EditorMsg::AutosaveRequested));
    let effects = update(
        &mut model,
        AppMsg::Editor(EditorMsg::AutosaveElapsed {
            session: super::EditorSessionId(1),
            timer_id: super::TimerId(1),
        }),
    );
    let request = match effects.as_slice() {
        [Effect::SaveNote { request }] => request.clone(),
        _ => panic!("the autosave timer should begin one save"),
    };
    let error = UiError::new("save failed");
    let _ = update(
        &mut model,
        AppMsg::Library(LibraryReply::EditorSaved {
            request: request.clone(),
            result: Err(error.clone()),
        }),
    );

    let Some(document) = model.editor.as_ref() else {
        panic!("editor should remain open");
    };
    assert_eq!(document.source, "Unsaved source");
    assert_eq!(document.save_state, super::EditorSaveState::Failed(error));
    assert_eq!(model.notice, None);
    assert_eq!(
        update(&mut model, AppMsg::Editor(EditorMsg::RetrySave)),
        vec![Effect::SaveNote { request }]
    );
}

#[test]
fn back_requested_while_saving_should_close_only_after_the_latest_source_saves() {
    let mut model = AppModel::new(&Config::default());
    let note_id = NoteId::new();
    let _ = update(
        &mut model,
        AppMsg::Editor(EditorMsg::Load {
            note_id,
            revision: Revision(7),
            source: "Initial".to_owned(),
        }),
    );
    let _ = update(
        &mut model,
        AppMsg::Editor(EditorMsg::SourceChanged("First save".to_owned())),
    );
    let _ = update(&mut model, AppMsg::Editor(EditorMsg::AutosaveRequested));
    let effects = update(
        &mut model,
        AppMsg::Editor(EditorMsg::AutosaveElapsed {
            session: super::EditorSessionId(1),
            timer_id: super::TimerId(1),
        }),
    );
    let first_request = match effects.as_slice() {
        [Effect::SaveNote { request }] => request.clone(),
        _ => panic!("the autosave timer should begin one save"),
    };

    assert!(update(&mut model, AppMsg::Editor(EditorMsg::BackRequested)).is_empty());
    let _ = update(
        &mut model,
        AppMsg::Editor(EditorMsg::SourceChanged("Final source".to_owned())),
    );
    let effects = update(
        &mut model,
        AppMsg::Library(LibraryReply::EditorSaved {
            request: first_request,
            result: Ok(Revision(8)),
        }),
    );
    let final_request = match effects.as_slice() {
        [Effect::SaveNote { request }] => request.clone(),
        _ => panic!("a changed source should schedule one follow-up save"),
    };
    assert_eq!(model.route, Route::Editor);

    let effects = update(
        &mut model,
        AppMsg::Library(LibraryReply::EditorSaved {
            request: final_request,
            result: Ok(Revision(9)),
        }),
    );
    assert!(matches!(
        effects.as_slice(),
        [
            Effect::LoadBrowser { .. },
            Effect::LoadLibraryRevision { .. }
        ]
    ));
    assert_eq!(model.route, Route::Browser);
    assert_eq!(model.editor, None);
}

#[test]
fn successful_trash_mutation_should_reload_each_dependent_resource_once() {
    let mut model = AppModel::new(&Config::default());
    let effects = update(
        &mut model,
        AppMsg::Trash(TrashMsg::RestoreNote(NoteId::new())),
    );
    assert!(matches!(effects.as_slice(), [Effect::RestoreNote { .. }]));

    let effects = update(
        &mut model,
        AppMsg::Library(LibraryReply::TrashMutationFinished {
            result: Ok(TrashMutation::NoteRestored),
        }),
    );

    assert_eq!(
        effects,
        vec![
            Effect::LoadSidebar {
                request_id: RequestId(1),
            },
            Effect::LoadBrowser {
                request_id: RequestId(2),
                category_id: None,
                query: String::new(),
            },
            Effect::LoadTrash {
                request_id: RequestId(3),
            },
            Effect::LoadLibraryRevision {
                request_id: RequestId(4),
            },
        ]
    );
}

#[test]
fn failed_trash_mutation_should_not_discard_loaded_resources() {
    let mut model = AppModel::new(&Config::default());

    let effects = update(
        &mut model,
        AppMsg::Library(LibraryReply::TrashMutationFinished {
            result: Err(UiError::new("restore failed")),
        }),
    );

    assert!(effects.is_empty());
    assert_eq!(model.trash.state, LoadState::Idle);
    assert_eq!(model.notice, Some(UiError::new("restore failed")));
}

#[test]
fn duplicate_category_rename_should_start_one_mutation() {
    let mut model = AppModel::new(&Config::default());
    let category_id = CategoryId::new();
    let message = AppMsg::Action(ActionMsg::RenameCategory {
        category_id,
        name: "Renamed".to_owned(),
    });

    let first = update(&mut model, message.clone());
    let second = update(&mut model, message);

    assert_eq!(
        first,
        vec![Effect::RenameCategory {
            category_id,
            name: "Renamed".to_owned(),
        }]
    );
    assert!(second.is_empty());
    assert_eq!(
        model.pending_actions,
        std::collections::BTreeSet::from([ActionKey::RenameCategory(category_id)])
    );
}

#[test]
fn invalid_category_name_should_preserve_loaded_resources_and_surface_an_error() {
    let mut model = AppModel::new(&Config::default());
    let _ = update(&mut model, AppMsg::Sidebar(SidebarMsg::Reload));

    let effects = update(
        &mut model,
        AppMsg::Action(ActionMsg::CreateCategory("  ".to_owned())),
    );

    assert!(effects.is_empty());
    assert_eq!(model.sidebar.state, LoadState::Loading(RequestId(1)));
    assert_eq!(
        model.notice,
        Some(UiError::new("Category names cannot be empty."))
    );
    assert!(model.pending_actions.is_empty());
}

#[test]
fn failed_action_should_not_invalidate_loaded_resources() {
    let mut model = AppModel::new(&Config::default());
    model.sidebar.state = LoadState::Ready(Vec::new());
    let category_id = CategoryId::new();
    let _ = update(
        &mut model,
        AppMsg::Action(ActionMsg::TrashCategory(category_id)),
    );

    let effects = update(
        &mut model,
        AppMsg::Library(LibraryReply::ActionFinished {
            action: ActionKey::TrashCategory(category_id),
            result: Err(UiError::new("trash failed")),
        }),
    );

    assert!(effects.is_empty());
    assert_eq!(model.sidebar.state, LoadState::Ready(Vec::new()));
    assert_eq!(model.notice, Some(UiError::new("trash failed")));
}

#[test]
fn moved_note_should_offer_undo_and_reload_dependent_resources_once() {
    let mut model = AppModel::new(&Config::default());
    let note_id = NoteId::new();
    let source_category_id = CategoryId::new();
    let destination_category_id = CategoryId::new();
    let action = ActionKey::MoveNote {
        note_id,
        source_category_id,
    };

    let effects = update(
        &mut model,
        AppMsg::Action(ActionMsg::MoveNote {
            note_id,
            source_category_id,
            category_id: destination_category_id,
        }),
    );
    assert_eq!(
        effects,
        vec![Effect::MoveNote {
            action,
            note_id,
            category_id: destination_category_id,
        }]
    );

    let effects = update(
        &mut model,
        AppMsg::Library(LibraryReply::ActionFinished {
            action,
            result: Ok(()),
        }),
    );
    assert_eq!(
        model.undo_move,
        Some(super::MoveUndo {
            note_id,
            source_category_id,
        })
    );
    assert!(matches!(
        effects.as_slice(),
        [
            Effect::LoadSidebar { .. },
            Effect::LoadBrowser { .. },
            Effect::LoadTrash { .. },
            Effect::LoadLibraryRevision { .. }
        ]
    ));
}

#[test]
fn creating_a_category_for_a_note_should_move_it_as_one_undoable_action() {
    let mut model = AppModel::new(&Config::default());
    let note_id = NoteId::new();
    let source_category_id = CategoryId::new();
    let action = ActionKey::MoveNote {
        note_id,
        source_category_id,
    };

    let effects = update(
        &mut model,
        AppMsg::Action(ActionMsg::CreateCategoryAndMoveNote {
            name: "  Client work  ".to_owned(),
            note_id,
            source_category_id,
        }),
    );

    assert_eq!(
        effects,
        vec![Effect::CreateCategoryAndMoveNote {
            action,
            name: "Client work".to_owned(),
            note_id,
        }]
    );
    let effects = update(
        &mut model,
        AppMsg::Library(LibraryReply::ActionFinished {
            action,
            result: Ok(()),
        }),
    );
    assert_eq!(
        model.undo_move,
        Some(super::MoveUndo {
            note_id,
            source_category_id,
        })
    );
    assert!(matches!(
        effects.as_slice(),
        [
            Effect::LoadSidebar { .. },
            Effect::LoadBrowser { .. },
            Effect::LoadTrash { .. },
            Effect::LoadLibraryRevision { .. }
        ]
    ));
}

#[test]
fn failed_move_undo_should_preserve_the_retryable_move_state() {
    let mut model = AppModel::new(&Config::default());
    let undo_move = super::MoveUndo {
        note_id: NoteId::new(),
        source_category_id: CategoryId::new(),
    };
    model.undo_move = Some(undo_move);

    let effects = update(&mut model, AppMsg::Action(ActionMsg::UndoMove));
    assert_eq!(
        effects,
        vec![Effect::MoveNote {
            action: ActionKey::UndoMove(undo_move.note_id),
            note_id: undo_move.note_id,
            category_id: undo_move.source_category_id,
        }]
    );
    let effects = update(
        &mut model,
        AppMsg::Library(LibraryReply::ActionFinished {
            action: ActionKey::UndoMove(undo_move.note_id),
            result: Err(UiError::new("undo failed")),
        }),
    );

    assert!(effects.is_empty());
    assert_eq!(model.undo_move, Some(undo_move));
    assert_eq!(model.notice, Some(UiError::new("undo failed")));
}

pub(crate) fn runtime_should_render_and_complete_each_initial_resource()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary_directory = tempfile::tempdir()?;
    let paths = AppPaths {
        config_dir: temporary_directory.path().join("config"),
        data_dir: temporary_directory.path().join("data"),
        cache_dir: temporary_directory.path().join("cache"),
    };
    paths.ensure_exists()?;
    let client = LibraryClient::spawn(SqliteLibrary::open(
        &paths.database_file(),
        &paths.assets_dir(),
    )?)?;
    let stack = gtk::Stack::new();
    for name in ["browser", "editor", "trash"] {
        stack.add_named(&gtk::Box::new(gtk::Orientation::Vertical, 0), Some(name));
    }
    let browser_list = gtk::ListBox::new();
    let browser_pages = gtk::Stack::new();
    let browser_status = libadwaita::StatusPage::new();
    browser_pages.add_named(
        &gtk::Box::new(gtk::Orientation::Vertical, 0),
        Some("contents"),
    );
    browser_pages.add_named(&browser_status, Some("empty"));
    let search_empty = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let empty_new_note = gtk::Button::new();
    let runtime = AppRuntime::new(
        client.clone(),
        AppModel::new(&Config::default()),
        crate::view::ViewRefs::new(stack, browser_status, libadwaita::StatusPage::new())
            .with_browser(
                browser_list,
                browser_pages,
                search_empty,
                empty_new_note,
                libadwaita::WindowTitle::new("Home", "All recent notes"),
            ),
    );

    runtime.dispatch(AppMsg::Navigation(NavigationMsg::Started));
    assert!(crate::tests::support::run_main_context_until(|| {
        matches!(runtime.model().sidebar.state, LoadState::Ready(_))
            && matches!(runtime.model().browser.notes.state, LoadState::Ready(_))
    }));
    assert_eq!(client.categories()?.len(), 1);

    runtime.dispatch(AppMsg::Navigation(NavigationMsg::ImportNote {
        format: DocumentImportFormat::Markdown,
        source: String::from("# Imported\n\n- [x] Converted"),
    }));
    assert!(crate::tests::support::run_main_context_until(|| {
        runtime.model().editor.as_ref().is_some_and(|document| {
            document.source.contains("# Imported") && document.source.contains("- [x] Converted")
        })
    }));
    assert_eq!(client.recent_notes(None, 10, 0)?.len(), 1);

    runtime.dispatch(AppMsg::Browser(BrowserMsg::SearchChanged(
        "needle".to_owned(),
    )));
    assert!(crate::tests::support::run_main_context_until(|| {
        matches!(runtime.model().browser.notes.state, LoadState::Ready(_))
    }));

    runtime.dispatch(AppMsg::Navigation(NavigationMsg::ShowTrash));
    assert!(crate::tests::support::run_main_context_until(|| {
        matches!(runtime.model().trash.state, LoadState::Ready(_))
    }));

    runtime.dispatch(AppMsg::Sidebar(SidebarMsg::Reload));
    let LoadState::Loading(request_id) = runtime.model().sidebar.state else {
        return Err("sidebar should be loading".into());
    };
    runtime.dispatch(AppMsg::Library(LibraryReply::SidebarLoaded {
        request_id,
        result: Err(UiError::new("offline")),
    }));
    assert_eq!(
        runtime.model().sidebar.state,
        LoadState::Failed(UiError::new("offline"))
    );

    runtime_should_write_the_current_carve_snapshot(&runtime, &temporary_directory)?;

    let dispatcher = AppDispatcher::default();
    {
        let detached_stack = gtk::Stack::new();
        detached_stack.add_named(
            &gtk::Box::new(gtk::Orientation::Vertical, 0),
            Some("browser"),
        );
        let detached_runtime = AppRuntime::new(
            client.clone(),
            AppModel::new(&Config::default()),
            crate::view::ViewRefs::new(
                detached_stack,
                libadwaita::StatusPage::new(),
                libadwaita::StatusPage::new(),
            ),
        );
        detached_runtime.bind_dispatcher(&dispatcher);
        assert!(dispatcher.dispatch(AppMsg::Navigation(NavigationMsg::ShowBrowser)));
    }
    assert!(!dispatcher.dispatch(AppMsg::Navigation(NavigationMsg::ShowBrowser)));
    Ok(())
}

pub(crate) fn runtime_should_refresh_visible_resources_after_a_separate_client_mutates_the_library()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary_directory = tempfile::tempdir()?;
    let paths = AppPaths {
        config_dir: temporary_directory.path().join("config"),
        data_dir: temporary_directory.path().join("data"),
        cache_dir: temporary_directory.path().join("cache"),
    };
    paths.ensure_exists()?;
    let client = LibraryClient::spawn(SqliteLibrary::open(
        &paths.database_file(),
        &paths.assets_dir(),
    )?)?;
    let stack = gtk::Stack::new();
    for name in ["browser", "editor", "trash"] {
        stack.add_named(&gtk::Box::new(gtk::Orientation::Vertical, 0), Some(name));
    }
    let browser_status = libadwaita::StatusPage::new();
    let runtime = AppRuntime::new(
        client,
        AppModel::new(&Config::default()),
        crate::view::ViewRefs::new(stack, browser_status, libadwaita::StatusPage::new()),
    );
    let dispatcher = AppDispatcher::default();
    runtime.bind_dispatcher(&dispatcher);
    runtime.monitor_library(&paths.database_file(), dispatcher)?;
    runtime.dispatch(AppMsg::Navigation(NavigationMsg::Started));
    if !crate::tests::support::run_main_context_until(|| {
        matches!(runtime.model().sidebar.state, LoadState::Ready(_))
            && matches!(runtime.model().browser.notes.state, LoadState::Ready(_))
            && runtime.model().library_revision.is_some()
    }) {
        return Err("initial library resources did not load".into());
    }

    let agent_client = carver_sdk::open_local_library(&paths.database_file(), &paths.assets_dir())?;
    let category = agent_client.create_category("From agent")?;
    let note = agent_client.create_note_with_source(category.id, "# Created by agent")?;

    if crate::tests::support::run_main_context_until(|| {
        let model = runtime.model();
        matches!(
            model.sidebar.state,
            LoadState::Ready(ref categories)
                if categories.iter().any(|summary| summary.category.id == category.id)
        ) && matches!(
            model.browser.notes.state,
            LoadState::Ready(ref notes) if notes.iter().any(|summary| summary.id == note.id)
        )
    }) {
        Ok(())
    } else {
        Err("separate client mutation did not refresh visible resources".into())
    }
}

fn runtime_should_write_the_current_carve_snapshot<B: LibraryBackend>(
    runtime: &AppRuntime<B>,
    temporary_directory: &tempfile::TempDir,
) -> Result<(), Box<dyn std::error::Error>> {
    let export_note_id = NoteId::new();
    runtime.dispatch(AppMsg::Editor(EditorMsg::Load {
        note_id: export_note_id,
        revision: Revision(1),
        source: String::from("# Exported draft\n\nUnsaved body"),
    }));
    runtime.dispatch(AppMsg::Editor(EditorMsg::ExportDialogRequested));
    let export_dialog = runtime
        .model()
        .editor_export_dialog_request
        .ok_or("export dialog request")?;
    let export_path = temporary_directory.path().join("exported-draft.crv");
    let export_target = gtk::gio::File::for_path(&export_path).uri().to_string();
    runtime.dispatch(AppMsg::Editor(EditorMsg::ExportRequested {
        request_id: export_dialog.request_id,
        format: EditorExportFormat::Carve,
        include_assets: false,
        target_uri: export_target,
    }));
    assert!(crate::tests::support::run_main_context_until(|| {
        std::fs::read_to_string(&export_path)
            .is_ok_and(|source| source == "# Exported draft\n\nUnsaved body")
    }));
    Ok(())
}
