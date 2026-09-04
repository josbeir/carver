//! Pure state transitions for the application model.

use super::{
    AppModel, AppMsg, BrowserMsg, EditorMsg, Effect, LibraryReply, NavigationMsg, PreferencesMsg,
    SidebarMsg, TrashMsg,
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
        AppMsg::Editor(EditorMsg::Open(_)) => {
            model.route = super::Route::Editor;
            model.editor_session = Some(model.next_editor_session_id());
            Vec::new()
        }
        AppMsg::Editor(EditorMsg::Close(session_id))
            if model.editor_session == Some(session_id) =>
        {
            model.route = super::Route::Browser;
            model.editor_session = None;
            Vec::new()
        }
        AppMsg::Browser(BrowserMsg::SearchTimerFired(_)) | AppMsg::Editor(EditorMsg::Close(_)) => {
            Vec::new()
        }
        AppMsg::Preferences(PreferencesMsg::SetRemoteImages(enabled)) => {
            model.preferences.load_remote_images = enabled;
            Vec::new()
        }
        AppMsg::Preferences(PreferencesMsg::SetEditorMode(mode)) => {
            model.preferences.editor_mode = mode;
            Vec::new()
        }
        AppMsg::Preferences(PreferencesMsg::SetSourceSplitView(visible)) => {
            model.preferences.source_split_view = visible;
            Vec::new()
        }
        AppMsg::Library(reply) => update_library(model, reply),
    }
}

fn update_library(model: &mut AppModel, reply: LibraryReply) -> Vec<Effect> {
    match reply {
        LibraryReply::SidebarLoaded { request_id, result } => {
            reload_sidebar_after(model.sidebar.finish(request_id, result), model)
        }
        LibraryReply::BrowserLoaded { request_id, result } => {
            reload_browser_after(model.browser.notes.finish(request_id, result), model)
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
