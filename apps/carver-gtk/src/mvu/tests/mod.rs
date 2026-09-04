use carver_config::{AppPaths, Config};
use carver_sdk::LibraryClient;
use carver_sdk::{CategoryId, NoteId};
use carver_storage_sqlite::SqliteLibrary;
use gtk::prelude::*;

use super::{
    AppModel, AppMsg, AppRuntime, BrowserMsg, EditorMsg, Effect, LibraryReply, LoadState,
    NavigationMsg, RequestId, Route, SidebarMsg, TrashMsg, TrashMutation, UiError, update,
};

#[test]
fn startup_should_request_sidebar_and_browser_data() {
    let mut model = AppModel::new(&Config::default());

    let effects = update(&mut model, AppMsg::Navigation(NavigationMsg::Started));

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
        ]
    );
    assert_eq!(model.sidebar.state, LoadState::Loading(RequestId(1)));
    assert_eq!(model.browser.notes.state, LoadState::Loading(RequestId(2)));
}

#[test]
fn stale_browser_reply_should_not_replace_a_newer_request() {
    let mut model = AppModel::new(&Config::default());
    let first = update(&mut model, AppMsg::Browser(BrowserMsg::Reload));
    let timer = update(
        &mut model,
        AppMsg::Browser(BrowserMsg::SearchChanged("new query".to_owned())),
    );
    let first_request = match first.as_slice() {
        [Effect::LoadBrowser { request_id, .. }] => *request_id,
        _ => panic!("browser reload should produce one browser effect"),
    };
    let timer_id = match timer.as_slice() {
        [Effect::ScheduleSearch { timer_id }] => *timer_id,
        _ => panic!("search should schedule one timer"),
    };

    assert!(
        update(
            &mut model,
            AppMsg::Browser(BrowserMsg::SearchTimerFired(timer_id)),
        )
        .is_empty()
    );

    let follow_up = update(
        &mut model,
        AppMsg::Library(LibraryReply::BrowserLoaded {
            request_id: first_request,
            result: Ok(Vec::new()),
        }),
    );
    let second_request = match follow_up.as_slice() {
        [Effect::LoadBrowser { request_id, .. }] => *request_id,
        _ => panic!("a queued reload should start after the active request"),
    };
    assert_eq!(
        model.browser.notes.state,
        LoadState::Loading(second_request)
    );

    let _ = update(
        &mut model,
        AppMsg::Library(LibraryReply::BrowserLoaded {
            request_id: second_request,
            result: Err(UiError::new("search failed")),
        }),
    );
    assert_eq!(
        model.browser.notes.state,
        LoadState::Failed(UiError::new("search failed"))
    );
}

#[test]
fn selecting_a_category_should_reload_the_browser_for_that_category() {
    let mut model = AppModel::new(&Config::default());
    let category_id = CategoryId::new();

    let effects = update(
        &mut model,
        AppMsg::Navigation(NavigationMsg::SelectCategory(Some(category_id))),
    );

    assert_eq!(model.route, Route::Browser);
    assert_eq!(model.selected_category, Some(category_id));
    assert_eq!(
        effects,
        vec![Effect::LoadBrowser {
            request_id: RequestId(1),
            category_id: Some(category_id),
            query: String::new(),
        }]
    );
}

#[test]
fn stale_editor_close_should_not_close_a_newer_session() {
    let mut model = AppModel::new(&Config::default());
    let _ = update(
        &mut model,
        AppMsg::Editor(EditorMsg::Open(carver_sdk::NoteId::new())),
    );
    let Some(first_session) = model.editor_session else {
        panic!("editor should be open");
    };
    let _ = update(
        &mut model,
        AppMsg::Editor(EditorMsg::Open(carver_sdk::NoteId::new())),
    );

    let _ = update(&mut model, AppMsg::Editor(EditorMsg::Close(first_session)));

    assert_eq!(model.route, Route::Editor);
    assert_ne!(model.editor_session, Some(first_session));
}

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

pub(crate) fn runtime_should_render_and_complete_each_initial_resource()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary_directory = tempfile::tempdir()?;
    let paths = AppPaths {
        config_dir: temporary_directory.path().join("config"),
        data_dir: temporary_directory.path().join("data"),
        cache_dir: temporary_directory.path().join("cache"),
    };
    paths.ensure_exists()?;
    let client = LibraryClient::spawn(SqliteLibrary::open(
        &paths.database_file(),
        &paths.assets_dir(),
    )?)?;
    crate::app::ensure_first_category(&client);

    let stack = gtk::Stack::new();
    for name in ["browser", "editor", "trash"] {
        stack.add_named(&gtk::Box::new(gtk::Orientation::Vertical, 0), Some(name));
    }
    let sidebar_list = gtk::ListBox::new();
    let browser_list = gtk::ListBox::new();
    let browser_pages = gtk::Stack::new();
    let browser_status = libadwaita::StatusPage::new();
    browser_pages.add_named(
        &gtk::Box::new(gtk::Orientation::Vertical, 0),
        Some("contents"),
    );
    browser_pages.add_named(&browser_status, Some("empty"));
    let search_empty = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let empty_new_note = gtk::Button::new();
    let runtime = AppRuntime::new(
        client,
        AppModel::new(&Config::default()),
        crate::view::ViewRefs::new(stack, browser_status, libadwaita::StatusPage::new())
            .with_browser_and_sidebar(
                sidebar_list.clone(),
                browser_list,
                browser_pages,
                search_empty,
                empty_new_note,
                libadwaita::WindowTitle::new("Home", "All recent notes"),
            ),
    );

    runtime.dispatch(AppMsg::Navigation(NavigationMsg::Started));
    assert!(crate::tests::support::run_main_context_until(|| {
        matches!(runtime.model().sidebar.state, LoadState::Ready(_))
            && matches!(runtime.model().browser.notes.state, LoadState::Ready(_))
    }));
    assert!(sidebar_list.first_child().is_some());

    runtime.dispatch(AppMsg::Browser(BrowserMsg::SearchChanged(
        "needle".to_owned(),
    )));
    assert!(crate::tests::support::run_main_context_until(|| {
        matches!(runtime.model().browser.notes.state, LoadState::Ready(_))
    }));

    runtime.dispatch(AppMsg::Navigation(NavigationMsg::ShowTrash));
    assert!(crate::tests::support::run_main_context_until(|| {
        matches!(runtime.model().trash.state, LoadState::Ready(_))
    }));

    runtime.dispatch(AppMsg::Sidebar(SidebarMsg::Reload));
    let LoadState::Loading(request_id) = runtime.model().sidebar.state else {
        return Err("sidebar should be loading".into());
    };
    runtime.dispatch(AppMsg::Library(LibraryReply::SidebarLoaded {
        request_id,
        result: Err(UiError::new("offline")),
    }));
    assert_eq!(
        runtime.model().sidebar.state,
        LoadState::Failed(UiError::new("offline"))
    );
    Ok(())
}
