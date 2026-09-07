use super::*;

#[test]
fn favorite_requested_with_dirty_source_should_save_before_updating_metadata() {
    let mut model = AppModel::new(&Config::default());
    let note_id = NoteId::new();
    let _ = update(
        &mut model,
        AppMsg::Editor(EditorMsg::Load {
            note_id,
            revision: Revision(1),
            source: String::from("Before"),
        }),
    );
    let _ = update(
        &mut model,
        AppMsg::Editor(EditorMsg::SourceChanged(String::from("After"))),
    );

    let effects = update(&mut model, AppMsg::Editor(EditorMsg::ToggleFavorite));
    let request = match effects.as_slice() {
        [Effect::SaveNote { request }] => request.clone(),
        _ => panic!("favorite changes should save dirty source first"),
    };

    let effects = update(
        &mut model,
        AppMsg::Library(LibraryReply::EditorSaved {
            request,
            result: Ok(Revision(2)),
        }),
    );

    assert!(effects.iter().any(|effect| matches!(
        effect,
        Effect::SetNoteFavorite {
            note_id: effect_note_id,
            revision: Revision(2),
            is_favorite: true,
            ..
        } if *effect_note_id == note_id
    )));
}

#[test]
fn repeated_favorite_toggles_while_saving_should_restore_the_original_state() {
    let mut model = AppModel::new(&Config::default());
    let note_id = NoteId::new();
    let _ = update(
        &mut model,
        AppMsg::Editor(EditorMsg::Load {
            note_id,
            revision: Revision(1),
            source: String::from("Before"),
        }),
    );
    let _ = update(
        &mut model,
        AppMsg::Editor(EditorMsg::SourceChanged(String::from("After"))),
    );
    let request = match update(&mut model, AppMsg::Editor(EditorMsg::ToggleFavorite)).as_slice() {
        [Effect::SaveNote { request }] => request.clone(),
        _ => panic!("favorite changes should save dirty source first"),
    };

    assert!(update(&mut model, AppMsg::Editor(EditorMsg::ToggleFavorite)).is_empty());
    assert_eq!(
        model
            .editor
            .as_ref()
            .and_then(|document| document.pending_favorite),
        Some(false)
    );

    let effects = update(
        &mut model,
        AppMsg::Library(LibraryReply::EditorSaved {
            request,
            result: Ok(Revision(2)),
        }),
    );
    assert!(
        !effects
            .iter()
            .any(|effect| matches!(effect, Effect::SetNoteFavorite { .. }))
    );
    assert_eq!(
        model
            .editor
            .as_ref()
            .and_then(|document| document.pending_favorite),
        None
    );
}

#[test]
fn repeated_favorite_toggles_while_clean_should_apply_the_latest_requested_state() {
    let mut model = AppModel::new(&Config::default());
    let note_id = NoteId::new();
    let category_id = CategoryId::new();
    let _ = update(
        &mut model,
        AppMsg::Editor(EditorMsg::Load {
            note_id,
            revision: Revision(1),
            source: String::from("Source"),
        }),
    );

    assert!(matches!(
        update(&mut model, AppMsg::Editor(EditorMsg::ToggleFavorite)).as_slice(),
        [Effect::SetNoteFavorite {
            note_id: effect_note_id,
            revision: Revision(1),
            is_favorite: true,
            ..
        }] if *effect_note_id == note_id
    ));
    assert!(update(&mut model, AppMsg::Editor(EditorMsg::ToggleFavorite)).is_empty());

    let effects = update(
        &mut model,
        AppMsg::Library(LibraryReply::FavoriteChanged {
            action: ActionKey::SetNoteFavorite(note_id),
            result: Ok(Note {
                id: note_id,
                category_id,
                source: String::from("Source"),
                title: String::from("Source"),
                plain_text: String::from("Source"),
                revision: Revision(2),
                is_favorite: true,
                created_at: OffsetDateTime::UNIX_EPOCH,
                updated_at: OffsetDateTime::UNIX_EPOCH,
                trashed_at: None,
            }),
        }),
    );
    assert!(effects.iter().any(|effect| matches!(
        effect,
        Effect::SetNoteFavorite {
            note_id: effect_note_id,
            revision: Revision(2),
            is_favorite: false,
            ..
        } if *effect_note_id == note_id
    )));

    let effects = update(
        &mut model,
        AppMsg::Library(LibraryReply::FavoriteChanged {
            action: ActionKey::SetNoteFavorite(note_id),
            result: Ok(Note {
                id: note_id,
                category_id,
                source: String::from("Source"),
                title: String::from("Source"),
                plain_text: String::from("Source"),
                revision: Revision(3),
                is_favorite: false,
                created_at: OffsetDateTime::UNIX_EPOCH,
                updated_at: OffsetDateTime::UNIX_EPOCH,
                trashed_at: None,
            }),
        }),
    );
    assert!(
        !effects
            .iter()
            .any(|effect| matches!(effect, Effect::SetNoteFavorite { .. }))
    );
    assert_eq!(
        model
            .editor
            .as_ref()
            .and_then(|document| document.pending_favorite),
        None
    );
}

