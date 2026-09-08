use super::*;

fn open_note() -> AppModel {
    let mut model = AppModel::new(&Config::default());
    let _ = update(
        &mut model,
        AppMsg::Editor(EditorMsg::Load {
            note_id: NoteId::new(),
            revision: Revision(1),
            source: "Before".into(),
        }),
    );
    model.library_revision = Some(LibraryRevision(1));
    model
}

fn refresh(model: &mut AppModel) -> Effect {
    let effects = update(model, AppMsg::LibraryChangedExternally);
    let Effect::LoadLibraryRevision { request_id } = effects[0] else {
        panic!("revision read")
    };
    let effects = update(
        model,
        AppMsg::Library(LibraryReply::LibraryRevisionLoaded {
            request_id,
            result: Ok(LibraryRevision(2)),
        }),
    );
    effects
        .into_iter()
        .find(|effect| matches!(effect, Effect::RefreshEditorNote { .. }))
        .unwrap_or_else(|| panic!("refresh"))
}

fn saved_note(note_id: NoteId, revision: Revision) -> Note {
    Note {
        id: note_id,
        category_id: CategoryId::new(),
        source: "From agent".into(),
        title: "From agent".into(),
        plain_text: "From agent".into(),
        revision,
        is_favorite: false,
        created_at: OffsetDateTime::UNIX_EPOCH,
        updated_at: OffsetDateTime::UNIX_EPOCH,
        trashed_at: None,
    }
}

fn complete(model: &mut AppModel, effect: Effect, revision: Revision) -> Vec<Effect> {
    let Effect::RefreshEditorNote { ref snapshot, .. } = effect else {
        panic!("refresh")
    };
    let note = saved_note(snapshot.note_id, revision);
    complete_result(model, effect, Ok(Some(note)))
}

fn complete_result(
    model: &mut AppModel,
    effect: Effect,
    result: Result<Option<Note>, UiError>,
) -> Vec<Effect> {
    let Effect::RefreshEditorNote {
        request_id,
        session,
        snapshot,
        discard_local,
    } = effect
    else {
        panic!("refresh")
    };
    update(
        model,
        AppMsg::Library(LibraryReply::EditorRefreshed {
            request_id,
            session,
            snapshot,
            discard_local,
            result,
        }),
    )
}

#[test]
fn external_edit_should_refresh_clean_document_without_reopening() {
    let mut model = open_note();
    let session = model
        .editor
        .as_ref()
        .unwrap_or_else(|| panic!("missing fixture value"))
        .session;
    let effect = refresh(&mut model);
    let effects = complete(&mut model, effect, Revision(2));
    let document = model
        .editor
        .as_ref()
        .unwrap_or_else(|| panic!("missing fixture value"));
    assert_eq!(
        (
            document.session,
            document.revision,
            document.source.as_str(),
            &document.save_state
        ),
        (session, Revision(2), "From agent", &EditorSaveState::Clean)
    );
    assert_eq!(
        model
            .editor_preview
            .as_ref()
            .unwrap_or_else(|| panic!("missing fixture value"))
            .source,
        "From agent"
    );
    assert_eq!(
        effects,
        vec![Effect::ReloadRichEditor {
            session,
            source: "From agent".into()
        }]
    );
}

#[test]
fn external_edit_should_preserve_typing_started_during_refresh() {
    let mut model = open_note();
    let effect = refresh(&mut model);
    let _ = update(
        &mut model,
        AppMsg::Editor(EditorMsg::SourceChanged("Local draft".into())),
    );
    let effects = complete(&mut model, effect, Revision(2));
    let document = model
        .editor
        .as_mut()
        .unwrap_or_else(|| panic!("missing fixture value"));
    assert_eq!(document.source, "Local draft");
    assert_eq!(document.revision, Revision(1));
    assert!(document.external_change.is_some());
    assert!(document.begin_save().is_none());
    assert!(matches!(
        effects.as_slice(),
        [Effect::ShowExternalEdit { .. }]
    ));
}

