//! GNOME dialogs and window-scoped actions.

use std::{path::Path, rc::Rc};

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
    let dialog = gtk::Dialog::builder()
        .modal(true)
        .title("Preferences")
        .default_width(420)
        .build();
    dialog.set_transient_for(Some(parent));
    dialog.add_button("Cancel", gtk::ResponseType::Cancel);
    dialog.add_button("Save", gtk::ResponseType::Accept);
    dialog.set_default_response(gtk::ResponseType::Accept);
    let content = dialog.content_area();
    content.set_spacing(12);
    content.set_margin_start(18);
    content.set_margin_end(18);
    content.set_margin_top(12);
    content.set_margin_bottom(12);

    let remote_images = gtk::CheckButton::with_label("Load remote images automatically");
    remote_images.set_active(config.images.load_remote_automatically);
    content.append(&remote_images);
    let autosave_row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    let autosave_label = gtk::Label::new(Some("Autosave delay (milliseconds)"));
    autosave_label.set_hexpand(true);
    autosave_label.set_xalign(0.0);
    let autosave = gtk::SpinButton::with_range(100.0, 10_000.0, 100.0);
    let initial_delay = u32::try_from(config.editor.autosave_delay_ms.clamp(100, 10_000))
        .map_or(10_000.0, f64::from);
    autosave.set_value(initial_delay);
    autosave_row.append(&autosave_label);
    autosave_row.append(&autosave);
    content.append(&autosave_row);

    let state_for_response = Rc::clone(state);
    let path_for_response = config_path.to_owned();
    dialog.connect_response(move |dialog, response| {
        if response == gtk::ResponseType::Accept {
            let mut config = state_for_response.config.borrow().clone();
            config.images.load_remote_automatically = remote_images.is_active();
            config.editor.autosave_delay_ms = u64::try_from(autosave.value_as_int()).unwrap_or(100);
            if save(&path_for_response, &config).is_ok() {
                state_for_response.config.replace(config);
            }
        }
        dialog.close();
    });
    dialog.present();
}

fn show_about_window(parent: &adw::ApplicationWindow) {
    let about = adw::AboutWindow::builder()
        .application_name("Carver")
        .version(env!("CARGO_PKG_VERSION"))
        .developer_name("Carver contributors")
        .comments("A native GNOME notebook for Carve markup.")
        .license_type(gtk::License::MitX11)
        .build();
    about.set_transient_for(Some(parent));
    about.present();
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
    let dialog = gtk::Dialog::builder()
        .modal(true)
        .title(title)
        .default_width(360)
        .build();
    if let Some(parent) = parent {
        dialog.set_transient_for(Some(parent));
    }
    dialog.add_button("Cancel", gtk::ResponseType::Cancel);
    let save_button = dialog.add_button("Save", gtk::ResponseType::Accept);
    dialog.set_default_response(gtk::ResponseType::Accept);
    let content = dialog.content_area();
    content.set_margin_start(18);
    content.set_margin_end(18);
    content.set_margin_top(12);
    content.set_margin_bottom(12);
    let entry = gtk::Entry::new();
    entry.set_widget_name("category-name-entry");
    entry.set_text(initial_name);
    entry.set_activates_default(true);
    entry.set_placeholder_text(Some("Category name"));
    save_button.set_sensitive(!initial_name.trim().is_empty());
    let save_for_entry = save_button.clone();
    entry.connect_changed(move |entry| {
        save_for_entry.set_sensitive(!entry.text().trim().is_empty());
    });
    content.append(&entry);
    dialog.connect_response(move |dialog, response| {
        if response == gtk::ResponseType::Accept {
            let name = entry.text().trim().to_owned();
            if !name.is_empty() {
                on_submit(name);
            }
        }
        dialog.close();
    });
    dialog.present();
}
