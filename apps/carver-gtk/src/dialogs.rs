//! GNOME dialogs and window-scoped actions.

use adw::prelude::*;
use carver_sdk::{CategoryId, CategorySummary, NoteId};
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
    let runtime_for_preferences = runtime.clone();
    let window_for_preferences = window.clone();
    preferences.connect_activate(move |_, _| {
        let config = runtime_for_preferences.model().config;
        let _ = show_preferences_dialog(
            &window_for_preferences,
            &dispatcher_for_preferences,
            &config,
        );
    });
    window.add_action(&preferences);

    let about = gtk::gio::SimpleAction::new("about", None);
    let window_for_about = window.clone();
    about.connect_activate(move |_, _| {
        let _ = show_about_window(&window_for_about);
    });
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
) -> adw::PreferencesDialog {
    let dialog = adw::PreferencesDialog::new();
    dialog.set_search_enabled(false);
    let page = adw::PreferencesPage::new();
    page.set_title("Editor");
    let group = adw::PreferencesGroup::new();
    group.set_title("Editing");

    let remote_images = adw::SwitchRow::new();
    remote_images.set_widget_name("remote-images-setting");
    remote_images.set_title("Load remote images automatically");
    remote_images.set_subtitle("Download images referenced by notes when they are displayed.");
    remote_images.set_active(config.images.load_remote_automatically);
    group.add(&remote_images);

    let source_group = adw::PreferencesGroup::new();
    source_group.set_title("Source editor");
    let line_numbers = adw::SwitchRow::new();
    line_numbers.set_widget_name("source-line-numbers-setting");
    line_numbers.set_title("Show line numbers");
    line_numbers.set_subtitle("Show source line positions in the editor gutter.");
    line_numbers.set_active(config.editor.source_line_numbers);
    source_group.add(&line_numbers);
    let current_line = adw::SwitchRow::new();
    current_line.set_widget_name("source-current-line-setting");
    current_line.set_title("Highlight current line");
    current_line.set_subtitle("Shade the line containing the cursor in Source mode.");
    current_line.set_active(config.editor.source_highlight_current_line);
    source_group.add(&current_line);
    let syntax_highlighting = adw::SwitchRow::new();
    syntax_highlighting.set_widget_name("source-syntax-highlighting-setting");
    syntax_highlighting.set_title("Syntax highlighting");
    syntax_highlighting.set_subtitle("Colour Carve markup in Source mode.");
    syntax_highlighting.set_active(config.editor.source_syntax_highlighting);
    source_group.add(&syntax_highlighting);
    page.add(&group);
    page.add(&source_group);
    dialog.add(&page);

    let dispatcher_for_images = dispatcher.clone();
    remote_images.connect_active_notify(move |remote_images| {
        let _ = dispatcher_for_images.dispatch(AppMsg::Preferences(
            PreferencesMsg::SetRemoteImages(remote_images.is_active()),
        ));
    });
    let dispatcher_for_line_numbers = dispatcher.clone();
    line_numbers.connect_active_notify(move |line_numbers| {
        let _ = dispatcher_for_line_numbers.dispatch(AppMsg::Preferences(
            PreferencesMsg::SetSourceLineNumbers(line_numbers.is_active()),
        ));
    });
    let dispatcher_for_current_line = dispatcher.clone();
    current_line.connect_active_notify(move |current_line| {
        let _ = dispatcher_for_current_line.dispatch(AppMsg::Preferences(
            PreferencesMsg::SetSourceHighlightCurrentLine(current_line.is_active()),
        ));
    });
    let dispatcher_for_syntax_highlighting = dispatcher.clone();
    syntax_highlighting.connect_active_notify(move |syntax_highlighting| {
        let _ = dispatcher_for_syntax_highlighting.dispatch(AppMsg::Preferences(
            PreferencesMsg::SetSourceSyntaxHighlighting(syntax_highlighting.is_active()),
        ));
    });
    dialog.present(Some(parent));
    dialog
}

#[cfg(test)]
pub(crate) fn present_dialogs_for_test(
    parent: &adw::ApplicationWindow,
    config: &carver_config::Config,
    dispatcher: &AppDispatcher,
) -> (adw::PreferencesDialog, adw::AboutDialog) {
    let preferences = show_preferences_dialog(parent, dispatcher, config);
    let about = show_about_window(parent);
    show_category_name_dialog(Some(parent.upcast_ref()), "New Category", "", |_| {});
    show_category_trash_confirmation(Some(parent.upcast_ref()), "Category", || {});
    (preferences, about)
}

