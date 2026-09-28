use super::*;
use crate::mvu::TabsMsg;

fn load_note(model: &mut AppModel, note_id: NoteId) -> crate::mvu::TabId {
    let _ = update(
        model,
        AppMsg::Editor(EditorMsg::Load {
            note_id,
            revision: Revision(1),
            source: String::from("# Note"),
        }),
    );
    match model.note_tab_for_note(note_id) {
        Some(tab_id) => tab_id,
        None => panic!("loading a note should open a tab"),
    }
}

#[test]
fn loading_a_second_note_should_open_a_new_tab_and_stash_the_first() {
    let mut model = AppModel::new(&Config::default());
    let first = NoteId::new();
    let second = NoteId::new();
    let first_tab = load_note(&mut model, first);
    let second_tab = load_note(&mut model, second);

    assert_eq!(model.note_tabs.len(), 2);
    assert_eq!(model.active_tab, Some(second_tab));
    assert_eq!(model.route, Route::Editor);
    assert_eq!(
        model.editor.as_ref().map(|document| document.note_id),
        Some(second)
    );
    assert!(
        model
            .background_documents
            .get(&first_tab)
            .is_some_and(|document| document.note_id == first)
    );
}

#[test]
fn opening_an_already_open_note_should_focus_its_tab() {
    let mut model = AppModel::new(&Config::default());
    let first = NoteId::new();
    let second = NoteId::new();
    let first_tab = load_note(&mut model, first);
    let _second_tab = load_note(&mut model, second);

    let effects = update(
        &mut model,
        AppMsg::Tabs(TabsMsg::OpenNote {
            note_id: first,
            background: false,
        }),
    );

    assert_eq!(model.note_tabs.len(), 2);
    assert_eq!(model.active_tab, Some(first_tab));
    assert_eq!(
        model.editor.as_ref().map(|document| document.note_id),
        Some(first)
    );
    assert!(
        effects
            .iter()
            .any(|effect| matches!(effect, Effect::FocusEditor { .. }))
    );
}

#[test]
fn opening_a_note_in_the_background_should_keep_the_active_tab() {
    let mut model = AppModel::new(&Config::default());
    let first = NoteId::new();
    let second = NoteId::new();
    let first_tab = load_note(&mut model, first);

    let effects = update(
        &mut model,
        AppMsg::Tabs(TabsMsg::OpenNote {
            note_id: second,
            background: true,
        }),
    );
    let (request_id, second_tab) = match effects.as_slice() {
        [
            Effect::LoadEditorNote {
                request_id, tab_id, ..
            },
        ] => (*request_id, *tab_id),
        _ => panic!("background open should start one load"),
    };
    assert_eq!(model.active_tab, Some(first_tab));

    let _ = update(
        &mut model,
        AppMsg::Library(LibraryReply::EditorLoaded {
            request_id,
            tab_id: second_tab,
            result: Ok(Note {
                id: second,
                category_id: CategoryId::new(),
                source: String::from("# Second"),
                title: String::from("Second"),
                plain_text: String::from("Second"),
                revision: Revision(1),
                is_favorite: false,
                created_at: OffsetDateTime::UNIX_EPOCH,
                updated_at: OffsetDateTime::UNIX_EPOCH,
                trashed_at: None,
            }),
        }),
    );
    assert_eq!(model.active_tab, Some(first_tab));
    assert!(
        model
            .background_documents
            .get(&second_tab)
            .is_some_and(|document| document.note_id == second)
    );
}

#[test]
fn closing_the_active_tab_should_activate_a_neighbor() {
    let mut model = AppModel::new(&Config::default());
    let first = NoteId::new();
    let second = NoteId::new();
    let first_tab = load_note(&mut model, first);
    let second_tab = load_note(&mut model, second);

    let _ = update(&mut model, AppMsg::Tabs(TabsMsg::Close(second_tab)));

    assert_eq!(model.note_tabs.len(), 1);
    assert_eq!(model.note_tabs[0].id, first_tab);
    assert_eq!(model.active_tab, Some(first_tab));
    assert_eq!(
        model.editor.as_ref().map(|document| document.note_id),
        Some(first)
    );
}

#[test]
fn closing_the_last_tab_should_return_to_the_notes_list() {
    let mut model = AppModel::new(&Config::default());
    let note = NoteId::new();
    let tab = load_note(&mut model, note);

    let effects = update(&mut model, AppMsg::Tabs(TabsMsg::Close(tab)));

    assert!(model.note_tabs.is_empty());
    assert_eq!(model.active_tab, None);
    assert_eq!(model.route, Route::Browser);
    assert!(model.editor.is_none());
    assert!(
        effects
            .iter()
            .any(|effect| matches!(effect, Effect::LoadBrowser { .. }))
    );
}

