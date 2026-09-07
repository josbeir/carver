use super::*;

#[test]
fn successful_trash_mutation_should_reload_each_dependent_resource_once() {
    let mut model = AppModel::new(&Config::default());
    let effects = update(
        &mut model,
        AppMsg::Trash(TrashMsg::RestoreNote(NoteId::new())),
    );
    assert!(matches!(effects.as_slice(), [Effect::RestoreNote { .. }]));

    let effects = update(
        &mut model,
        AppMsg::Library(LibraryReply::TrashMutationFinished {
            result: Ok(TrashMutation::NoteRestored),
        }),
    );

    assert_eq!(
        effects,
        vec![
            Effect::LoadSidebar {
                request_id: RequestId(1),
            },
            Effect::LoadBrowser {
                request_id: RequestId(2),
                category_id: None,
                query: String::new(),
            },
            Effect::LoadTrash {
                request_id: RequestId(3),
            },
            Effect::LoadLibraryRevision {
                request_id: RequestId(4),
            },
        ]
    );
}

#[test]
fn failed_trash_mutation_should_not_discard_loaded_resources() {
    let mut model = AppModel::new(&Config::default());

    let effects = update(
        &mut model,
        AppMsg::Library(LibraryReply::TrashMutationFinished {
            result: Err(UiError::new("restore failed")),
        }),
    );

    assert!(effects.is_empty());
    assert_eq!(model.trash.state, LoadState::Idle);
    assert_eq!(model.notice, Some(UiError::new("restore failed")));
}

#[test]
fn duplicate_category_rename_should_start_one_mutation() {
    let mut model = AppModel::new(&Config::default());
    let category_id = CategoryId::new();
    let message = AppMsg::Action(ActionMsg::RenameCategory {
        category_id,
        name: "Renamed".to_owned(),
    });

    let first = update(&mut model, message.clone());
    let second = update(&mut model, message);

    assert_eq!(
        first,
        vec![Effect::RenameCategory {
            category_id,
            name: "Renamed".to_owned(),
        }]
    );
    assert!(second.is_empty());
    assert_eq!(
        model.pending_actions,
        std::collections::BTreeSet::from([ActionKey::RenameCategory(category_id)])
    );
}

#[test]
fn category_appearance_update_should_start_one_mutation() {
    let mut model = AppModel::new(&Config::default());
    let category_id = CategoryId::new();
    let appearance = carver_sdk::CategoryAppearance {
        icon: carver_sdk::CategoryIcon::Heart,
        color: carver_sdk::CategoryColor::Rose,
    };
    let message = AppMsg::Action(ActionMsg::UpdateCategory {
        category_id,
        name: "Personal".to_owned(),
        appearance,
    });

    let first = update(&mut model, message.clone());
    let second = update(&mut model, message);

    assert_eq!(
        first,
        vec![Effect::UpdateCategory {
            category_id,
            name: "Personal".to_owned(),
            appearance,
        }]
    );
    assert!(second.is_empty());
    assert_eq!(
        model.pending_actions,
        std::collections::BTreeSet::from([ActionKey::UpdateCategory(category_id)])
    );
}

#[test]
fn invalid_category_name_should_preserve_loaded_resources_and_surface_an_error() {
    let mut model = AppModel::new(&Config::default());
    let _ = update(&mut model, AppMsg::Sidebar(SidebarMsg::Reload));

    let effects = update(
        &mut model,
        AppMsg::Action(ActionMsg::CreateCategory("  ".to_owned())),
    );

    assert!(effects.is_empty());
    assert_eq!(model.sidebar.state, LoadState::Loading(RequestId(1)));
    assert_eq!(
        model.notice,
        Some(UiError::new("Category names cannot be empty."))
    );
    assert!(model.pending_actions.is_empty());
}

#[test]
fn failed_action_should_not_invalidate_loaded_resources() {
    let mut model = AppModel::new(&Config::default());
    model.sidebar.state = LoadState::Ready(Vec::new());
    let category_id = CategoryId::new();
    let _ = update(
        &mut model,
        AppMsg::Action(ActionMsg::TrashCategory(category_id)),
    );

    let effects = update(
        &mut model,
        AppMsg::Library(LibraryReply::ActionFinished {
            action: ActionKey::TrashCategory(category_id),
            result: Err(UiError::new("trash failed")),
        }),
    );

    assert!(effects.is_empty());
    assert_eq!(model.sidebar.state, LoadState::Ready(Vec::new()));
    assert_eq!(model.notice, Some(UiError::new("trash failed")));
}