#[test]
fn favorite_requested_before_closing_a_dirty_editor_should_run_after_saving() {
    let mut model = AppModel::new(&Config::default());
    let note_id = NoteId::new();
    let _ = update(
        &mut model,
        AppMsg::Editor(EditorMsg::Load {
            note_id,
            revision: Revision(1),
            source: String::from("Before"),
        }),
    );
    let _ = update(
        &mut model,
        AppMsg::Editor(EditorMsg::SourceChanged(String::from("After"))),
    );
    let request = match update(&mut model, AppMsg::Editor(EditorMsg::ToggleFavorite)).as_slice() {
        [Effect::SaveNote { request }] => request.clone(),
        _ => panic!("favorite changes should save dirty source first"),
    };
    let _ = update(&mut model, AppMsg::Editor(EditorMsg::BackRequested));

    let effects = update(
        &mut model,
        AppMsg::Library(LibraryReply::EditorSaved {
            request,
            result: Ok(Revision(2)),
        }),
    );

    assert!(model.editor.is_none());
    assert!(effects.iter().any(|effect| matches!(
        effect,
        Effect::SetNoteFavorite {
            note_id: effect_note_id,
            revision: Revision(2),
            is_favorite: true,
            ..
        } if *effect_note_id == note_id
    )));
}

#[test]
fn favorite_completion_should_rebase_a_save_started_with_its_old_revision() {
    let mut model = AppModel::new(&Config::default());
    let note_id = NoteId::new();
    let category_id = CategoryId::new();
    let _ = update(
        &mut model,
        AppMsg::Editor(EditorMsg::Load {
            note_id,
            revision: Revision(1),
            source: String::from("Before"),
        }),
    );
    let _ = update(&mut model, AppMsg::Editor(EditorMsg::ToggleFavorite));
    let _ = update(
        &mut model,
        AppMsg::Editor(EditorMsg::SourceChanged(String::from("After"))),
    );
    let stale_request = match update(&mut model, AppMsg::Editor(EditorMsg::RetrySave)).as_slice() {
        [Effect::SaveNote { request }] => request.clone(),
        _ => panic!("editing after favoriting should start an autosave"),
    };

    let effects = update(
        &mut model,
        AppMsg::Library(LibraryReply::FavoriteChanged {
            action: ActionKey::SetNoteFavorite(note_id),
            result: Ok(Note {
                id: note_id,
                category_id,
                source: String::from("Before"),
                title: String::from("Before"),
                plain_text: String::from("Before"),
                revision: Revision(2),
                is_favorite: true,
                created_at: OffsetDateTime::UNIX_EPOCH,
                updated_at: OffsetDateTime::UNIX_EPOCH,
                trashed_at: None,
            }),
        }),
    );
    let rebased_request = match effects.as_slice() {
        [Effect::SaveNote { request }, ..] => request.clone(),
        _ => panic!("favorite completion should restart the stale save"),
    };
    assert_eq!(rebased_request.expected_revision, Revision(2));

    assert!(
        update(
            &mut model,
            AppMsg::Library(LibraryReply::EditorSaved {
                request: stale_request,
                result: Err(UiError::new("revision conflict")),
            }),
        )
        .is_empty()
    );
    let effects = update(
        &mut model,
        AppMsg::Library(LibraryReply::EditorSaved {
            request: rebased_request,
            result: Ok(Revision(3)),
        }),
    );

    assert!(effects.is_empty());
    let Some(document) = model.editor else {
        panic!("editor should remain open");
    };
    assert_eq!(document.revision, Revision(3));
    assert_eq!(document.save_state, super::EditorSaveState::Clean);
}

