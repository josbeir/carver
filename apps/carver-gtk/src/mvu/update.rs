//! Pure state transitions for the application model.

use super::{
    ActionKey, ActionMsg, AppModel, AppMsg, BrowserMsg, EditorMsg, EditorSaveRequest, Effect,
    LibraryReply, MoveUndo, NavigationMsg, PreferencesMsg, SidebarMsg, TrashMsg, UiError,
};

/// Applies one message and returns the work a runtime must perform afterwards.
#[must_use]
pub fn update(model: &mut AppModel, message: AppMsg) -> Vec<Effect> {
    match message {
        AppMsg::Navigation(NavigationMsg::Started) => {
            [reload_sidebar(model), reload_browser(model)]
                .into_iter()
                .flatten()
                .collect()
        }
        AppMsg::Navigation(NavigationMsg::SelectCategory(category_id)) => {
            model.route = super::Route::Browser;
            model.selected_category = category_id;
            reload_browser(model).into_iter().collect()
        }
        AppMsg::Navigation(NavigationMsg::OpenNote(note_id)) => {
            let request_id = model.next_request_id();
            model.editor_load_request = Some(request_id);
            vec![Effect::LoadEditorNote {
                request_id,
                note_id,
            }]
        }
        AppMsg::Navigation(NavigationMsg::ShowTrash) => {
            model.route = super::Route::Trash;
            reload_trash(model).into_iter().collect()
        }
        AppMsg::Navigation(NavigationMsg::ShowBrowser) => {
            model.route = super::Route::Browser;
            Vec::new()
        }
        AppMsg::Browser(BrowserMsg::Reload) => reload_browser(model).into_iter().collect(),
        AppMsg::Browser(BrowserMsg::SearchTimerFired(timer_id))
            if model.browser.search_timer == Some(timer_id) =>
        {
            model.browser.search_timer = None;
            reload_browser(model).into_iter().collect()
        }
        AppMsg::Browser(BrowserMsg::SearchChanged(query)) => {
            model.browser.search_query = query;
            let timer_id = model.next_timer_id();
            model.browser.search_timer = Some(timer_id);
            vec![Effect::ScheduleSearch { timer_id }]
        }
        AppMsg::Sidebar(SidebarMsg::Reload) => reload_sidebar(model).into_iter().collect(),
        AppMsg::Trash(TrashMsg::Reload) => reload_trash(model).into_iter().collect(),
        AppMsg::Trash(TrashMsg::RestoreCategory(category_id)) => {
            vec![Effect::RestoreCategory { category_id }]
        }
        AppMsg::Trash(TrashMsg::RestoreNote(note_id)) => vec![Effect::RestoreNote { note_id }],
        AppMsg::Trash(TrashMsg::Empty) => vec![Effect::EmptyTrash],
        AppMsg::Editor(message) => update_editor(model, message),
        AppMsg::Browser(BrowserMsg::SearchTimerFired(_)) => Vec::new(),
        AppMsg::Preferences(PreferencesMsg::SetRemoteImages(enabled)) => {
            model.preferences.load_remote_images = enabled;
            Vec::new()
        }
        AppMsg::Preferences(PreferencesMsg::SetEditorMode(mode)) => {
            model.preferences.editor_mode = mode;
            if let Some(document) = model.editor.as_mut() {
                document.mode = mode;
            }
            Vec::new()
        }
        AppMsg::Preferences(PreferencesMsg::SetSourceSplitView(visible)) => {
            model.preferences.source_split_view = visible;
            Vec::new()
        }
        AppMsg::Action(action) => update_action(model, action),
        AppMsg::Library(reply) => update_library(model, reply),
    }
}

fn update_editor(model: &mut AppModel, message: EditorMsg) -> Vec<Effect> {
    match message {
        EditorMsg::Load {
            note_id,
            revision,
            source,
        } => {
            model.route = super::Route::Editor;
            if model
                .editor
                .as_ref()
                .is_some_and(|document| document.note_id == note_id)
            {
                return Vec::new();
            }
            let session = model.next_editor_session_id();
            model.editor = Some(super::EditorDocument::new(
                session,
                note_id,
                revision,
                source,
                model.preferences.editor_mode,
            ));
            Vec::new()
        }
        EditorMsg::SourceChanged(source) => {
            let changed = model
                .editor
                .as_mut()
                .is_some_and(|document| document.source_changed(source));
            if changed {
                model.notice = None;
            }
            Vec::new()
        }
        EditorMsg::AutosaveRequested => schedule_editor_save(model).into_iter().collect(),
        EditorMsg::AutosaveElapsed { session, timer_id } => model
            .editor
            .as_mut()
            .filter(|document| document.session == session && document.is_current_timer(timer_id))
            .and_then(super::EditorDocument::begin_save)
            .map_or_else(Vec::new, save_note_effect),
        EditorMsg::RetrySave => model
            .editor
            .as_mut()
            .and_then(super::EditorDocument::begin_save)
            .map_or_else(Vec::new, save_note_effect),
        EditorMsg::BackRequested => request_editor_close(model),
        EditorMsg::Close(session_id)
            if model
                .editor
                .as_ref()
                .is_some_and(|document| document.session == session_id) =>
        {
            model.route = super::Route::Browser;
            model.editor = None;
            model.editor_load_request = None;
            Vec::new()
        }
        EditorMsg::Close(_) => Vec::new(),
    }
}