#[test]
fn trashing_the_selected_category_should_clear_selection_and_ensure_a_default() {
    let mut model = AppModel::new(&Config::default());
    let category_id = CategoryId::new();
    model.selected_category = Some(category_id);
    let _ = update(
        &mut model,
        AppMsg::Action(ActionMsg::TrashCategory(category_id)),
    );

    let effects = update(
        &mut model,
        AppMsg::Library(LibraryReply::ActionFinished {
            action: ActionKey::TrashCategory(category_id),
            result: Ok(()),
        }),
    );

    assert_eq!(model.selected_category, None);
    assert!(effects.contains(&Effect::EnsureDefaultCategory));
}

#[test]
fn moved_note_should_offer_undo_and_reload_dependent_resources_once() {
    let mut model = AppModel::new(&Config::default());
    let note_id = NoteId::new();
    let source_category_id = CategoryId::new();
    let destination_category_id = CategoryId::new();
    let action = ActionKey::MoveNote {
        note_id,
        source_category_id,
    };

    let effects = update(
        &mut model,
        AppMsg::Action(ActionMsg::MoveNote {
            note_id,
            source_category_id,
            category_id: destination_category_id,
        }),
    );
    assert_eq!(
        effects,
        vec![Effect::MoveNote {
            action,
            note_id,
            category_id: destination_category_id,
        }]
    );

    let effects = update(
        &mut model,
        AppMsg::Library(LibraryReply::ActionFinished {
            action,
            result: Ok(()),
        }),
    );
    assert_eq!(
        model.undo_move,
        Some(super::MoveUndo {
            note_id,
            source_category_id,
        })
    );
    assert!(matches!(
        effects.as_slice(),
        [
            Effect::LoadSidebar { .. },
            Effect::LoadBrowser { .. },
            Effect::LoadTrash { .. },
            Effect::LoadLibraryRevision { .. }
        ]
    ));
}

#[test]
fn creating_a_category_for_a_note_should_move_it_as_one_undoable_action() {
    let mut model = AppModel::new(&Config::default());
    let note_id = NoteId::new();
    let source_category_id = CategoryId::new();
    let action = ActionKey::MoveNote {
        note_id,
        source_category_id,
    };

    let effects = update(
        &mut model,
        AppMsg::Action(ActionMsg::CreateCategoryAndMoveNote {
            name: "  Client work  ".to_owned(),
            note_id,
            source_category_id,
        }),
    );

    assert_eq!(
        effects,
        vec![Effect::CreateCategoryAndMoveNote {
            action,
            name: "Client work".to_owned(),
            note_id,
        }]
    );
    let effects = update(
        &mut model,
        AppMsg::Library(LibraryReply::ActionFinished {
            action,
            result: Ok(()),
        }),
    );
    assert_eq!(
        model.undo_move,
        Some(super::MoveUndo {
            note_id,
            source_category_id,
        })
    );
    assert!(matches!(
        effects.as_slice(),
        [
            Effect::LoadSidebar { .. },
            Effect::LoadBrowser { .. },
            Effect::LoadTrash { .. },
            Effect::LoadLibraryRevision { .. }
        ]
    ));
}

#[test]
fn failed_move_undo_should_preserve_the_retryable_move_state() {
    let mut model = AppModel::new(&Config::default());
    let undo_move = super::MoveUndo {
        note_id: NoteId::new(),
        source_category_id: CategoryId::new(),
    };
    model.undo_move = Some(undo_move);

    let effects = update(&mut model, AppMsg::Action(ActionMsg::UndoMove));
    assert_eq!(
        effects,
        vec![Effect::MoveNote {
            action: ActionKey::UndoMove(undo_move.note_id),
            note_id: undo_move.note_id,
            category_id: undo_move.source_category_id,
        }]
    );
    let effects = update(
        &mut model,
        AppMsg::Library(LibraryReply::ActionFinished {
            action: ActionKey::UndoMove(undo_move.note_id),
            result: Err(UiError::new("undo failed")),
        }),
    );

    assert!(effects.is_empty());
    assert_eq!(model.undo_move, Some(undo_move));
    assert_eq!(model.notice, Some(UiError::new("undo failed")));
}
