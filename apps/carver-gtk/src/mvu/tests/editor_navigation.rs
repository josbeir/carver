use super::*;

#[test]
fn completed_autosave_should_reload_browser_only_when_leaving_the_editor() {
    let mut model = AppModel::new(&Config::default());
    let note_id = NoteId::new();
    let _ = update(
        &mut model,
        AppMsg::Editor(EditorMsg::Load {
            note_id,
            revision: Revision(1),
            source: String::from("Initial"),
        }),
    );
    let _ = update(
        &mut model,
        AppMsg::Editor(EditorMsg::SourceChanged(String::from("Saved"))),
    );
    let effects = update(&mut model, AppMsg::Editor(EditorMsg::AutosaveRequested));
    let (session, timer_id) = match effects.as_slice() {
        [
            Effect::ScheduleEditorSave {
                session, timer_id, ..
            },
        ] => (*session, *timer_id),
        _ => panic!("an edited document should schedule one autosave"),
    };
    let effects = update(
        &mut model,
        AppMsg::Editor(EditorMsg::AutosaveElapsed { session, timer_id }),
    );
    let request = match effects.as_slice() {
        [Effect::SaveNote { request }] => request.clone(),
        _ => panic!("the autosave timer should persist the document"),
    };

    let effects = update(
        &mut model,
        AppMsg::Library(LibraryReply::EditorSaved {
            request,
            result: Ok(Revision(2)),
            move_error: None,
        }),
    );
    assert!(
        effects
            .iter()
            .any(|effect| matches!(effect, Effect::LoadNoteLinks { .. }))
    );
    assert!(matches!(
        effects.last(),
        Some(Effect::LoadLibraryRevision { .. })
    ));
    assert_eq!(model.route, Route::Editor);

    let effects = update(&mut model, AppMsg::Editor(EditorMsg::BackRequested));
    assert!(matches!(effects.as_slice(), [Effect::LoadBrowser { .. }]));
    assert_eq!(model.route, Route::Browser);
}

#[test]
fn opening_a_linked_note_with_a_clean_editor_should_load_it() {
    let mut model = AppModel::new(&Config::default());
    let _ = update(
        &mut model,
        AppMsg::Editor(EditorMsg::Load {
            note_id: NoteId::new(),
            revision: Revision(1),
            source: String::from("Current"),
        }),
    );
    let target = NoteId::new();

    let effects = update(
        &mut model,
        AppMsg::Navigation(NavigationMsg::OpenNote {
            note_id: target,
            intent: NoteOpenIntent::Default,
        }),
    );

    assert!(matches!(
        effects.as_slice(),
        [Effect::LoadEditorNote {
            note_id,
            ..
        }] if *note_id == target
    ));
    assert!(model.editor.is_none());
}

#[test]
fn opening_a_linked_note_should_open_a_new_tab_and_keep_the_dirty_tab() {
    let mut model = AppModel::new(&Config::default());
    let first_note = NoteId::new();
    let _ = update(
        &mut model,
        AppMsg::Editor(EditorMsg::Load {
            note_id: first_note,
            revision: Revision(1),
            source: String::from("Initial"),
        }),
    );
    let _ = update(
        &mut model,
        AppMsg::Editor(EditorMsg::SourceChanged(String::from("Dirty"))),
    );
    let target = NoteId::new();

    let effects = update(
        &mut model,
        AppMsg::Navigation(NavigationMsg::OpenNote {
            note_id: target,
            intent: NoteOpenIntent::Default,
        }),
    );
    assert!(matches!(
        effects.as_slice(),
        [Effect::LoadEditorNote { note_id, .. }] if *note_id == target
    ));
    // The dirty document is stashed for its own tab instead of being saved or discarded.
    assert_eq!(
        model
            .active_tab
            .map(|tab_id| model.note_tab(tab_id).map(|tab| tab.note_id)),
        Some(Some(target))
    );
    assert!(model.editor.is_none());
    assert!(
        model
            .background_documents
            .values()
            .any(|document| document.note_id == first_note)
    );
}

#[test]
fn selecting_a_category_should_reload_that_category_after_closing_a_clean_editor() {
    let mut model = AppModel::new(&Config::default());
    let category_id = CategoryId::new();
    let _ = update(
        &mut model,
        AppMsg::Editor(EditorMsg::Load {
            note_id: NoteId::new(),
            revision: Revision(1),
            source: String::new(),
        }),
    );

    let effects = update(
        &mut model,
        AppMsg::Navigation(NavigationMsg::SelectCategory(Some(category_id))),
    );

    assert!(matches!(
        effects.as_slice(),
        [Effect::LoadBrowser {
            category_id: Some(actual_category_id),
            ..
        }] if *actual_category_id == category_id
    ));
    assert_eq!(model.route, Route::Browser);
    assert_eq!(model.selected_category, Some(category_id));
}