fn show_about_window(parent: &adw::ApplicationWindow) -> adw::AboutDialog {
    let about = adw::AboutDialog::builder()
        .application_name("Carver")
        .application_icon(crate::app::APPLICATION_ICON)
        .version(env!("CARGO_PKG_VERSION"))
        .developer_name("Carver contributors")
        .comments("A native GNOME notebook for Carve markup.")
        .issue_url("https://github.com/josbeir/carver/issues")
        .license_type(gtk::License::MitX11)
        .build();
    about.present(Some(parent));
    about
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

/// Presents a searchable category picker for moving one note.
///
/// The dialog retains only transient widget state. Selecting a destination or creating one
/// dispatches a typed action; the MVU runtime owns all persistence and subsequent reloads.
pub(crate) fn show_move_note_dialog(
    parent: Option<&gtk::Window>,
    dispatcher: &AppDispatcher,
    note_id: NoteId,
    source_category_id: CategoryId,
    note_title: &str,
    categories: &[CategorySummary],
) -> adw::Dialog {
    let dialog = adw::Dialog::builder()
        .title(format!("Move “{note_title}”"))
        .content_width(420)
        .content_height(460)
        .build();
    dialog.set_widget_name("move-note-dialog");
    let content = gtk::Box::new(gtk::Orientation::Vertical, 12);
    content.set_margin_start(18);
    content.set_margin_end(18);
    content.set_margin_top(18);
    content.set_margin_bottom(18);
    let title = gtk::Label::new(Some(&format!("Move “{note_title}”")));
    title.add_css_class("title-3");
    title.set_xalign(0.0);
    content.append(&title);
    let search = gtk::SearchEntry::new();
    search.set_widget_name("move-note-search");
    search.set_placeholder_text(Some("Search categories"));
    content.append(&search);
    let list = gtk::ListBox::new();
    list.set_widget_name("move-note-category-list");
    list.set_selection_mode(gtk::SelectionMode::None);
    list.add_css_class("boxed-list");
    let scroll = gtk::ScrolledWindow::new();
    scroll.set_vexpand(true);
    scroll.set_child(Some(&list));
    content.append(&scroll);
    let new_category = gtk::Button::with_label("New Category…");
    new_category.set_widget_name("move-note-new-category-button");
    new_category.add_css_class("flat");
    new_category.set_halign(gtk::Align::Start);
    content.append(&new_category);
    let cancel = gtk::Button::with_label("Cancel");
    cancel.set_halign(gtk::Align::End);
    content.append(&cancel);
    dialog.set_child(Some(&content));

    let categories = std::rc::Rc::new(categories.to_vec());
    populate_move_categories(
        &list,
        &categories,
        "",
        dispatcher,
        note_id,
        source_category_id,
        &dialog,
    );
    connect_move_picker_search(
        &search,
        &list,
        &categories,
        dispatcher,
        note_id,
        source_category_id,
        &dialog,
    );
    connect_move_picker_new_category(
        &new_category,
        parent,
        dispatcher,
        note_id,
        source_category_id,
        &dialog,
    );
    let dialog_for_cancel = dialog.clone();
    cancel.connect_clicked(move |_| {
        dialog_for_cancel.close();
    });
    dialog.present(parent);
    dialog
}

fn connect_move_picker_search(
    search: &gtk::SearchEntry,
    list: &gtk::ListBox,
    categories: &std::rc::Rc<Vec<CategorySummary>>,
    dispatcher: &AppDispatcher,
    note_id: NoteId,
    source_category_id: CategoryId,
    dialog: &adw::Dialog,
) {
    let list = list.clone();
    let categories = std::rc::Rc::clone(categories);
    let dispatcher = dispatcher.clone();
    let dialog = dialog.clone();
    search.connect_search_changed(move |search| {
        populate_move_categories(
            &list,
            &categories,
            search.text().as_str(),
            &dispatcher,
            note_id,
            source_category_id,
            &dialog,
        );
    });
}

fn connect_move_picker_new_category(
    button: &gtk::Button,
    parent: Option<&gtk::Window>,
    dispatcher: &AppDispatcher,
    note_id: NoteId,
    source_category_id: CategoryId,
    dialog: &adw::Dialog,
) {
    let parent = parent.cloned();
    let dispatcher = dispatcher.clone();
    let dialog = dialog.clone();
    button.connect_clicked(move |_| {
        dialog.close();
        let dispatcher = dispatcher.clone();
        show_category_name_dialog(parent.as_ref(), "New Category", "", move |name| {
            let _ = dispatcher.dispatch(AppMsg::Action(ActionMsg::CreateCategoryAndMoveNote {
                name,
                note_id,
                source_category_id,
            }));
        });
    });
}

fn populate_move_categories(
    list: &gtk::ListBox,
    categories: &[CategorySummary],
    query: &str,
    dispatcher: &AppDispatcher,
    note_id: NoteId,
    source_category_id: CategoryId,
    dialog: &adw::Dialog,
) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }
    let query = query.trim().to_lowercase();
    let mut matching_categories = 0;
    for summary in categories {
        let category = &summary.category;
        if !query.is_empty() && !category.name.to_lowercase().contains(&query) {
            continue;
        }
        matching_categories += 1;
        let row = gtk::ListBoxRow::new();
        row.set_widget_name(&format!("move-note-category:{}", category.id));
        let button = gtk::Button::new();
        button.add_css_class("flat");
        button.set_hexpand(true);
        let content = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        content.set_margin_start(12);
        content.set_margin_end(12);
        content.set_margin_top(8);
        content.set_margin_bottom(8);
        content.append(&gtk::Image::from_icon_name("folder-symbolic"));
        let label = gtk::Label::new(Some(&category.name));
        label.set_xalign(0.0);
        label.set_hexpand(true);
        content.append(&label);
        if category.id == source_category_id {
            let current = gtk::Label::new(Some("Current"));
            current.add_css_class("dim-label");
            content.append(&current);
            button.set_sensitive(false);
        } else {
            let dispatcher = dispatcher.clone();
            let dialog = dialog.clone();
            let category_id = category.id;
            button.connect_clicked(move |_| {
                dialog.close();
                let _ = dispatcher.dispatch(AppMsg::Action(ActionMsg::MoveNote {
                    note_id,
                    source_category_id,
                    category_id,
                }));
            });
        }
        button.set_child(Some(&content));
        row.set_child(Some(&button));
        list.append(&row);
    }
    if matching_categories == 0 {
        let row = gtk::ListBoxRow::new();
        row.set_selectable(false);
        let label = gtk::Label::new(Some("No categories found"));
        label.add_css_class("dim-label");
        label.set_margin_top(12);
        label.set_margin_bottom(12);
        row.set_child(Some(&label));
        list.append(&row);
    }
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