#[test]
fn closing_a_dirty_tab_should_save_it_without_keeping_the_tab() {
    let mut model = AppModel::new(&Config::default());
    let tab = open_and_load(&mut model, NoteId::new());
    let _ = update(
        &mut model,
        AppMsg::Editor(EditorMsg::SourceChanged(String::from("Dirty"))),
    );

    let effects = update(&mut model, AppMsg::Tabs(TabsMsg::Close(tab)));
    let Some(request) = effects.iter().find_map(|effect| match effect {
        Effect::SaveNote { request } => Some(request.clone()),
        _ => None,
    }) else {
        panic!("closing a dirty tab should start a save");
    };
    // The tab is gone immediately; the document keeps saving in the background.
    assert!(model.note_tabs.is_empty());
    assert!(model.active_tab.is_none());
    assert_eq!(model.closing_documents.len(), 1);

    let _ = update(
        &mut model,
        AppMsg::Library(LibraryReply::EditorSaved {
            request,
            result: Ok(Revision(2)),
            move_error: None,
        }),
    );
    assert!(model.closing_documents.is_empty());
}

#[test]
fn reordering_tabs_should_update_the_model_order() {
    let mut model = AppModel::new(&Config::default());
    let first_tab = load_note(&mut model, NoteId::new());
    let second_tab = load_note(&mut model, NoteId::new());

    let _ = update(
        &mut model,
        AppMsg::Tabs(TabsMsg::Reordered {
            tab_id: second_tab,
            position: 0,
        }),
    );

    assert_eq!(
        model.note_tabs.iter().map(|tab| tab.id).collect::<Vec<_>>(),
        vec![second_tab, first_tab]
    );
}

/// Opens a note through the tab flow and completes its asynchronous load.
fn open_and_load(model: &mut AppModel, note_id: NoteId) -> crate::mvu::TabId {
    let effects = update(
        model,
        AppMsg::Tabs(TabsMsg::OpenNote {
            note_id,
            background: false,
        }),
    );
    let (request_id, tab_id) = match effects.as_slice() {
        [
            Effect::LoadEditorNote {
                request_id, tab_id, ..
            },
        ] => (*request_id, *tab_id),
        _ => panic!("opening a note should start one load"),
    };
    let _ = update(
        model,
        AppMsg::Library(LibraryReply::EditorLoaded {
            request_id,
            tab_id,
            result: Ok(Note {
                id: note_id,
                category_id: CategoryId::new(),
                source: String::from("# Note"),
                title: String::from("Note"),
                plain_text: String::from("Note"),
                revision: Revision(1),
                is_favorite: false,
                created_at: OffsetDateTime::UNIX_EPOCH,
                updated_at: OffsetDateTime::UNIX_EPOCH,
                trashed_at: None,
            }),
        }),
    );
    tab_id
}

#[test]
fn creating_a_note_should_open_a_new_tab() {
    let mut model = AppModel::new(&Config::default());
    let note_id = NoteId::new();
    let _ = update(
        &mut model,
        AppMsg::Library(LibraryReply::NoteCreated {
            result: Ok(Note {
                id: note_id,
                category_id: CategoryId::new(),
                source: String::from("# Fresh"),
                title: String::from("Fresh"),
                plain_text: String::from("Fresh"),
                revision: Revision(1),
                is_favorite: false,
                created_at: OffsetDateTime::UNIX_EPOCH,
                updated_at: OffsetDateTime::UNIX_EPOCH,
                trashed_at: None,
            }),
        }),
    );

    assert_eq!(model.note_tabs.len(), 1);
    assert_eq!(model.note_tabs[0].note_id, note_id);
    assert_eq!(model.active_tab, Some(model.note_tabs[0].id));
    assert_eq!(model.route, Route::Editor);
    assert_eq!(
        model.editor.as_ref().map(|document| document.note_id),
        Some(note_id)
    );
}

#[test]
fn back_should_return_to_the_previously_active_tab() {
    let mut model = AppModel::new(&Config::default());
    let first = NoteId::new();
    let second = NoteId::new();
    let first_tab = open_and_load(&mut model, first);
    let second_tab = open_and_load(&mut model, second);
    assert_eq!(model.active_tab, Some(second_tab));

    // Back returns to the note the current one was opened from.
    let _ = update(&mut model, AppMsg::Editor(EditorMsg::BackRequested));
    assert_eq!(model.active_tab, Some(first_tab));
    assert_eq!(model.route, Route::Editor);

    // Back again falls through to the pinned Notes list.
    let _ = update(&mut model, AppMsg::Editor(EditorMsg::BackRequested));
    assert_eq!(model.active_tab, None);
    assert_eq!(model.route, Route::Browser);
}

#[test]
fn closing_a_tab_should_drop_it_from_the_back_history() {
    let mut model = AppModel::new(&Config::default());
    let first_tab = open_and_load(&mut model, NoteId::new());
    let second_tab = open_and_load(&mut model, NoteId::new());

    let _ = update(&mut model, AppMsg::Tabs(TabsMsg::Close(first_tab)));
    // The closed tab must not be reachable through Back.
    let _ = update(&mut model, AppMsg::Editor(EditorMsg::BackRequested));
    assert_ne!(model.active_tab, Some(first_tab));
    assert_ne!(model.active_tab, Some(second_tab));
    assert_eq!(model.active_tab, None);
    assert_eq!(model.note_tabs.len(), 1);
    assert_eq!(model.note_tabs[0].id, second_tab);
}