fn update_action(model: &mut AppModel, action: ActionMsg) -> Vec<Effect> {
    if matches!(action, ActionMsg::UndoMove) {
        return update_undo_move(model);
    }
    let Some(key) = action.key() else {
        return Vec::new();
    };
    if !model.begin_action(key) {
        return Vec::new();
    }
    let effect = match action {
        ActionMsg::CreateCategory(name) => {
            category_name_effect(&name, |name| Effect::CreateCategory { name })
        }
        ActionMsg::RenameCategory { category_id, name } => {
            category_name_effect(&name, |name| Effect::RenameCategory { category_id, name })
        }
        ActionMsg::TrashCategory(category_id) => Some(Effect::TrashCategory { category_id }),
        ActionMsg::MoveNote {
            note_id,
            source_category_id: _,
            category_id,
        } => Some(Effect::MoveNote {
            action: key,
            note_id,
            category_id,
        }),
        ActionMsg::TrashNote(note_id) => Some(Effect::TrashNote { note_id }),
        ActionMsg::UndoMove => None,
    };
    if let Some(effect) = effect {
        vec![effect]
    } else {
        model.finish_action(key);
        model.notice = Some(UiError::new("Category names cannot be empty."));
        Vec::new()
    }
}

fn update_undo_move(model: &mut AppModel) -> Vec<Effect> {
    let Some(MoveUndo {
        note_id,
        source_category_id,
    }) = model.undo_move
    else {
        return Vec::new();
    };
    let action = ActionKey::UndoMove(note_id);
    if !model.begin_action(action) {
        return Vec::new();
    }
    vec![Effect::MoveNote {
        action,
        note_id,
        category_id: source_category_id,
    }]
}

fn category_name_effect(name: &str, effect: impl FnOnce(String) -> Effect) -> Option<Effect> {
    let name = name.trim().to_owned();
    (!name.is_empty()).then(|| effect(name))
}

fn update_library(model: &mut AppModel, reply: LibraryReply) -> Vec<Effect> {
    match reply {
        LibraryReply::ActionFinished { action, result } => {
            model.finish_action(action);
            match result {
                Ok(()) => {
                    update_undo_state(model, action);
                    model.notice = None;
                    reload_all_resources(model)
                }
                Err(error) => {
                    model.notice = Some(error);
                    Vec::new()
                }
            }
        }
        LibraryReply::SidebarLoaded { request_id, result } => {
            reload_sidebar_after(model.sidebar.finish(request_id, result), model)
        }
        LibraryReply::BrowserLoaded { request_id, result } => {
            reload_browser_after(model.browser.notes.finish(request_id, result), model)
        }
        LibraryReply::EditorLoaded { request_id, result } => {
            if model.editor_load_request != Some(request_id) {
                return Vec::new();
            }
            model.editor_load_request = None;
            match result {
                Ok(note) => {
                    let session = model.next_editor_session_id();
                    model.editor = Some(super::EditorDocument::new(
                        session,
                        note.id,
                        note.revision,
                        note.source,
                        model.preferences.editor_mode,
                    ));
                    model.route = super::Route::Editor;
                }
                Err(error) => model.notice = Some(error),
            }
            Vec::new()
        }
        LibraryReply::TrashLoaded { request_id, result } => {
            reload_trash_after(model.trash.finish(request_id, result), model)
        }
        LibraryReply::TrashMutationFinished { result } => match result {
            Ok(_) => {
                model.notice = None;
                reload_all_resources(model)
            }
            Err(error) => {
                model.notice = Some(error);
                Vec::new()
            }
        },
        LibraryReply::EditorSaved { request, result } => {
            update_editor_save(model, &request, result)
        }
    }
}