#[test]
fn confirmed_reload_should_replace_conflicting_draft() {
    let mut model = open_note();
    let _ = update(
        &mut model,
        AppMsg::Editor(EditorMsg::SourceChanged("Local draft".into())),
    );
    let effect = refresh(&mut model);
    let _ = complete(&mut model, effect, Revision(2));
    let session = model
        .editor
        .as_ref()
        .unwrap_or_else(|| panic!("missing fixture value"))
        .session;
    let effects = update(
        &mut model,
        AppMsg::Editor(EditorMsg::ReloadExternal { session }),
    );
    let _ = complete(&mut model, effects[0].clone(), Revision(2));
    let document = model
        .editor
        .as_ref()
        .unwrap_or_else(|| panic!("missing fixture value"));
    assert_eq!(document.source, "From agent");
    assert!(document.external_change.is_none());
    assert_eq!(document.save_state, EditorSaveState::Clean);
}

#[test]
fn stale_refresh_should_not_replace_another_editor_session() {
    let mut model = open_note();
    let effect = refresh(&mut model);
    let _ = update(
        &mut model,
        AppMsg::Editor(EditorMsg::Load {
            note_id: NoteId::new(),
            revision: Revision(1),
            source: "Other note".into(),
        }),
    );
    assert!(complete(&mut model, effect, Revision(2)).is_empty());
    assert_eq!(
        model
            .editor
            .as_ref()
            .unwrap_or_else(|| panic!("missing fixture value"))
            .source,
        "Other note"
    );
}

#[test]
fn unchanged_note_revision_should_preserve_editor_projection() {
    let mut model = open_note();
    let effect = refresh(&mut model);
    assert!(complete(&mut model, effect, Revision(1)).is_empty());
    assert_eq!(
        model
            .editor
            .as_ref()
            .unwrap_or_else(|| panic!("missing fixture value"))
            .source,
        "Before"
    );
}

#[test]
fn wakeup_during_revision_read_should_schedule_a_follow_up_check() {
    let mut model = open_note();
    let effects = update(&mut model, AppMsg::LibraryChangedExternally);
    let Effect::LoadLibraryRevision { request_id } = effects[0] else {
        panic!("revision")
    };
    assert!(update(&mut model, AppMsg::LibraryChangedExternally).is_empty());
    let effects = update(
        &mut model,
        AppMsg::Library(LibraryReply::LibraryRevisionLoaded {
            request_id,
            result: Ok(LibraryRevision(1)),
        }),
    );
    assert!(matches!(
        effects.as_slice(),
        [Effect::LoadLibraryRevision { .. }]
    ));
}

#[test]
fn refresh_completing_during_save_should_wait_for_save_completion() {
    let mut model = open_note();
    let effect = refresh(&mut model);
    let document = model
        .editor
        .as_mut()
        .unwrap_or_else(|| panic!("missing fixture value"));
    document.source_changed("Saving locally".into());
    let request = document
        .begin_save()
        .unwrap_or_else(|| panic!("missing fixture value"));
    assert!(complete(&mut model, effect, Revision(2)).is_empty());
    assert!(
        model
            .editor
            .as_ref()
            .unwrap_or_else(|| panic!("missing fixture value"))
            .external_change
            .is_none()
    );
    let effects = update(
        &mut model,
        AppMsg::Library(LibraryReply::EditorSaved {
            request,
            result: Ok(Revision(2)),
        }),
    );
    assert!(
        effects
            .iter()
            .any(|effect| matches!(effect, Effect::RefreshEditorNote { .. }))
    );
}

#[test]
fn refresh_should_recheck_when_local_revision_changed_during_read() {
    let mut model = open_note();
    let effect = refresh(&mut model);
    model
        .editor
        .as_mut()
        .unwrap_or_else(|| panic!("editor"))
        .revision = Revision(2);
    let effects = complete(&mut model, effect, Revision(3));
    assert!(matches!(
        effects.as_slice(),
        [Effect::RefreshEditorNote { .. }]
    ));
    assert_eq!(
        model
            .editor
            .as_ref()
            .unwrap_or_else(|| panic!("editor"))
            .source,
        "Before"
    );
    let _ = complete(&mut model, effects[0].clone(), Revision(3));
    assert_eq!(
        model
            .editor
            .as_ref()
            .unwrap_or_else(|| panic!("editor"))
            .source,
        "From agent"
    );
}