#[test]
fn rebased_save_should_finish_before_a_queued_favorite_reversal() {
    let mut model = AppModel::new(&Config::default());
    let note_id = NoteId::new();
    let category_id = CategoryId::new();
    let _ = update(
        &mut model,
        AppMsg::Editor(EditorMsg::Load {
            note_id,
            revision: Revision(1),
            source: String::from("Before"),
        }),
    );
    let _ = update(&mut model, AppMsg::Editor(EditorMsg::ToggleFavorite));
    let _ = update(
        &mut model,
        AppMsg::Editor(EditorMsg::SourceChanged(String::from("After"))),
    );
    let stale_request = match update(&mut model, AppMsg::Editor(EditorMsg::RetrySave)).as_slice() {
        [Effect::SaveNote { request }] => request.clone(),
        _ => panic!("editing after favoriting should start an autosave"),
    };
    let _ = update(&mut model, AppMsg::Editor(EditorMsg::ToggleFavorite));

    let effects = update(
        &mut model,
        AppMsg::Library(LibraryReply::FavoriteChanged {
            action: ActionKey::SetNoteFavorite(note_id),
            result: Ok(Note {
                id: note_id,
                category_id,
                source: String::from("Before"),
                title: String::from("Before"),
                plain_text: String::from("Before"),
                revision: Revision(2),
                is_favorite: true,
                created_at: OffsetDateTime::UNIX_EPOCH,
                updated_at: OffsetDateTime::UNIX_EPOCH,
                trashed_at: None,
            }),
        }),
    );
    let rebased_request = match effects.as_slice() {
        [Effect::SaveNote { request }, ..] => request.clone(),
        _ => panic!("favorite completion should restart the stale save"),
    };
    assert!(
        !effects
            .iter()
            .any(|effect| matches!(effect, Effect::SetNoteFavorite { .. }))
    );

    let _ = update(
        &mut model,
        AppMsg::Library(LibraryReply::EditorSaved {
            request: stale_request,
            result: Err(UiError::new("revision conflict")),
        }),
    );
    let effects = update(
        &mut model,
        AppMsg::Library(LibraryReply::EditorSaved {
            request: rebased_request,
            result: Ok(Revision(3)),
        }),
    );
    assert!(effects.iter().any(|effect| matches!(
        effect,
        Effect::SetNoteFavorite {
            note_id: effect_note_id,
            revision: Revision(3),
            is_favorite: false,
            ..
        } if *effect_note_id == note_id
    )));
}

