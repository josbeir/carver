//! GNOME dialogs and window-scoped actions.

use std::{path::Path, rc::Rc};

use adw::prelude::*;
use carver_config::{ConfigError, save};
use gtk::prelude::*;
use libadwaita as adw;

use crate::controller::AppState;

/// Installs actions exposed from the application menu.
pub(crate) fn install_window_actions(
    window: &adw::ApplicationWindow,
    state: &Rc<AppState>,
    config_path: &Path,
) {
    let preferences = gtk::gio::SimpleAction::new("preferences", None);
    let state_for_preferences = Rc::clone(state);
    let path_for_preferences = config_path.to_owned();
    let window_for_preferences = window.clone();
    preferences.connect_activate(move |_, _| {
        show_preferences_dialog(
            &window_for_preferences,
            &state_for_preferences,
            &path_for_preferences,
        );
    });
    window.add_action(&preferences);

    let about = gtk::gio::SimpleAction::new("about", None);
    let window_for_about = window.clone();
    about.connect_activate(move |_, _| show_about_window(&window_for_about));
    window.add_action(&about);
}

fn show_preferences_dialog(
    parent: &adw::ApplicationWindow,
    state: &Rc<AppState>,
    config_path: &Path,
) {
    let config = state.config.borrow().clone();
    let dialog = adw::PreferencesDialog::new();
    dialog.set_search_enabled(false);
    let page = adw::PreferencesPage::new();
    page.set_title("Editor");
    let group = adw::PreferencesGroup::new();
    group.set_title("Editing");

    let remote_images = adw::SwitchRow::new();
    remote_images.set_title("Load remote images automatically");
    remote_images.set_active(config.images.load_remote_automatically);
    group.add(&remote_images);

    let autosave_row = adw::ActionRow::new();
    autosave_row.set_title("Autosave delay");
    autosave_row.set_subtitle("Milliseconds before changes are saved");
    let autosave = gtk::SpinButton::with_range(100.0, 10_000.0, 100.0);
    let initial_delay = u32::try_from(config.editor.autosave_delay_ms.clamp(100, 10_000))
        .map_or(10_000.0, f64::from);
    autosave.set_value(initial_delay);
    autosave_row.add_suffix(&autosave);
    group.add(&autosave_row);
    page.add(&group);
    dialog.add(&page);

    let state_for_images = Rc::clone(state);
    let path_for_images = config_path.to_owned();
    remote_images.connect_active_notify(move |remote_images| {
        let mut updated = state_for_images.config.borrow().clone();
        updated.images.load_remote_automatically = remote_images.is_active();
        if save(&path_for_images, &updated).is_ok() {
            state_for_images.config.replace(updated);
        }
    });
    let state_for_delay = Rc::clone(state);
    let path_for_delay = config_path.to_owned();
    autosave.connect_value_changed(move |autosave| {
        let mut updated = state_for_delay.config.borrow().clone();
        updated.editor.autosave_delay_ms = u64::try_from(autosave.value_as_int()).unwrap_or(100);
        if save(&path_for_delay, &updated).is_ok() {
            state_for_delay.config.replace(updated);
        }
    });
    dialog.present(Some(parent));
}

fn show_about_window(parent: &adw::ApplicationWindow) {
    let about = adw::AboutDialog::builder()
        .application_name("Carver")
        .version(env!("CARGO_PKG_VERSION"))
        .developer_name("Carver contributors")
        .comments("A native GNOME notebook for Carve markup.")
        .license_type(gtk::License::MitX11)
        .build();
    about.present(Some(parent));
}

/// Saves window geometry supplied by the close-request interaction.
pub(crate) fn persist_window_config(
    state: &AppState,
    config_path: &Path,
    width: i32,
    height: i32,
    maximized: bool,
) -> Result<(), ConfigError> {
    let mut config = state.config.borrow().clone();
    config.window.width = width;
    config.window.height = height;
    config.window.maximized = maximized;
    save(config_path, &config)
}

/// Presents a validated single-line category name dialog.
pub(crate) fn show_category_name_dialog(
    parent: Option<&gtk::Window>,
    title: &str,
    initial_name: &str,
    on_submit: impl Fn(String) + 'static,
) {
    let entry = gtk::Entry::new();
    entry.set_widget_name("category-name-entry");
    entry.set_text(initial_name);
    entry.set_placeholder_text(Some("Category name"));
    let dialog = adw::AlertDialog::builder()
        .heading(title)
        .extra_child(&entry)
        .default_response("save")
        .close_response("cancel")
        .build();
    dialog.add_responses(&[("cancel", "Cancel"), ("save", "Save")]);
    dialog.set_response_enabled("save", !initial_name.trim().is_empty());
    let dialog_for_entry = dialog.clone();
    entry.connect_changed(move |entry| {
        dialog_for_entry.set_response_enabled("save", !entry.text().trim().is_empty());
    });
    dialog.connect_response(None, move |_dialog, response| {
        if response == "save" {
            let name = entry.text().trim().to_owned();
            if !name.is_empty() {
                on_submit(name);
            }
        }
    });
    dialog.present(parent);
}