#[test]
fn confirmed_reload_should_preserve_typing_after_confirmation() {
    let mut model = open_note();
    let session = model
        .editor
        .as_ref()
        .unwrap_or_else(|| panic!("editor"))
        .session;
    let effects = update(
        &mut model,
        AppMsg::Editor(EditorMsg::ReloadExternal { session }),
    );
    let _ = update(
        &mut model,
        AppMsg::Editor(EditorMsg::SourceChanged("New local work".into())),
    );
    let effects = complete(&mut model, effects[0].clone(), Revision(2));
    assert_eq!(
        model
            .editor
            .as_ref()
            .unwrap_or_else(|| panic!("editor"))
            .source,
        "New local work"
    );
    assert!(matches!(
        effects.as_slice(),
        [Effect::ShowExternalEdit { .. }]
    ));
}

#[test]
fn back_should_offer_reload_when_external_conflict_prevents_saving() {
    let mut model = open_note();
    let _ = update(
        &mut model,
        AppMsg::Editor(EditorMsg::SourceChanged("Local draft".into())),
    );
    let effect = refresh(&mut model);
    let _ = complete(&mut model, effect, Revision(2));
    let effects = update(&mut model, AppMsg::Editor(EditorMsg::BackRequested));
    assert!(matches!(
        effects.as_slice(),
        [Effect::ShowExternalEdit { .. }]
    ));
    assert_eq!(
        model
            .editor
            .as_ref()
            .unwrap_or_else(|| panic!("editor"))
            .source,
        "Local draft"
    );
}

#[test]
fn confirmed_reload_should_cancel_a_previously_requested_close() {
    let mut model = open_note();
    let document = model.editor.as_mut().unwrap_or_else(|| panic!("editor"));
    let session = document.session;
    document.request_close();
    let effects = update(
        &mut model,
        AppMsg::Editor(EditorMsg::ReloadExternal { session }),
    );
    let _ = complete(&mut model, effects[0].clone(), Revision(2));
    assert!(
        !model
            .editor
            .as_ref()
            .unwrap_or_else(|| panic!("editor"))
            .close_is_requested()
    );
}

#[test]
fn externally_trashed_note_should_preserve_draft_and_block_autosaves() {
    let mut model = open_note();
    let document = model.editor.as_mut().unwrap_or_else(|| panic!("editor"));
    let mut note = saved_note(document.note_id, Revision(2));
    note.trashed_at = Some(OffsetDateTime::UNIX_EPOCH);
    document.source_changed("Local draft".into());
    let effect = refresh(&mut model);
    let effects = complete_result(&mut model, effect, Ok(Some(note)));
    assert!(matches!(
        effects.as_slice(),
        [Effect::ShowExternalEdit { deleted: true, .. }]
    ));
    let document = model.editor.as_mut().unwrap_or_else(|| panic!("editor"));
    assert_eq!(document.source, "Local draft");
    assert!(document.begin_save().is_none());
    let effects = update(&mut model, AppMsg::Editor(EditorMsg::BackRequested));
    assert!(matches!(
        effects.as_slice(),
        [Effect::ShowExternalEdit { deleted: true, .. }]
    ));
}

#[test]
fn missing_note_should_offer_confirmed_discard_to_reach_trash() {
    let mut model = open_note();
    let effect = refresh(&mut model);
    let _ = complete_result(&mut model, effect, Ok(None));
    let session = model
        .editor
        .as_ref()
        .unwrap_or_else(|| panic!("editor"))
        .session;
    let _ = update(
        &mut model,
        AppMsg::Editor(EditorMsg::KeepExternalDraft { session }),
    );
    assert!(model.editor.is_some());
    model.trash = super::super::Resource::default();
    let effects = update(
        &mut model,
        AppMsg::Editor(EditorMsg::CloseDeleted { session }),
    );
    assert!(model.editor.is_none());
    assert_eq!(model.route, Route::Trash);
    assert!(matches!(effects.as_slice(), [Effect::LoadTrash { .. }]));
}

#[test]
fn deletion_confirmation_should_not_close_a_new_editor_session() {
    let mut model = open_note();
    let session = model
        .editor
        .as_ref()
        .unwrap_or_else(|| panic!("editor"))
        .session;
    let _ = update(
        &mut model,
        AppMsg::Editor(EditorMsg::Load {
            note_id: NoteId::new(),
            revision: Revision(1),
            source: "New note".into(),
        }),
    );
    assert!(
        update(
            &mut model,
            AppMsg::Editor(EditorMsg::CloseDeleted { session })
        )
        .is_empty()
    );
    assert_eq!(
        model
            .editor
            .as_ref()
            .unwrap_or_else(|| panic!("editor"))
            .source,
        "New note"
    );
}