fn schedule_editor_save(model: &mut AppModel) -> Option<Effect> {
    let timer_id = model.next_timer_id();
    let delay_ms = model.preferences.autosave_delay_ms;
    let document = model.editor.as_mut()?;
    if matches!(document.save_state, super::EditorSaveState::Saving(_)) {
        return None;
    }
    document.schedule_save(timer_id);
    Some(Effect::ScheduleEditorSave {
        session: document.session,
        timer_id,
        delay_ms,
    })
}

fn save_note_effect(request: EditorSaveRequest) -> Vec<Effect> {
    vec![Effect::SaveNote { request }]
}

fn update_editor_save(
    model: &mut AppModel,
    request: &EditorSaveRequest,
    result: Result<carver_sdk::Revision, UiError>,
) -> Vec<Effect> {
    let close_after_save = {
        let Some(document) = model.editor.as_mut() else {
            return Vec::new();
        };
        if document.session != request.session
            || document.note_id != request.note_id
            || document.revision != request.expected_revision
            || document.save_state != super::EditorSaveState::Saving(request.clone())
        {
            return Vec::new();
        }
        match result {
            Ok(revision) => {
                document.revision = revision;
                if document.source == request.source {
                    document.save_state = super::EditorSaveState::Clean;
                    document.closes_after_save()
                } else {
                    document.save_state = super::EditorSaveState::Dirty;
                    return document
                        .begin_save()
                        .map_or_else(Vec::new, save_note_effect);
                }
            }
            Err(error) if document.source == request.source => {
                document.save_state = super::EditorSaveState::Failed(error);
                return Vec::new();
            }
            Err(_) => {
                document.save_state = super::EditorSaveState::Dirty;
                return document
                    .begin_save()
                    .map_or_else(Vec::new, save_note_effect);
            }
        }
    };
    if close_after_save {
        model.route = super::Route::Browser;
        model.editor = None;
    }
    reload_browser(model).into_iter().collect()
}

fn request_editor_close(model: &mut AppModel) -> Vec<Effect> {
    let Some(document) = model.editor.as_mut() else {
        return Vec::new();
    };
    document.request_close();
    if matches!(&document.save_state, super::EditorSaveState::Clean) {
        model.route = super::Route::Browser;
        model.editor = None;
        return Vec::new();
    }
    document
        .begin_save()
        .map_or_else(Vec::new, save_note_effect)
}

fn update_undo_state(model: &mut AppModel, action: ActionKey) {
    match action {
        ActionKey::MoveNote {
            note_id,
            source_category_id,
        } => {
            model.undo_move = Some(MoveUndo {
                note_id,
                source_category_id,
            });
        }
        ActionKey::UndoMove(_) => model.undo_move = None,
        ActionKey::CreateCategory
        | ActionKey::RenameCategory(_)
        | ActionKey::TrashCategory(_)
        | ActionKey::TrashNote(_) => {}
    }
}

fn reload_sidebar_after(reload: bool, model: &mut AppModel) -> Vec<Effect> {
    reload
        .then(|| reload_sidebar(model))
        .flatten()
        .into_iter()
        .collect()
}

fn reload_browser_after(reload: bool, model: &mut AppModel) -> Vec<Effect> {
    reload
        .then(|| reload_browser(model))
        .flatten()
        .into_iter()
        .collect()
}

fn reload_trash_after(reload: bool, model: &mut AppModel) -> Vec<Effect> {
    reload
        .then(|| reload_trash(model))
        .flatten()
        .into_iter()
        .collect()
}

fn reload_all_resources(model: &mut AppModel) -> Vec<Effect> {
    [
        reload_sidebar(model),
        reload_browser(model),
        reload_trash(model),
    ]
    .into_iter()
    .flatten()
    .collect()
}

fn reload_sidebar(model: &mut AppModel) -> Option<Effect> {
    let request_id = model.next_request_id();
    model
        .sidebar
        .begin_reload(request_id)
        .then_some(Effect::LoadSidebar { request_id })
}

fn reload_browser(model: &mut AppModel) -> Option<Effect> {
    let request_id = model.next_request_id();
    model
        .browser
        .notes
        .begin_reload(request_id)
        .then_some(Effect::LoadBrowser {
            request_id,
            category_id: model.selected_category,
            query: model.browser.search_query.clone(),
        })
}

fn reload_trash(model: &mut AppModel) -> Option<Effect> {
    let request_id = model.next_request_id();
    model
        .trash
        .begin_reload(request_id)
        .then_some(Effect::LoadTrash { request_id })
}
