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
    sidebar::build_sidebar,
};

const APPLICATION_ID: &str = "io.github.josbeir.Carver.Devel";

/// Runs the Libadwaita application.
pub(crate) fn run() -> glib::ExitCode {
    let application =
        adw::Application::new(Some(APPLICATION_ID), gtk::gio::ApplicationFlags::empty());
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
    let state = Rc::new(AppState::new(client, config));
    build_window(application, &state, config_path);
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

fn build_window(application: &adw::Application, state: &Rc<AppState>, config_path: PathBuf) {
    let window = adw::ApplicationWindow::new(application);
    window.set_title(Some("Carver"));
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
    split_view.set_sidebar(Some(&sidebar_page));
    split_view.set_content(Some(&content_page));

    toast_overlay.set_child(Some(&split_view));
    window.set_content(Some(&toast_overlay));
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
}
