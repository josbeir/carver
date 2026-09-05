//! Application bootstrap and top-level window composition.

use std::path::Path;

use adw::prelude::*;
use carver_config::{AppPaths, Config, load, save};
use carver_sdk::{InstalledLibraryClient, open_installed_library};
use gtk::prelude::*;
use libadwaita as adw;

use crate::{
    browser::build_content,
    dialogs::install_window_actions,
    editor::install_syntax_assets,
    mvu::{AppDispatcher, AppModel, AppMsg, AppRuntime, NavigationMsg, WindowMsg},
    sidebar::build_sidebar,
    view::ViewRefs,
};

pub(crate) const APPLICATION_ID: &str = "io.github.josbeir.Carver";
const APPLICATION_NAME: &str = "Carver";
pub(crate) const APPLICATION_ICON: &str = "io.github.josbeir.Carver";

type AppLibraryClient = InstalledLibraryClient;

/// Runs the Libadwaita application.
pub(crate) fn run() -> glib::ExitCode {
    glib::set_application_name(APPLICATION_NAME);
    if std::env::var_os("CARVER_DISABLE_PORTALS").is_some() {
        // CONTEXT: the black-box headless test has no desktop portal services.
        gtk::disable_portals();
    }
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
    let source_syntax_dir = match install_syntax_assets(&paths.data_dir) {
        Ok(directory) => directory,
        Err(error) => {
            show_startup_error(application, &error.to_string());
            return;
        }
    };
    let client = match open_installed_library() {
        Ok(client) => client,
        Err(error) => {
            show_startup_error(application, &error.to_string());
            return;
        }
    };
    if let Err(error) = build_window(
        application,
        client,
        &config,
        Some(paths.assets_dir()).as_deref(),
        &source_syntax_dir,
        Some(config_path).as_deref(),
        Some(paths.database_file()).as_deref(),
    ) {
        show_startup_error(application, &error.to_string());
    }
}

pub(crate) fn load_styles() {
    let resource = gtk::gio::Resource::from_data(&glib::Bytes::from_static(include_bytes!(
        concat!(env!("OUT_DIR"), "/carver-agent-icons.gresource")
    )));
    if let Ok(resource) = resource {
        gtk::gio::resources_register(&resource);
        if let Some(display) = gtk::gdk::Display::default() {
            let icon_theme = gtk::IconTheme::for_display(&display);
            icon_theme.add_resource_path("/io/github/josbeir/Carver/icons");
        }
    }
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

fn build_window(
    application: &adw::Application,
    client: AppLibraryClient,
    config: &Config,
    assets_dir: Option<&Path>,
    source_syntax_dir: &Path,
    config_path: Option<&Path>,
    database_path: Option<&Path>,
) -> Result<adw::ApplicationWindow, crate::editor::SourceSyntaxError> {
    let window = adw::ApplicationWindow::new(application);
    window.set_title(Some("Carver"));
    window.set_icon_name(Some(APPLICATION_ICON));
    window.set_default_size(config.window.width, config.window.height);
    window.set_maximized(config.window.maximized);
    let dispatcher = AppDispatcher::default();
    let toast_overlay = adw::ToastOverlay::new();
    let split_view = adw::NavigationSplitView::new();
    split_view.set_show_content(true);
    split_view.set_collapsed(config.window.sidebar_collapsed);
    let sidebar = build_sidebar(&dispatcher, &split_view);
    let content = build_content(
        &dispatcher,
        config,
        assets_dir,
        source_syntax_dir,
        &split_view,
        &toast_overlay,
    )?;
    let sidebar_page = adw::NavigationPage::new(&sidebar.widget, "Categories");
    let content_page = adw::NavigationPage::new(&content.widget, "Notes");
    content_page.set_can_pop(false);
    split_view.set_sidebar(Some(&sidebar_page));
    split_view.set_content(Some(&content_page));
    toast_overlay.set_child(Some(&split_view));
    window.set_content(Some(&toast_overlay));

    let sidebar_for_render = sidebar.clone();
    let view = ViewRefs::new(
        content.route_stack,
        content.browser.status.clone(),
        content.trash.status.clone(),
    )
    .with_browser(
        content.browser.list,
        content.browser.pages,
        content.browser.search_empty_card,
        content.browser.empty_new_note_button,
        content.browser.category_hero,
    )
    .with_sidebar_renderer(move |model| sidebar_for_render.render(model))
    .with_editor(content.editor)
    .with_trash(
        content.trash.list,
        content.trash.pages,
        content.trash.empty_button,
    )
    .with_toast_overlay(toast_overlay)
    .with_dispatcher(dispatcher.clone());
    let runtime = AppRuntime::new_with_config_path(
        client,
        AppModel::new(config),
        view,
        config_path.map(Path::to_path_buf),
    );
    runtime.bind_dispatcher(&dispatcher);
    if let Some(database_path) = database_path
        && let Err(error) = runtime.monitor_library(database_path, dispatcher.clone())
    {
        eprintln!("Carver could not monitor its shared library: {error}");
    }
    install_window_actions(&window, &dispatcher, &runtime);
    let dispatcher_for_close = dispatcher.clone();
    let runtime_for_close = runtime.clone();
    window.connect_close_request(move |window| {
        let _ = dispatcher_for_close.dispatch(AppMsg::Window(WindowMsg::SaveGeometry {
            width: window.default_width(),
            height: window.default_height(),
            maximized: window.is_maximized(),
        }));
        let _ = runtime_for_close.model();
        glib::Propagation::Proceed
    });
    let _ = dispatcher.dispatch(AppMsg::Navigation(NavigationMsg::Started));
    window.present();
    Ok(window)
}

#[cfg(test)]
pub(crate) fn build_window_for_test(
    application: &adw::Application,
    client: AppLibraryClient,
    config: &Config,
    config_path: &Path,
) -> Result<adw::ApplicationWindow, crate::editor::SourceSyntaxError> {
    load_styles();
    let data_dir = config_path.parent().unwrap_or_else(|| Path::new("."));
    let source_syntax_dir = install_syntax_assets(data_dir)?;
    build_window(
        application,
        client,
        config,
        None,
        &source_syntax_dir,
        Some(config_path),
        None,
    )
}
