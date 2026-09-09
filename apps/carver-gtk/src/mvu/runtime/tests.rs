use super::*;
use crate::mvu::{EditorMsg, EditorSessionId};
use crate::ui::tests::support::{TestResult, run_main_context_until, test_state};

use std::io::Read;

struct Fixture {
    directory: tempfile::TempDir,
    runtime: AppRuntime<carver_storage_sqlite::SqliteLibrary>,
    note: carver_sdk::Note,
}

fn fixture(source: &str) -> Result<Fixture, Box<dyn std::error::Error>> {
    let (directory, client) = test_state()?;
    let category = client.create_category("Exports")?;
    let created = client.create_note(category.id)?;
    let note = client.save_note(created.id, created.revision, source)?;
    let stack = gtk::Stack::new();
    stack.add_named(
        &gtk::Box::new(gtk::Orientation::Vertical, 0),
        Some("editor"),
    );
    let runtime = AppRuntime::new(
        client,
        AppModel::new(&carver_config::Config::default()),
        ViewRefs::new(
            stack,
            libadwaita::StatusPage::new(),
            libadwaita::StatusPage::new(),
        ),
    );
    runtime.dispatch(AppMsg::Editor(EditorMsg::Load {
        note_id: note.id,
        revision: note.revision,
        source: source.into(),
    }));
    Ok(Fixture {
        directory,
        runtime,
        note,
    })
}

fn request_export(fixture: &Fixture, path: &Path, include_assets: bool) -> u64 {
    fixture
        .runtime
        .dispatch(AppMsg::Editor(EditorMsg::ExportDialogRequested));
    let request = fixture
        .runtime
        .model()
        .editor_export_dialog_request
        .unwrap_or_else(|| panic!("export dialog"));
    fixture
        .runtime
        .dispatch(AppMsg::Editor(EditorMsg::ExportRequested {
            request_id: request.request_id,
            format: EditorExportFormat::Carve,
            include_assets,
            target_uri: gio::File::for_path(path).uri().into(),
        }));
    request.request_id
}

fn portable_export_should_include_only_referenced_managed_assets() -> TestResult {
    let fixture = fixture("# Export")?;
    let client = &fixture.runtime.inner.client;
    let path = client.store_asset(fixture.note.id, "png", b"image bytes")?;
    let _unused = client.store_asset(fixture.note.id, "png", b"unused")?;
    let source = format!("# Export\n\n![Image]({path})");
    fixture
        .runtime
        .dispatch(AppMsg::Editor(EditorMsg::SourceChanged(source.clone())));
    let output = fixture.directory.path().join("export.zip");
    request_export(&fixture, &output, true);
    assert!(run_main_context_until(|| fixture
        .runtime
        .model()
        .editor_export_progress
        .is_none()));
    assert_eq!(
        fixture.runtime.model().notice,
        Some(UiError::new("Note exported"))
    );
    let mut zip = zip::ZipArchive::new(File::open(output)?)?;
    assert_eq!(zip.len(), 2);
    let mut bytes = Vec::new();
    zip.by_name(&path)?.read_to_end(&mut bytes)?;
    assert_eq!(bytes, b"image bytes");
    let mut document = String::new();
    zip.by_name("Export.crv")?.read_to_string(&mut document)?;
    assert_eq!(document, source);
    let persisted = client.note(fixture.note.id)?.ok_or("note")?;
    assert_eq!(
        (persisted.source, persisted.revision, persisted.updated_at),
        (
            fixture.note.source,
            fixture.note.revision,
            fixture.note.updated_at
        )
    );
    assert!(fixture.runtime.inner.prepared_exports.borrow().is_empty());
    Ok(())
}

fn cancelled_export_should_remove_staging_without_writing_the_destination() -> TestResult {
    let fixture = fixture("# Missing\n\n![Missing](assets/missing.png)")?;
    let output = fixture.directory.path().join("cancelled.zip");
    let request_id = request_export(&fixture, &output, true);
    assert!(run_main_context_until(|| fixture
        .runtime
        .model()
        .editor_export_warning_request
        .is_some()));
    let staging = {
        let exports = fixture.runtime.inner.prepared_exports.borrow();
        let prepared = exports.get(&request_id).ok_or("prepared export")?;
        let PreparedExportArtifact::File { path, .. } = &prepared.artifact else {
            return Err("file artifact".into());
        };
        path.clone()
    };
    fixture
        .runtime
        .dispatch(AppMsg::Editor(EditorMsg::ExportCancelled { request_id }));
    assert!(!staging.exists());
    assert!(!output.exists());
    assert!(fixture.runtime.model().editor_export_progress.is_none());
    Ok(())
}

