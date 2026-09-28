use super::*;

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
            move_error: None,
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
                move_to: None,
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
            move_error: None,
        }),
    );

    // The first note's save completes against its own background tab. It may refresh the
    // active note's links (a new backlink), but must not replace the newer document.
    assert!(
        effects.iter().all(|effect| matches!(
            effect,
            Effect::LoadLibraryRevision { .. } | Effect::LoadNoteLinks { .. }
        )),
        "a stale save should only refresh library metadata and note links"
    );
    let first = model
        .background_documents
        .values()
        .find(|document| document.note_id == first_note_id);
    assert_eq!(first.map(|document| document.revision), Some(Revision(2)));
    assert!(first.is_some_and(|document| document.save_state == super::EditorSaveState::Clean));
    // The active second document is untouched.
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
            move_error: None,
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

    let effects = update(&mut model, AppMsg::Editor(EditorMsg::BackRequested));
    assert!(
        effects
            .iter()
            .any(|effect| matches!(effect, Effect::LoadBrowser { .. }))
    );
    assert_eq!(model.route, Route::Browser);
    assert!(model.editor.is_none());

    // The in-flight save completes against the note's background tab.
    let effects = update(
        &mut model,
        AppMsg::Library(LibraryReply::EditorSaved {
            request: first_request,
            result: Ok(Revision(8)),
            move_error: None,
        }),
    );
    assert!(
        effects
            .iter()
            .all(|effect| matches!(effect, Effect::LoadLibraryRevision { .. }))
    );
    let background = model
        .background_documents
        .values()
        .find(|document| document.note_id == note_id);
    assert_eq!(
        background.map(|document| document.revision),
        Some(Revision(8))
    );
    assert!(
        background.is_some_and(|document| document.save_state == super::EditorSaveState::Clean)
    );
}

#[test]
fn a_superseded_save_error_should_keep_the_move_and_restart_the_save() {
    let mut model = AppModel::new(&Config::default());
    let note_id = NoteId::new();
    let source = "---\nstatus: ready\n---\n\nInitial";
    let _ = update(
        &mut model,
        AppMsg::Editor(EditorMsg::Load {
            note_id,
            revision: Revision(1),
            source: source.to_owned(),
        }),
    );
    let Some(session) = model.editor.as_ref().map(|document| document.session) else {
        panic!("editor session");
    };
    let category = carver_sdk::CategoryId::new();
    let Some(parsed) = carver_domain::parse_frontmatter_document(source) else {
        panic!("frontmatter");
    };
    let _ = update(
        &mut model,
        AppMsg::Editor(EditorMsg::ApplyFrontmatter {
            session,
            edit: FrontmatterEdit::Parsed(parsed),
            category: Some(category),
        }),
    );

    // Start the save that carries the move, then type before it finishes.
    let effects = update(&mut model, AppMsg::Editor(EditorMsg::RetrySave));
    let request = match effects.as_slice() {
        [Effect::SaveNote { request }] => request.clone(),
        _ => panic!("the retry should start one save"),
    };
    assert_eq!(request.move_to, Some(category));
    let _ = update(
        &mut model,
        AppMsg::Editor(EditorMsg::SourceChanged("Typed more".to_owned())),
    );

    // The error is stale for the newer source, so the save restarts with the move preserved.
    let effects = update(
        &mut model,
        AppMsg::Library(LibraryReply::EditorSaved {
            request,
            result: Err(UiError::new("write failed")),
            move_error: None,
        }),
    );
    let restarted = match effects.as_slice() {
        [Effect::SaveNote { request }] => request.clone(),
        _ => panic!("a superseded failure should restart the save"),
    };
    assert_eq!(restarted.move_to, Some(category));
    assert_eq!(restarted.source, "Typed more");
}
