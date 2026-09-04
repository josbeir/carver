//! Application bootstrap and top-level window composition.

use std::{path::PathBuf, rc::Rc};

use adw::prelude::*;
use carver_config::{AppPaths, load, save};
use carver_storage_sqlite::SqliteLibrary;
use gtk::prelude::*;
use libadwaita as adw;

use crate::{
    browser::build_content,
    controller::{AppLibraryClient, AppState},
    dialogs::{install_window_actions, persist_window_config},
    mvu::{AppModel, AppMsg, AppRuntime, NavigationMsg},
    sidebar::{build_sidebar, render_mvu_sidebar},
    view::ViewRefs,
};

pub(crate) const APPLICATION_ID: &str = "io.github.josbeir.Carver";
const APPLICATION_NAME: &str = "Carver";
pub(crate) const APPLICATION_ICON: &str = "io.github.josbeir.Carver";

/// Runs the Libadwaita application.
pub(crate) fn run() -> glib::ExitCode {
    glib::set_application_name(APPLICATION_NAME);
    let application_id =
        std::env::var("CARVER_APPLICATION_ID").unwrap_or_else(|_| APPLICATION_ID.to_owned());
    let application =
        adw::Application::new(Some(&application_id), gtk::gio::ApplicationFlags::empty());
    application.connect_activate(build_application);
    application.run()
}

fn build_application(application: &adw::Application) {
    load_styles();
    let paths = AppPaths::discover();
    let config_path = paths.config_file();
    let config = load(&config_path).unwrap_or_default();
    let _ = save(&config_path, &config);
    let client = match open_library(&paths) {
        Ok(client) => client,
        Err(error) => {
            show_startup_error(application, &error);
            return;
        }
    };

    ensure_first_category(&client);
    let state = Rc::new(AppState::new_with_assets(
        client,
        config,
        Some(paths.assets_dir()),
        Some(config_path.clone()),
    ));
    build_window(application, &state, config_path.clone());
}

fn load_styles() {
    let provider = gtk::CssProvider::new();
    provider.load_from_string(include_str!("style.css"));
    if let Some(display) = gtk::gdk::Display::default() {
        gtk::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }
}

fn show_startup_error(application: &adw::Application, error: &str) {
    let window = adw::ApplicationWindow::new(application);
    window.set_title(Some("Carver"));
    window.set_icon_name(Some(APPLICATION_ICON));
    let status = adw::StatusPage::builder()
        .title("Carver could not open its library")
        .description(error)
        .icon_name("dialog-error-symbolic")
        .build();
    window.set_content(Some(&status));
    window.present();
}

/// Ensures a newly created library has a default category.
pub(crate) fn ensure_first_category(client: &AppLibraryClient) {
    if let Ok(categories) = client.categories()
        && categories.is_empty()
    {
        let _ = client.create_category("Notes");
    }
}

fn open_library(paths: &AppPaths) -> Result<AppLibraryClient, String> {
    paths.ensure_exists().map_err(|error| error.to_string())?;
    let storage = SqliteLibrary::open(&paths.database_file(), &paths.assets_dir())
        .map_err(|error| error.to_string())?;
    AppLibraryClient::spawn(storage).map_err(|error| error.to_string())
}

fn build_window(
    application: &adw::Application,
    state: &Rc<AppState>,
    config_path: PathBuf,
) -> adw::ApplicationWindow {
    let window = adw::ApplicationWindow::new(application);
    window.set_title(Some("Carver"));
    window.set_icon_name(Some(APPLICATION_ICON));
    let window_config = state.config.borrow().window.clone();
    window.set_default_size(window_config.width, window_config.height);
    window.set_maximized(window_config.maximized);
    install_window_actions(&window, state, &config_path);

    let toast_overlay = adw::ToastOverlay::new();
    let split_view = adw::NavigationSplitView::new();
    split_view.set_show_content(true);
    split_view.set_collapsed(window_config.sidebar_collapsed);

    let sidebar = build_sidebar(state, &split_view);
    let content = build_content(state, &split_view, &toast_overlay);
    let sidebar_page = adw::NavigationPage::new(&sidebar, "Categories");
    let content_page = adw::NavigationPage::new(&content, "Notes");
    // The content page has an explicit sidebar control in every view. Avoid
    // NavigationSplitView adding a visually identical back affordance.
    content_page.set_can_pop(false);
    split_view.set_sidebar(Some(&sidebar_page));
    split_view.set_content(Some(&content_page));

    toast_overlay.set_child(Some(&split_view));
    window.set_content(Some(&toast_overlay));
    install_mvu_runtime(state);
    let state_for_close = Rc::clone(state);
    window.connect_close_request(move |window| {
        let _ = persist_window_config(
            &state_for_close,
            &config_path,
            window.default_width(),
            window.default_height(),
            window.is_maximized(),
        );
        glib::Propagation::Proceed
    });
    window.present();
    window
}

fn install_mvu_runtime(state: &Rc<AppState>) {
    let (
        Some(route_stack),
        Some(sidebar_list),
        Some(browser_list),
        Some(browser_pages),
        Some(browser_search_empty_card),
        Some(browser_empty_new_note_button),
        Some(browser_title),
        Some(browser_status),
        Some(trash_status),
        Some(toast_overlay),
    ) = (
        state.browser_stack.borrow().clone(),
        state.sidebar_list.borrow().clone(),
        state.browser_list.borrow().clone(),
        state.browser_content_stack.borrow().clone(),
        state.browser_search_empty_card.borrow().clone(),
        state.browser_empty_new_note_button.borrow().clone(),
        state.browser_title.borrow().clone(),
        state.browser_status.borrow().clone(),
        state.trash_status.borrow().clone(),
        state.browser_toast_overlay.borrow().clone(),
    )
    else {
        return;
    };
    let (Some(trash_list), Some(trash_pages), Some(empty_trash_button)) = (
        state.trash_list.borrow().clone(),
        state.trash_content_stack.borrow().clone(),
        state.empty_trash_button.borrow().clone(),
    ) else {
        return;
    };
    let state_for_sidebar_renderer = Rc::downgrade(state);
    let view = ViewRefs::new(route_stack, browser_status, trash_status)
        .with_browser_and_sidebar(
            sidebar_list,
            browser_list,
            browser_pages,
            browser_search_empty_card,
            browser_empty_new_note_button,
            browser_title,
        )
        .with_sidebar_renderer(move |model| {
            if let Some(state) = state_for_sidebar_renderer.upgrade() {
                render_mvu_sidebar(&state, model);
            }
        })
        .with_trash(trash_list, trash_pages, empty_trash_button)
        .with_toast_overlay(toast_overlay);
    let model = AppModel::new(&state.config.borrow());
    if state.install_mvu_runtime(AppRuntime::new(state.client.clone(), model, view)) {
        let _ = state.dispatch_mvu(AppMsg::Navigation(NavigationMsg::Started));
    }
}

#[cfg(test)]
pub(crate) fn install_mvu_runtime_for_test(state: &Rc<AppState>) {
    install_mvu_runtime(state);
}

/// Builds the complete application window inside the shared GTK integration scenario.
///
/// Keeping this test-only seam here lets the scenario exercise private startup wiring
/// on its one initialized GTK thread.
#[cfg(test)]
pub(crate) fn build_window_for_test(
    application: &adw::Application,
    state: &Rc<AppState>,
    config_path: PathBuf,
) -> adw::ApplicationWindow {
    load_styles();
    build_window(application, state, config_path)
}

#[cfg(test)]
pub(crate) fn open_library_for_test(paths: &AppPaths) -> Result<AppLibraryClient, String> {
    open_library(paths)
}