#[test]
fn favorite_reversal_should_finish_before_a_clean_editor_closes() {
    let mut model = AppModel::new(&Config::default());
    let note_id = NoteId::new();
    let category_id = CategoryId::new();
    let _ = update(
        &mut model,
        AppMsg::Editor(EditorMsg::Load {
            note_id,
            revision: Revision(1),
            source: String::from("Source"),
        }),
    );
    let _ = update(&mut model, AppMsg::Editor(EditorMsg::ToggleFavorite));
    let _ = update(&mut model, AppMsg::Editor(EditorMsg::ToggleFavorite));
    assert!(update(&mut model, AppMsg::Editor(EditorMsg::BackRequested)).is_empty());
    assert_eq!(model.route, Route::Editor);
    assert!(model.editor.is_some());

    let effects = update(
        &mut model,
        AppMsg::Library(LibraryReply::FavoriteChanged {
            action: ActionKey::SetNoteFavorite(note_id),
            result: Ok(Note {
                id: note_id,
                category_id,
                source: String::from("Source"),
                title: String::from("Source"),
                plain_text: String::from("Source"),
                revision: Revision(2),
                is_favorite: true,
                created_at: OffsetDateTime::UNIX_EPOCH,
                updated_at: OffsetDateTime::UNIX_EPOCH,
                trashed_at: None,
            }),
        }),
    );
    assert!(model.editor.is_some());
    assert!(effects.iter().any(|effect| matches!(
        effect,
        Effect::SetNoteFavorite {
            note_id: effect_note_id,
            revision: Revision(2),
            is_favorite: false,
            ..
        } if *effect_note_id == note_id
    )));

    let _ = update(
        &mut model,
        AppMsg::Library(LibraryReply::FavoriteChanged {
            action: ActionKey::SetNoteFavorite(note_id),
            result: Ok(Note {
                id: note_id,
                category_id,
                source: String::from("Source"),
                title: String::from("Source"),
                plain_text: String::from("Source"),
                revision: Revision(3),
                is_favorite: false,
                created_at: OffsetDateTime::UNIX_EPOCH,
                updated_at: OffsetDateTime::UNIX_EPOCH,
                trashed_at: None,
            }),
        }),
    );
    assert_eq!(model.route, Route::Browser);
    assert!(model.editor.is_none());
}

#[test]
fn category_selection_should_complete_after_a_clean_favorite_mutation_closes() {
    let mut model = AppModel::new(&Config::default());
    let note_id = NoteId::new();
    let category_id = CategoryId::new();
    let selected_category_id = CategoryId::new();
    let _ = update(
        &mut model,
        AppMsg::Editor(EditorMsg::Load {
            note_id,
            revision: Revision(1),
            source: String::from("Source"),
        }),
    );
    let _ = update(&mut model, AppMsg::Editor(EditorMsg::ToggleFavorite));
    assert!(
        update(
            &mut model,
            AppMsg::Navigation(NavigationMsg::SelectCategory(Some(selected_category_id))),
        )
        .is_empty()
    );
    assert!(model.editor.is_some());

    let effects = update(
        &mut model,
        AppMsg::Library(LibraryReply::FavoriteChanged {
            action: ActionKey::SetNoteFavorite(note_id),
            result: Ok(Note {
                id: note_id,
                category_id,
                source: String::from("Source"),
                title: String::from("Source"),
                plain_text: String::from("Source"),
                revision: Revision(2),
                is_favorite: true,
                created_at: OffsetDateTime::UNIX_EPOCH,
                updated_at: OffsetDateTime::UNIX_EPOCH,
                trashed_at: None,
            }),
        }),
    );

    assert_eq!(model.route, Route::Browser);
    assert_eq!(model.selected_category, Some(selected_category_id));
    assert!(model.editor.is_none());
    assert!(effects.iter().any(|effect| matches!(
        effect,
        Effect::LoadBrowser {
            category_id: Some(actual_category_id),
            ..
        } if *actual_category_id == selected_category_id
    )));
}

#[test]
fn favorite_action_should_use_the_summary_revision() {
    let mut model = AppModel::new(&Config::default());
    let note_id = NoteId::new();

    let effects = update(
        &mut model,
        AppMsg::Action(ActionMsg::SetNoteFavorite {
            note_id,
            revision: Revision(3),
            is_favorite: true,
        }),
    );

    assert_eq!(
        effects,
        vec![Effect::SetNoteFavorite {
            action: ActionKey::SetNoteFavorite(note_id),
            note_id,
            revision: Revision(3),
            is_favorite: true,
        }]
    );
}