fn failed_export_write_should_report_error_and_preserve_the_note() -> TestResult {
    let fixture = fixture("# Unchanged")?;
    let output = fixture.directory.path().join("absent/output.crv");
    request_export(&fixture, &output, false);
    assert!(run_main_context_until(|| fixture
        .runtime
        .model()
        .editor_export_progress
        .is_none()));
    assert!(fixture.runtime.model().notice.is_some());
    assert!(!output.exists());
    assert!(fixture.runtime.inner.prepared_exports.borrow().is_empty());
    let note = fixture
        .runtime
        .inner
        .client
        .note(fixture.note.id)?
        .ok_or("note")?;
    assert_eq!(
        (note.source, note.revision, note.updated_at),
        (
            fixture.note.source,
            fixture.note.revision,
            fixture.note.updated_at
        )
    );
    Ok(())
}

fn stale_dialog_response_should_leave_the_newer_export_usable() -> TestResult {
    let fixture = fixture("# Current")?;
    fixture
        .runtime
        .dispatch(AppMsg::Editor(EditorMsg::ExportDialogRequested));
    let old = fixture
        .runtime
        .model()
        .editor_export_dialog_request
        .ok_or("old request")?;
    fixture
        .runtime
        .dispatch(AppMsg::Editor(EditorMsg::ExportDialogRequested));
    let current = fixture
        .runtime
        .model()
        .editor_export_dialog_request
        .ok_or("current request")?;
    let output = fixture.directory.path().join("current.crv");
    for request_id in [old.request_id, current.request_id] {
        fixture
            .runtime
            .dispatch(AppMsg::Editor(EditorMsg::ExportRequested {
                request_id,
                format: EditorExportFormat::Carve,
                include_assets: false,
                target_uri: gio::File::for_path(&output).uri().into(),
            }));
    }
    assert!(run_main_context_until(
        || output.exists() && fixture.runtime.model().editor_export_progress.is_none()
    ));
    assert_eq!(std::fs::read_to_string(output)?, "# Current");
    Ok(())
}

fn missing_prepared_export_should_report_a_recoverable_error() -> TestResult {
    let fixture = fixture("# Missing artifact")?;
    fixture
        .runtime
        .inner
        .model
        .borrow_mut()
        .editor_export_progress = Some(crate::mvu::EditorExportProgress {
        request_id: 42,
        session: EditorSessionId(1),
    });
    fixture.runtime.write_editor_export(42);
    assert_eq!(
        fixture.runtime.model().notice,
        Some(UiError::new("The prepared export is no longer available."))
    );
    assert!(fixture.runtime.model().editor_export_progress.is_none());
    Ok(())
}

pub(crate) fn export_runtime_should_cover_completion_cancellation_and_failures() -> TestResult {
    category_move_should_preserve_source_and_offer_undo()?;
    empty_trash_should_remove_trashed_notes_but_keep_active_notes()?;
    portable_export_should_include_only_referenced_managed_assets()?;
    cancelled_export_should_remove_staging_without_writing_the_destination()?;
    failed_export_write_should_report_error_and_preserve_the_note()?;
    stale_dialog_response_should_leave_the_newer_export_usable()?;
    missing_prepared_export_should_report_a_recoverable_error()?;
    Ok(())
}

fn category_move_should_preserve_source_and_offer_undo() -> TestResult {
    let fixture = fixture("# Move me")?;
    fixture.runtime.dispatch(AppMsg::Action(
        crate::mvu::ActionMsg::CreateCategoryAndMoveNote {
            name: "Destination".into(),
            note_id: fixture.note.id,
            source_category_id: fixture.note.category_id,
        },
    ));
    assert!(run_main_context_until(|| fixture
        .runtime
        .inner
        .client
        .note(fixture.note.id)
        .is_ok_and(|note| note.is_some_and(
            |note| note.category_id != fixture.note.category_id
        ))));
    let note = fixture
        .runtime
        .inner
        .client
        .note(fixture.note.id)?
        .ok_or("moved note")?;
    assert_eq!(note.source, fixture.note.source);
    assert!(run_main_context_until(|| fixture
        .runtime
        .model()
        .undo_move
        .is_some()));
    Ok(())
}

fn empty_trash_should_remove_trashed_notes_but_keep_active_notes() -> TestResult {
    let fixture = fixture("# Keep")?;
    let trashed = fixture
        .runtime
        .inner
        .client
        .create_note(fixture.note.category_id)?;
    fixture.runtime.inner.client.trash_note(trashed.id)?;
    fixture
        .runtime
        .dispatch(AppMsg::Trash(crate::mvu::TrashMsg::Empty));
    assert!(run_main_context_until(|| fixture
        .runtime
        .inner
        .client
        .note(trashed.id)
        .is_ok_and(|note| note.is_none())));
    assert_eq!(
        fixture
            .runtime
            .inner
            .client
            .note(fixture.note.id)?
            .ok_or("active note")?
            .source,
        fixture.note.source
    );
    Ok(())
}
