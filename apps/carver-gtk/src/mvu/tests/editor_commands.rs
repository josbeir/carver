use super::*;

#[test]
fn source_format_command_should_update_only_the_reducer_model() {
    let mut model = AppModel::new(&Config::default());
    let _ = update(
        &mut model,
        AppMsg::Editor(EditorMsg::Load {
            note_id: NoteId::new(),
            revision: Revision(1),
            source: String::from("Carver"),
        }),
    );

    let effects = update(
        &mut model,
        AppMsg::Editor(EditorMsg::ApplySourceCommand {
            command: SourceCommand::ToggleInline {
                opening: String::from("*"),
                closing: String::from("*"),
            },
            selection: 0..6,
        }),
    );

    assert_eq!(
        model
            .editor
            .as_ref()
            .map(|document| document.source.as_str()),
        Some("*Carver*")
    );
    assert!(matches!(
        effects.as_slice(),
        [
            Effect::SchedulePreview { .. },
            Effect::ScheduleEditorSave { .. },
            Effect::SelectEditorSource { .. }
        ]
    ));
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

    let effects = update(&mut model, AppMsg::Editor(EditorMsg::CopyRequested));
    assert!(matches!(
        effects.as_slice(),
        [Effect::CopyEditorDocument { request }] if request.source == "Unsaved source"
    ));
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
fn rich_selection_copy_should_publish_the_fragment_without_a_success_notice() {
    let mut model = AppModel::new(&Config::default());
    let _ = update(
        &mut model,
        AppMsg::Editor(EditorMsg::Load {
            note_id: NoteId::new(),
            revision: Revision(1),
            source: String::from("Complete note"),
        }),
    );
    let session = model.editor.as_ref().map(|document| document.session);
    let effects = update(
        &mut model,
        AppMsg::Editor(EditorMsg::CopySelectionRequested {
            session: session.unwrap_or(EditorSessionId(0)),
            source: String::from("Selected *text*"),
        }),
    );

    assert!(matches!(
        effects.as_slice(),
        [Effect::CopyEditorDocument { request }]
            if request.source == "Selected *text*"
                && request.scope == EditorCopyScope::Selection
    ));
    let request_id = model
        .editor_copy_request
        .as_ref()
        .map(|request| request.request_id)
        .unwrap_or_default();
    let _ = update(
        &mut model,
        AppMsg::Editor(EditorMsg::CopyCompleted {
            request_id,
            omitted_images: 0,
        }),
    );
    assert_eq!(model.notice, None);
}

#[test]
fn stale_rich_selection_copy_should_be_ignored() {
    let mut model = AppModel::new(&Config::default());
    let _ = update(
        &mut model,
        AppMsg::Editor(EditorMsg::Load {
            note_id: NoteId::new(),
            revision: Revision(1),
            source: String::from("Complete note"),
        }),
    );

    let effects = update(
        &mut model,
        AppMsg::Editor(EditorMsg::CopySelectionRequested {
            session: EditorSessionId(u64::MAX),
            source: String::from("Stale selection"),
        }),
    );

    assert!(effects.is_empty());
    assert_eq!(model.editor_copy_request, None);
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
            format: EditorExportFormat::Html,
            include_assets: false,
            target_uri: String::from("file:///tmp/draft.html"),
        }),
    );

    assert!(matches!(
        effects.as_slice(),
        [Effect::PrepareEditorExport {
            source,
            note_id: effect_note_id,
            format: EditorExportFormat::Html,
            ..
        }] if source == "# Unsaved draft" && effect_note_id == &note_id
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

    assert!(matches!(
        effects.as_slice(),
        [Effect::ShowEditorExportWarning { request }] if request.request_id == dialog.request_id
    ));
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