#[test]
fn refresh_read_failure_should_preserve_draft_without_claiming_deletion() {
    let mut model = open_note();
    let effect = refresh(&mut model);
    let _ = complete_result(&mut model, effect, Err(UiError::new("Read failed")));
    let document = model.editor.as_ref().unwrap_or_else(|| panic!("editor"));
    assert_eq!(document.source, "Before");
    assert!(document.external_change.is_none());
    assert_eq!(model.notice, Some(UiError::new("Read failed")));
}

#[test]
fn first_external_revision_should_refresh_after_initial_revision_read_failed() {
    let mut model = open_note();
    model.library_revision = None;
    let effects = update(&mut model, AppMsg::LibraryChangedExternally);
    let Effect::LoadLibraryRevision { request_id } = effects[0] else {
        panic!("revision")
    };
    let _ = update(
        &mut model,
        AppMsg::Library(LibraryReply::LibraryRevisionLoaded {
            request_id,
            result: Err(UiError::new("Read failed")),
        }),
    );
    let effect = refresh(&mut model);
    let _ = complete(&mut model, effect, Revision(2));
    assert_eq!(
        model
            .editor
            .as_ref()
            .unwrap_or_else(|| panic!("editor"))
            .source,
        "From agent"
    );
}

#[test]
fn startup_revision_should_refresh_when_external_wakeup_arrives_during_initial_read() {
    use super::super::model::{LibraryRevisionCheckReason, LibraryRevisionRequest};
    let mut model = open_note();
    model.library_revision = None;
    model.library_revision_request = Some(LibraryRevisionRequest {
        request_id: RequestId(100),
        reason: LibraryRevisionCheckReason::InitialLoad,
    });
    assert!(update(&mut model, AppMsg::LibraryChangedExternally).is_empty());
    let effects = update(
        &mut model,
        AppMsg::Library(LibraryReply::LibraryRevisionLoaded {
            request_id: RequestId(100),
            result: Ok(LibraryRevision(2)),
        }),
    );
    let effect = effects
        .iter()
        .find(|effect| matches!(effect, Effect::RefreshEditorNote { .. }))
        .unwrap_or_else(|| panic!("refresh"))
        .clone();
    let _ = complete(&mut model, effect, Revision(2));
    assert_eq!(
        model
            .editor
            .as_ref()
            .unwrap_or_else(|| panic!("editor"))
            .source,
        "From agent"
    );
}

#[test]
fn trash_requested_should_preserve_a_draft_when_note_was_deleted_externally() {
    let mut model = open_note();
    model
        .editor
        .as_mut()
        .unwrap_or_else(|| panic!("editor"))
        .source_changed("Local draft".into());
    let effect = refresh(&mut model);
    let _ = complete_result(&mut model, effect, Ok(None));
    let effects = update(&mut model, AppMsg::Editor(EditorMsg::TrashRequested));
    assert!(
        model.editor.is_some(),
        "TrashRequested discarded the conflicted draft"
    );
    assert!(matches!(
        effects.as_slice(),
        [Effect::ShowExternalEdit { deleted: true, .. }]
    ));
    assert_eq!(
        model
            .editor
            .as_ref()
            .unwrap_or_else(|| panic!("editor"))
            .source,
        "Local draft"
    );
}

#[test]
fn failed_refresh_should_retry_on_next_wakeup_with_unchanged_library_revision() {
    let mut model = open_note();
    let effect = refresh(&mut model);
    assert!(
        complete_result(
            &mut model,
            effect,
            Err(UiError::new("Temporary read failure"))
        )
        .is_empty()
    );
    let effects = update(&mut model, AppMsg::LibraryChangedExternally);
    let Effect::LoadLibraryRevision { request_id } = effects[0] else {
        panic!("revision")
    };
    let effects = update(
        &mut model,
        AppMsg::Library(LibraryReply::LibraryRevisionLoaded {
            request_id,
            result: Ok(LibraryRevision(2)),
        }),
    );
    let effect = effects
        .into_iter()
        .find(|effect| matches!(effect, Effect::RefreshEditorNote { .. }))
        .unwrap_or_else(|| panic!("unchanged revision suppressed the failed editor read"));
    let _ = complete(&mut model, effect, Revision(2));
    assert_eq!(
        model
            .editor
            .as_ref()
            .unwrap_or_else(|| panic!("editor"))
            .source,
        "From agent"
    );
}
