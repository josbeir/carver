//! GNOME dialogs and window-scoped actions.

use adw::prelude::*;
use carver_sdk::{CategoryId, NoteId};
use gtk::prelude::*;
use libadwaita as adw;

use crate::mvu::{
    ActionMsg, AppDispatcher, AppMsg, AppRuntime, EditorMsg, PreferencesMsg, TrashMsg,
};
use carver_storage_sqlite::SqliteLibrary;

/// Installs actions exposed from the application menu.
pub(crate) fn install_window_actions(
    window: &adw::ApplicationWindow,
    dispatcher: &AppDispatcher,
    runtime: &AppRuntime<SqliteLibrary>,
) {
    let preferences = gtk::gio::SimpleAction::new("preferences", None);
    let dispatcher_for_preferences = dispatcher.clone();
    let config_for_preferences = runtime.model().config;
    let window_for_preferences = window.clone();
    preferences.connect_activate(move |_, _| {
        show_preferences_dialog(
            &window_for_preferences,
            &dispatcher_for_preferences,
            &config_for_preferences,
        );
    });
    window.add_action(&preferences);

    let about = gtk::gio::SimpleAction::new("about", None);
    let window_for_about = window.clone();
    about.connect_activate(move |_, _| show_about_window(&window_for_about));
    window.add_action(&about);

    install_trash_actions(window, dispatcher);
    install_mvu_actions(window, dispatcher, runtime);
}

fn install_mvu_actions(
    window: &impl IsA<gtk::Widget>,
    dispatcher: &AppDispatcher,
    runtime: &AppRuntime<SqliteLibrary>,
) {
    let actions = gtk::gio::SimpleActionGroup::new();
    let undo_move = gtk::gio::SimpleAction::new("undo-move", None);
    let dispatcher_for_undo = dispatcher.clone();
    undo_move.connect_activate(move |_, _| {
        let _ = dispatcher_for_undo.dispatch(AppMsg::Action(ActionMsg::UndoMove));
    });
    actions.add_action(&undo_move);
    let undo_trash_note = gtk::gio::SimpleAction::new("undo-trash-note", None);
    let dispatcher_for_trash_undo = dispatcher.clone();
    let runtime_for_trash_undo = runtime.clone();
    undo_trash_note.connect_activate(move |_, _| {
        let Some(note_id) = runtime_for_trash_undo.model().undo_trash_note else {
            return;
        };
        let _ = dispatcher_for_trash_undo.dispatch(AppMsg::Trash(TrashMsg::RestoreNote(note_id)));
    });
    actions.add_action(&undo_trash_note);
    let retry_save = gtk::gio::SimpleAction::new("retry-save", None);
    let dispatcher_for_retry = dispatcher.clone();
    retry_save.connect_activate(move |_, _| {
        let _ = dispatcher_for_retry.dispatch(AppMsg::Editor(EditorMsg::RetrySave));
    });
    actions.add_action(&retry_save);
    window.insert_action_group("mvu", Some(&actions));
}

fn install_trash_actions(window: &impl IsA<gtk::Widget>, dispatcher: &AppDispatcher) {
    let actions = gtk::gio::SimpleActionGroup::new();
    let restore_category =
        gtk::gio::SimpleAction::new("restore-category", Some(&String::static_variant_type()));
    let dispatcher_for_category = dispatcher.clone();
    restore_category.connect_activate(move |_, parameter| {
        let Some(category_id) = parameter
            .and_then(gtk::glib::Variant::get::<String>)
            .and_then(|value| uuid::Uuid::parse_str(&value).ok())
            .map(CategoryId::from_uuid)
        else {
            return;
        };
        let _ =
            dispatcher_for_category.dispatch(AppMsg::Trash(TrashMsg::RestoreCategory(category_id)));
    });
    actions.add_action(&restore_category);

    let restore_note =
        gtk::gio::SimpleAction::new("restore-note", Some(&String::static_variant_type()));
    let dispatcher_for_note = dispatcher.clone();
    restore_note.connect_activate(move |_, parameter| {
        let Some(note_id) = parameter
            .and_then(gtk::glib::Variant::get::<String>)
            .and_then(|value| uuid::Uuid::parse_str(&value).ok())
            .map(NoteId::from_uuid)
        else {
            return;
        };
        let _ = dispatcher_for_note.dispatch(AppMsg::Trash(TrashMsg::RestoreNote(note_id)));
    });
    actions.add_action(&restore_note);
    window.insert_action_group("trash", Some(&actions));
}

fn show_preferences_dialog(
    parent: &adw::ApplicationWindow,
    dispatcher: &AppDispatcher,
    config: &carver_config::Config,
) {
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

    let dispatcher_for_images = dispatcher.clone();
    remote_images.connect_active_notify(move |remote_images| {
        let _ = dispatcher_for_images.dispatch(AppMsg::Preferences(
            PreferencesMsg::SetRemoteImages(remote_images.is_active()),
        ));
    });
    let dispatcher_for_delay = dispatcher.clone();
    autosave.connect_value_changed(move |autosave| {
        let delay_ms = u64::try_from(autosave.value_as_int()).unwrap_or(100);
        let _ = dispatcher_for_delay.dispatch(AppMsg::Preferences(
            PreferencesMsg::SetAutosaveDelay(delay_ms),
        ));
    });
    dialog.present(Some(parent));
}

#[cfg(test)]
pub(crate) fn present_dialogs_for_test(
    parent: &adw::ApplicationWindow,
    config: &carver_config::Config,
    dispatcher: &AppDispatcher,
) {
    show_preferences_dialog(parent, dispatcher, config);
    show_about_window(parent);
    show_category_name_dialog(Some(parent.upcast_ref()), "New Category", "", |_| {});
    show_category_trash_confirmation(Some(parent.upcast_ref()), "Category", || {});
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
    entry.set_activates_default(true);
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

/// Presents a destructive confirmation before a category is moved to Trash.
pub(crate) fn show_category_trash_confirmation(
    parent: Option<&gtk::Window>,
    category_name: &str,
    on_confirm: impl Fn() + 'static,
) {
    let dialog = category_trash_dialog(category_name, on_confirm);
    dialog.present(parent);
}

/// Builds the category-trash confirmation so its destructive behavior is testable.
pub(crate) fn category_trash_dialog(
    category_name: &str,
    on_confirm: impl Fn() + 'static,
) -> adw::AlertDialog {
    let dialog = adw::AlertDialog::new(
        Some(&format!("Move “{category_name}” to Trash?")),
        Some(
            "Notes in this category will no longer appear in your library. You can restore the category from Trash later.",
        ),
    );
    dialog.set_widget_name("category-trash-confirmation");
    dialog.add_responses(&[("cancel", "Cancel"), ("trash", "Move to Trash")]);
    dialog.set_response_appearance("trash", adw::ResponseAppearance::Destructive);
    dialog.set_default_response(Some("cancel"));
    dialog.set_close_response("cancel");
    dialog.connect_response(None, move |_dialog, response| {
        if response == "trash" {
            on_confirm();
        }
    });
    dialog
}
