use super::*;

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
            source_target: None,
        }]
    );
    let effects = update(
        &mut model,
        AppMsg::Library(LibraryReply::EditorAssetStored {
            session,
            alt: "Pasted image".to_owned(),
            source_target: None,
            result: Ok("assets/pasted.png".to_owned()),
        }),
    );

    assert!(matches!(
        effects.as_slice(),
        [
            Effect::SchedulePreview { session: preview_session, .. },
            Effect::ScheduleEditorSave { session: save_session, .. },
            Effect::ReloadRichEditor { session: reload_session, source },
        ] if *preview_session == session
            && *save_session == session
            && *reload_session == session
            && source == "Before\n![Pasted image](assets/pasted.png)\n"
    ));
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
                source_target: None,
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
    let target = SourceImageTarget {
        source: String::from("Before remove After"),
        selection: 7..13,
    };

    let effects = update(
        &mut model,
        AppMsg::Editor(EditorMsg::ImportImage {
            extension: "png".to_owned(),
            bytes: vec![1, 2, 3],
            alt: "Pasted image".to_owned(),
            source_target: Some(target.clone()),
        }),
    );
    assert!(
        matches!(effects.as_slice(), [Effect::StoreEditorAsset { source_target: Some(actual), .. }] if actual == &target)
    );

    let _ = update(
        &mut model,
        AppMsg::Library(LibraryReply::EditorAssetStored {
            session,
            alt: "Pasted image".to_owned(),
            source_target: Some(target),
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
fn source_image_import_should_not_replace_text_changed_while_asset_stores() {
    let mut model = AppModel::new(&Config::default());
    let note_id = NoteId::new();
    let _ = update(
        &mut model,
        AppMsg::Editor(EditorMsg::Load {
            note_id,
            revision: Revision(1),
            source: String::from("Before remove After"),
        }),
    );
    let Some(session) = model.editor.as_ref().map(|document| document.session) else {
        panic!("editor should be open");
    };
    let target = SourceImageTarget {
        source: String::from("Before remove After"),
        selection: 7..13,
    };

    let _ = update(
        &mut model,
        AppMsg::Editor(EditorMsg::ImportImage {
            extension: String::from("png"),
            bytes: vec![1, 2, 3],
            alt: String::from("Pasted image"),
            source_target: Some(target.clone()),
        }),
    );
    let _ = update(
        &mut model,
        AppMsg::Editor(EditorMsg::SourceChanged(String::from(
            "Before typed remove After",
        ))),
    );
    let _ = update(
        &mut model,
        AppMsg::Library(LibraryReply::EditorAssetStored {
            session,
            alt: String::from("Pasted image"),
            source_target: Some(target),
            result: Ok(String::from("assets/pasted.png")),
        }),
    );

    assert_eq!(
        model
            .editor
            .as_ref()
            .map(|document| document.source.as_str()),
        Some("Before typed remove After\n![Pasted image](assets/pasted.png)\n")
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
