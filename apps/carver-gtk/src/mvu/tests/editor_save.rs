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
