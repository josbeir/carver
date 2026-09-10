use super::*;

fn editor() -> AppModel {
    let mut model = AppModel::new(&Config::default());
    let _ = update(
        &mut model,
        AppMsg::Editor(EditorMsg::Load {
            note_id: NoteId::new(),
            revision: Revision(1),
            source: "# Draft".into(),
        }),
    );
    model
}

fn export_dialog(model: &mut AppModel) -> super::super::EditorExportDialogRequest {
    let _ = update(model, AppMsg::Editor(EditorMsg::ExportDialogRequested));
    model
        .editor_export_dialog_request
        .clone()
        .unwrap_or_else(|| panic!("fixture request should exist"))
}

#[test]
fn export_should_keep_the_profile_captured_before_a_setting_change() {
    for format in [EditorExportFormat::Html, EditorExportFormat::Pdf] {
        let mut model = editor();
        let request = export_dialog(&mut model);
        let _ = update(
            &mut model,
            AppMsg::Preferences(PreferencesMsg::SetEnhancedCarveRendering(false)),
        );
        let effects = update(
            &mut model,
            AppMsg::Editor(EditorMsg::ExportRequested {
                request_id: request.request_id,
                format,
                include_assets: false,
                target_uri: "file:///tmp/export".into(),
            }),
        );
        match &effects[0] {
            Effect::PrepareEditorExport { html_profile, .. } => assert_eq!(
                *html_profile,
                carver_domain::rendering::HtmlProfile::Enhanced
            ),
            Effect::ExportEditorPdf { request } => assert_eq!(
                request.html_profile,
                carver_domain::rendering::HtmlProfile::Enhanced
            ),
            effect => panic!("unexpected effect: {effect:?}"),
        }
    }
}

#[test]
fn stale_export_dialog_response_should_preserve_the_newer_snapshot() {
    let mut model = editor();
    let old = export_dialog(&mut model);
    let current = export_dialog(&mut model);
    let effects = update(
        &mut model,
        AppMsg::Editor(EditorMsg::ExportRequested {
            request_id: old.request_id,
            format: EditorExportFormat::Carve,
            include_assets: false,
            target_uri: "file:///unused.crv".into(),
        }),
    );
    assert!(effects.is_empty());
    assert_eq!(model.editor_export_dialog_request, Some(current));
}

#[test]
fn confirmed_warning_should_write_only_the_matching_export() {
    let mut model = editor();
    let dialog = export_dialog(&mut model);
    let _ = update(
        &mut model,
        AppMsg::Editor(EditorMsg::ExportRequested {
            request_id: dialog.request_id,
            format: EditorExportFormat::Markdown,
            include_assets: false,
            target_uri: "file:///unused.md".into(),
        }),
    );
    let _ = update(
        &mut model,
        AppMsg::Library(LibraryReply::EditorExportPrepared {
            request_id: dialog.request_id,
            session: dialog.session,
            result: Ok(vec!["Omission".into()]),
        }),
    );
    assert!(
        update(
            &mut model,
            AppMsg::Editor(EditorMsg::ExportConfirmed {
                request_id: dialog.request_id + 1
            })
        )
        .is_empty()
    );
    assert!(model.editor_export_warning_request.is_some());
    assert_eq!(
        update(
            &mut model,
            AppMsg::Editor(EditorMsg::ExportConfirmed {
                request_id: dialog.request_id
            })
        ),
        vec![Effect::WriteEditorExport {
            request_id: dialog.request_id
        }]
    );
    assert!(model.editor_export_warning_request.is_none());
}

#[test]
fn pdf_completion_should_clear_only_the_matching_request() {
    let mut model = editor();
    let dialog = export_dialog(&mut model);
    let effects = update(
        &mut model,
        AppMsg::Editor(EditorMsg::ExportRequested {
            request_id: dialog.request_id,
            format: EditorExportFormat::Pdf,
            include_assets: false,
            target_uri: "file:///unused.pdf".into(),
        }),
    );
    assert!(
        matches!(effects.as_slice(), [Effect::ExportEditorPdf { request }] if request.source == "# Draft")
    );
    let _ = update(
        &mut model,
        AppMsg::Editor(EditorMsg::PdfExportCompleted {
            request_id: dialog.request_id + 1,
        }),
    );
    assert!(model.editor_pdf_export_request.is_some());
    let _ = update(
        &mut model,
        AppMsg::Editor(EditorMsg::PdfExportCompleted {
            request_id: dialog.request_id,
        }),
    );
    assert!(model.editor_pdf_export_request.is_none());
    assert_eq!(model.notice, Some(UiError::new("Note exported as PDF")));
}

#[test]
fn pdf_failure_should_preserve_source_and_report_the_matching_error() {
    let mut model = editor();
    let _ = update(&mut model, AppMsg::Editor(EditorMsg::PrintRequested));
    let request = model
        .editor_pdf_export_request
        .clone()
        .unwrap_or_else(|| panic!("fixture request should exist"));
    let _ = update(
        &mut model,
        AppMsg::Editor(EditorMsg::PdfExportFailed {
            request_id: request.request_id + 1,
        }),
    );
    assert!(model.editor_pdf_export_request.is_some());
    let _ = update(
        &mut model,
        AppMsg::Editor(EditorMsg::PdfExportFailed {
            request_id: request.request_id,
        }),
    );
    assert!(model.editor_pdf_export_request.is_none());
    assert_eq!(
        model.notice,
        Some(UiError::new("Could not export the note as PDF."))
    );
    assert_eq!(
        model
            .editor
            .unwrap_or_else(|| panic!("fixture request should exist"))
            .source,
        "# Draft"
    );
}

#[test]
fn copy_failure_should_reject_stale_ids_and_report_note_failure() {
    let mut model = editor();
    let _ = update(&mut model, AppMsg::Editor(EditorMsg::CopyRequested));
    let request = model
        .editor_copy_request
        .clone()
        .unwrap_or_else(|| panic!("fixture request should exist"));
    let _ = update(
        &mut model,
        AppMsg::Editor(EditorMsg::CopyFailed {
            request_id: request.request_id + 1,
        }),
    );
    assert!(model.editor_copy_request.is_some());
    let _ = update(
        &mut model,
        AppMsg::Editor(EditorMsg::CopyFailed {
            request_id: request.request_id,
        }),
    );
    assert!(model.editor_copy_request.is_none());
    assert_eq!(model.notice, Some(UiError::new("Could not copy the note.")));
}

#[test]
fn selection_copy_failure_should_identify_the_failed_operation() {
    let mut model = editor();
    let session = model
        .editor
        .as_ref()
        .unwrap_or_else(|| panic!("fixture request should exist"))
        .session;
    let _ = update(
        &mut model,
        AppMsg::Editor(EditorMsg::CopySelectionRequested {
            session,
            source: "fragment".into(),
        }),
    );
    let request = model
        .editor_copy_request
        .clone()
        .unwrap_or_else(|| panic!("fixture request should exist"));
    let _ = update(
        &mut model,
        AppMsg::Editor(EditorMsg::CopyFailed {
            request_id: request.request_id,
        }),
    );
    assert_eq!(
        model.notice,
        Some(UiError::new("Could not copy the selection."))
    );
    assert_eq!(
        model
            .editor
            .unwrap_or_else(|| panic!("fixture request should exist"))
            .source,
        "# Draft"
    );
}
