//! Pure state transitions for the application model.

use super::{
    AppModel, AppMsg, BrowserMsg, EditorMsg, Effect, LibraryReply, LoadState, NavigationMsg,
    PreferencesMsg, RequestId, SidebarMsg, TrashMsg,
};

/// Applies one message and returns the work a runtime must perform afterwards.
#[must_use]
pub fn update(model: &mut AppModel, message: AppMsg) -> Vec<Effect> {
    match message {
        AppMsg::Navigation(NavigationMsg::Started) => {
            vec![reload_sidebar(model), reload_browser(model)]
        }
        AppMsg::Navigation(NavigationMsg::SelectCategory(category_id)) => {
            model.route = super::Route::Browser;
            model.selected_category = category_id;
            vec![reload_browser(model)]
        }
        AppMsg::Navigation(NavigationMsg::ShowTrash) => {
            model.route = super::Route::Trash;
            vec![reload_trash(model)]
        }
        AppMsg::Navigation(NavigationMsg::ShowBrowser) => {
            model.route = super::Route::Browser;
            Vec::new()
        }
        AppMsg::Browser(BrowserMsg::Reload | BrowserMsg::SearchTimerFired(_)) => {
            vec![reload_browser(model)]
        }
        AppMsg::Browser(BrowserMsg::SearchChanged(query)) => {
            model.browser.search_query = query;
            vec![reload_browser(model)]
        }
        AppMsg::Sidebar(SidebarMsg::Reload) => vec![reload_sidebar(model)],
        AppMsg::Trash(TrashMsg::Reload) => vec![reload_trash(model)],
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
        AppMsg::Editor(EditorMsg::Close(_)) => Vec::new(),
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
        AppMsg::Library(LibraryReply::SidebarLoaded { request_id, result }) => {
            replace_if_current(&mut model.sidebar, request_id, result);
            Vec::new()
        }
        AppMsg::Library(LibraryReply::BrowserLoaded { request_id, result }) => {
            replace_if_current(&mut model.browser.notes, request_id, result);
            Vec::new()
        }
        AppMsg::Library(LibraryReply::TrashLoaded { request_id, result }) => {
            replace_if_current(&mut model.trash, request_id, result);
            Vec::new()
        }
    }
}

fn reload_sidebar(model: &mut AppModel) -> Effect {
    let request_id = model.next_request_id();
    model.sidebar = LoadState::Loading(request_id);
    Effect::LoadSidebar { request_id }
}

fn reload_browser(model: &mut AppModel) -> Effect {
    let request_id = model.next_request_id();
    model.browser.notes = LoadState::Loading(request_id);
    Effect::LoadBrowser {
        request_id,
        category_id: model.selected_category,
        query: model.browser.search_query.clone(),
    }
}

fn reload_trash(model: &mut AppModel) -> Effect {
    let request_id = model.next_request_id();
    model.trash = LoadState::Loading(request_id);
    Effect::LoadTrash { request_id }
}

fn replace_if_current<T>(
    state: &mut LoadState<T>,
    request_id: RequestId,
    result: Result<T, super::UiError>,
) {
    if matches!(state, LoadState::Loading(current) if *current == request_id) {
        *state = match result {
            Ok(value) => LoadState::Ready(value),
            Err(error) => LoadState::Failed(error),
        };
    }
}
