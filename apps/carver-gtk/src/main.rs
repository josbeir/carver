//! Carver's GTK application entry point.

#![forbid(unsafe_code)]

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    time::Duration as StdDuration,
};

use adw::prelude::*;
use carver_config::{AppPaths, Config, ConfigError, load, save};
use carver_sdk::{Category, CategoryId, LibraryClient, LibraryError, Note, NoteId, NoteSummary};
use gtk::prelude::*;
use libadwaita as adw;
use time::{Duration, OffsetDateTime, UtcOffset};

mod formatting;

const APPLICATION_ID: &str = "io.github.josbeir.Carver.Devel";

fn main() -> glib::ExitCode {
    let application =
        adw::Application::new(Some(APPLICATION_ID), gtk::gio::ApplicationFlags::empty());
    application.connect_activate(build_application);
    application.run()
}

struct AppState {
    client: LibraryClient,
    config: RefCell<Config>,
    selected_category: Cell<Option<CategoryId>>,
    current_note: RefCell<Option<Note>>,
    source_mode: Cell<bool>,
    search_query: RefCell<String>,
    browser_list: RefCell<Option<gtk::ListBox>>,
    browser_stack: RefCell<Option<gtk::Stack>>,
}

impl AppState {
    fn new(client: LibraryClient, config: Config) -> Self {
        Self {
            client,
            config: RefCell::new(config),
            selected_category: Cell::new(None),
            current_note: RefCell::new(None),
            source_mode: Cell::new(false),
            search_query: RefCell::new(String::new()),
            browser_list: RefCell::new(None),
            browser_stack: RefCell::new(None),
        }
    }
}

fn build_application(application: &adw::Application) {
    load_styles();
    let paths = AppPaths::discover();
    let config_path = paths.config_file();
    let config = load(&config_path).unwrap_or_default();
    let _ = save(&config_path, &config);
    let client = match LibraryClient::open(&paths) {
        Ok(client) => client,
        Err(error) => {
            show_startup_error(application, &error.to_string());
            return;
        }
    };

    ensure_first_category(&client);
    let state = Rc::new(AppState::new(client, config));
    build_window(application, &state, config_path);
}

fn load_styles() {
    let provider = gtk::CssProvider::new();
    provider.load_from_data(include_str!("style.css"));
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

fn ensure_first_category(client: &LibraryClient) {
    if let Ok(categories) = client.categories()
        && categories.is_empty()
    {
        let _ = client.create_category("Notes");
    }
}

/// Returns the active category, falling back to the first available category.
///
/// The client is deliberately cloned before querying SQLite so a `RefCell` borrow
/// can never be held while a callback makes an I/O call.
fn active_category(state: &AppState) -> Result<Option<CategoryId>, LibraryError> {
    let selected_category = state.selected_category.get();
    if selected_category.is_some() {
        return Ok(selected_category);
    }
    Ok(state
        .client
        .categories()?
        .first()
        .map(|category| category.id))
}

/// Creates a note and makes it the current note without overlapping state borrows.
fn create_note_for_active_category(state: &AppState) -> Result<Option<Note>, LibraryError> {
    let Some(category_id) = active_category(state)? else {
        return Ok(None);
    };
    let note = state.client.create_note(category_id)?;
    state.current_note.replace(Some(note.clone()));
    Ok(Some(note))
}

/// Creates the next numbered category.
#[cfg(test)]
fn create_next_category(state: &AppState) -> Result<Category, LibraryError> {
    let sequence = state.client.categories()?.len() + 1;
    state
        .client
        .create_category(&format!("Category {sequence}"))
}

/// Creates a category using a user-provided name.
fn create_category(state: &AppState, name: &str) -> Result<Category, LibraryError> {
    state.client.create_category(name.trim())
}

/// Renames a category using the frontend-neutral SDK.
fn rename_category(
    state: &AppState,
    category_id: CategoryId,
    name: &str,
) -> Result<Category, LibraryError> {
    state.client.rename_category(category_id, name.trim())
}

/// Moves the active note to trash and clears the editor state.
fn trash_current_note(state: &AppState) -> Result<bool, LibraryError> {
    let note = state.current_note.borrow().clone();
    let Some(note) = note else {
        return Ok(false);
    };
    state.client.trash_note(note.id)?;
    state.current_note.take();
    Ok(true)
}

/// Loads a note into the editor state.
fn open_note(state: &AppState, note_id: NoteId) -> Result<Option<Note>, LibraryError> {
    let note = state.client.note(note_id)?;
    state.current_note.replace(note.clone());
    Ok(note)
}

/// Saves the active note's source and updates its optimistic-concurrency revision.
fn save_current_note(state: &AppState, source: &str) -> Result<Option<Note>, LibraryError> {
    let note = state.current_note.borrow().clone();
    let Some(note) = note else {
        return Ok(None);
    };
    let saved = state.client.save_note(note.id, note.revision, source)?;
    state.current_note.replace(Some(saved.clone()));
    Ok(Some(saved))
}

/// Stores a clipboard image for the active note and returns its Carve image path.
fn store_pasted_image(state: &AppState, bytes: &[u8]) -> Result<Option<String>, LibraryError> {
    let note = state.current_note.borrow().clone();
    let Some(note) = note else {
        return Ok(None);
    };
    Ok(Some(state.client.store_asset(note.id, "png", bytes)?))
}

fn build_window(
    application: &adw::Application,
    state: &Rc<AppState>,
    config_path: std::path::PathBuf,
) {
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

fn install_window_actions(
    window: &adw::ApplicationWindow,
    state: &Rc<AppState>,
    config_path: &std::path::Path,
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
    config_path: &std::path::Path,
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
fn persist_window_config(
    state: &AppState,
    config_path: &std::path::Path,
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

fn show_category_name_dialog(
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

fn build_sidebar(state: &Rc<AppState>, split_view: &adw::NavigationSplitView) -> gtk::Widget {
    let container = gtk::Box::new(gtk::Orientation::Vertical, 0);
    container.add_css_class("sidebar");

    let header = adw::HeaderBar::new();
    let new_category = gtk::Button::from_icon_name("folder-new-symbolic");
    new_category.set_widget_name("new-category-button");
    new_category.set_tooltip_text(Some("New Category"));
    header.pack_end(&new_category);
    container.append(&header);

    let list = gtk::ListBox::new();
    list.set_widget_name("category-list");
    list.set_selection_mode(gtk::SelectionMode::Single);
    list.add_css_class("navigation-sidebar");
    let home = gtk::ListBoxRow::new();
    home.set_selectable(true);
    home.set_child(Some(&sidebar_row("go-home-symbolic", "Home")));
    list.append(&home);

    if let Ok(categories) = state.client.categories() {
        for category in categories {
            list.append(&category_sidebar_row(state, &category));
        }
    }
    list.select_row(Some(&home));

    let state_for_selection = Rc::clone(state);
    let split_for_selection = split_view.clone();
    list.connect_row_selected(move |_list, row| {
        let Some(row) = row else {
            return;
        };
        let selected = row
            .widget_name()
            .strip_prefix("category:")
            .and_then(|value| uuid::Uuid::parse_str(value).ok())
            .map(CategoryId::from_uuid);
        state_for_selection.selected_category.set(selected);
        refresh_browser(&state_for_selection);
        if split_for_selection.is_collapsed() {
            split_for_selection.set_show_content(true);
        }
    });

    let state_for_new = Rc::clone(state);
    let list_for_new = list.clone();
    new_category.connect_clicked(move |button| {
        let parent = button
            .root()
            .and_then(|root| root.downcast::<gtk::Window>().ok());
        let state = Rc::clone(&state_for_new);
        let list = list_for_new.clone();
        show_category_name_dialog(parent.as_ref(), "New Category", "", move |name| {
            if let Ok(category) = create_category(&state, &name) {
                list.append(&category_sidebar_row(&state, &category));
            }
        });
    });

    let scroll = gtk::ScrolledWindow::new();
    scroll.set_child(Some(&list));
    scroll.set_vexpand(true);
    container.append(&scroll);
    container.upcast()
}

fn sidebar_row(icon_name: &str, label: &str) -> gtk::Widget {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    row.set_margin_start(12);
    row.set_margin_end(12);
    row.set_margin_top(8);
    row.set_margin_bottom(8);
    row.append(&gtk::Image::from_icon_name(icon_name));
    let text = gtk::Label::new(Some(label));
    text.set_xalign(0.0);
    row.append(&text);
    row.upcast()
}

fn category_sidebar_row(state: &Rc<AppState>, category: &Category) -> gtk::ListBoxRow {
    let row = gtk::ListBoxRow::new();
    row.set_selectable(true);
    row.set_widget_name(&format!("category:{}", category.id));
    let content = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    content.set_margin_start(12);
    content.set_margin_end(6);
    content.set_margin_top(6);
    content.set_margin_bottom(6);
    content.append(&gtk::Image::from_icon_name("folder-symbolic"));
    let name = gtk::Label::new(Some(&category.name));
    name.set_xalign(0.0);
    name.set_hexpand(true);
    content.append(&name);
    let rename = gtk::Button::from_icon_name("document-edit-symbolic");
    rename.set_widget_name(&format!("rename-category:{}", category.id));
    rename.set_tooltip_text(Some("Rename Category"));
    rename.add_css_class("flat");
    let state_for_rename = Rc::clone(state);
    let name_for_rename = name.clone();
    let category_id = category.id;
    rename.connect_clicked(move |button| {
        let parent = button
            .root()
            .and_then(|root| root.downcast::<gtk::Window>().ok());
        let state = Rc::clone(&state_for_rename);
        let name_label = name_for_rename.clone();
        let current_name = name_for_rename.text().to_string();
        show_category_name_dialog(
            parent.as_ref(),
            "Rename Category",
            &current_name,
            move |name| {
                if let Ok(category) = rename_category(&state, category_id, &name) {
                    name_label.set_text(&category.name);
                }
            },
        );
    });
    content.append(&rename);
    row.set_child(Some(&content));
    row
}

fn build_content(
    state: &Rc<AppState>,
    split_view: &adw::NavigationSplitView,
    toast_overlay: &adw::ToastOverlay,
) -> gtk::Widget {
    let stack = gtk::Stack::new();
    stack.set_transition_type(gtk::StackTransitionType::SlideLeftRight);
    let browser = build_browser(state, &stack, split_view);
    stack.add_named(&browser, Some("browser"));
    let editor = build_editor(state, &stack, toast_overlay);
    stack.add_named(&editor, Some("editor"));
    stack.set_visible_child_name("browser");
    stack.upcast()
}

fn build_browser(
    state: &Rc<AppState>,
    stack: &gtk::Stack,
    split_view: &adw::NavigationSplitView,
) -> gtk::Widget {
    let view = adw::ToolbarView::new();
    let header = adw::HeaderBar::new();
    let title = adw::WindowTitle::new("Recent Notes", "Recently edited");
    header.set_title_widget(Some(&title));
    let new_note = gtk::Button::from_icon_name("document-new-symbolic");
    new_note.set_widget_name("new-note-button");
    new_note.set_tooltip_text(Some("New Note"));
    header.pack_end(&new_note);
    let menu = gtk::gio::Menu::new();
    menu.append(Some("Preferences"), Some("win.preferences"));
    menu.append(Some("About Carver"), Some("win.about"));
    let app_menu = gtk::MenuButton::new();
    app_menu.set_widget_name("app-menu-button");
    app_menu.set_icon_name("open-menu-symbolic");
    app_menu.set_tooltip_text(Some("Main Menu"));
    app_menu.set_menu_model(Some(&menu));
    header.pack_end(&app_menu);
    let reveal_sidebar = gtk::Button::from_icon_name("sidebar-show-symbolic");
    reveal_sidebar.set_widget_name("show-categories-button");
    reveal_sidebar.set_tooltip_text(Some("Show Categories"));
    let split = split_view.clone();
    reveal_sidebar.connect_clicked(move |_| split.set_show_content(false));
    header.pack_start(&reveal_sidebar);
    view.add_top_bar(&header);

    let content = gtk::Box::new(gtk::Orientation::Vertical, 12);
    content.set_margin_start(18);
    content.set_margin_end(18);
    content.set_margin_top(18);
    content.set_margin_bottom(18);
    let search = gtk::SearchEntry::new();
    search.set_widget_name("note-search-entry");
    search.set_placeholder_text(Some("Search notes"));
    content.append(&search);
    let list = gtk::ListBox::new();
    list.set_widget_name("note-list");
    list.set_selection_mode(gtk::SelectionMode::Single);
    list.add_css_class("boxed-list");
    let scroll = gtk::ScrolledWindow::new();
    scroll.set_vexpand(true);
    scroll.set_child(Some(&list));
    content.append(&scroll);
    let clamp = adw::Clamp::new();
    clamp.set_widget_name("browser-content-clamp");
    clamp.set_maximum_size(720);
    clamp.set_tightening_threshold(520);
    clamp.set_child(Some(&content));
    view.set_content(Some(&clamp));

    state.browser_list.replace(Some(list.clone()));
    state.browser_stack.replace(Some(stack.clone()));
    refresh_browser(state);
    let state_for_search = Rc::clone(state);
    search.connect_search_changed(move |entry| {
        state_for_search
            .search_query
            .replace(entry.text().to_string());
        refresh_browser(&state_for_search);
    });
    let state_for_new = Rc::clone(state);
    let stack_for_new = stack.clone();
    new_note.connect_clicked(move |_| {
        if let Ok(Some(_note)) = create_note_for_active_category(&state_for_new) {
            refresh_browser(&state_for_new);
            stack_for_new.set_visible_child_name("editor");
        }
    });
    let state_for_row = Rc::clone(state);
    let stack_for_row = stack.clone();
    list.connect_row_activated(move |_list, row| {
        let widget_name = row.widget_name();
        let Some(raw_id) = widget_name.strip_prefix("note:") else {
            return;
        };
        let Ok(id) = uuid::Uuid::parse_str(raw_id) else {
            return;
        };
        if let Ok(Some(_note)) = open_note(&state_for_row, NoteId::from_uuid(id)) {
            stack_for_row.set_visible_child_name("editor");
        }
    });
    view.upcast()
}

fn refresh_browser(state: &AppState) {
    let (list, stack) = {
        let Some(list) = state.browser_list.borrow().clone() else {
            return;
        };
        let Some(stack) = state.browser_stack.borrow().clone() else {
            return;
        };
        (list, stack)
    };
    refresh_note_list(&list, state, &stack);
}

fn refresh_note_list(list: &gtk::ListBox, state: &AppState, _stack: &gtk::Stack) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }
    let search_query = state.search_query.borrow();
    let search_is_active = !search_query.trim().is_empty();
    let entries: Vec<NoteSummary> = if search_is_active {
        state
            .client
            .search(&search_query, state.selected_category.get(), 200)
            .unwrap_or_default()
            .into_iter()
            .map(|hit| hit.note)
            .collect()
    } else {
        state
            .client
            .recent_notes(state.selected_category.get(), 200, 0)
            .unwrap_or_default()
    };
    drop(search_query);
    let mut previous_day = None;
    for note in entries {
        let day = local_day(note.updated_at);
        if !search_is_active && previous_day != Some(day) {
            let header = gtk::ListBoxRow::new();
            header.set_selectable(false);
            let label = gtk::Label::new(Some(&day_label(day)));
            label.set_xalign(0.0);
            label.add_css_class("heading");
            label.set_margin_top(18);
            label.set_margin_bottom(6);
            header.set_child(Some(&label));
            list.append(&header);
            previous_day = Some(day);
        }
        let row = gtk::ListBoxRow::new();
        row.set_widget_name(&format!("note:{}", note.id));
        let box_ = gtk::Box::new(gtk::Orientation::Vertical, 4);
        box_.set_margin_start(12);
        box_.set_margin_end(12);
        box_.set_margin_top(10);
        box_.set_margin_bottom(10);
        let title = gtk::Label::new(Some(&note.title));
        title.set_xalign(0.0);
        title.add_css_class("heading");
        box_.append(&title);
        let excerpt = gtk::Label::new(Some(&note.excerpt));
        excerpt.set_xalign(0.0);
        excerpt.set_ellipsize(gtk::pango::EllipsizeMode::End);
        excerpt.set_lines(2);
        excerpt.add_css_class("dim-label");
        box_.append(&excerpt);
        row.set_child(Some(&box_));
        list.append(&row);
    }
}

fn local_day(timestamp: OffsetDateTime) -> time::Date {
    timestamp
        .to_offset(UtcOffset::current_local_offset().unwrap_or(UtcOffset::UTC))
        .date()
}

fn day_label(day: time::Date) -> String {
    let today = OffsetDateTime::now_utc()
        .to_offset(UtcOffset::current_local_offset().unwrap_or(UtcOffset::UTC))
        .date();
    if day == today {
        return "Today".to_owned();
    }
    if day == today - Duration::days(1) {
        return "Yesterday".to_owned();
    }
    day.to_string()
}

// CONTEXT: GTK retains signal closures only while their widgets live. Keeping the
// editor construction in one ownership scope avoids accidental short-lived widgets.
#[expect(clippy::too_many_lines)]
fn build_editor(
    state: &Rc<AppState>,
    stack: &gtk::Stack,
    toast_overlay: &adw::ToastOverlay,
) -> gtk::Widget {
    let view = adw::ToolbarView::new();
    let header = adw::HeaderBar::new();
    let back = gtk::Button::from_icon_name("go-previous-symbolic");
    back.set_widget_name("back-to-notes-button");
    back.set_tooltip_text(Some("Back to notes"));
    header.pack_start(&back);
    let title = adw::WindowTitle::new("Note", "Saved automatically");
    header.set_title_widget(Some(&title));
    let mode = gtk::ToggleButton::with_label("Carve Source");
    mode.set_widget_name("editor-mode-toggle");
    header.pack_end(&mode);
    let trash = gtk::Button::from_icon_name("user-trash-symbolic");
    trash.set_widget_name("delete-note-button");
    trash.set_tooltip_text(Some("Move Note to Trash"));
    trash.add_css_class("flat");
    header.pack_end(&trash);
    view.add_top_bar(&header);

    let format_bar = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    format_bar.set_widget_name("formatting-toolbar");
    format_bar.add_css_class("toolbar");
    format_bar.set_margin_start(12);
    format_bar.set_margin_end(12);
    format_bar.set_margin_top(6);
    format_bar.set_margin_bottom(6);

    let editor_stack = gtk::Stack::new();
    let rich_buffer = gtk::TextBuffer::new(None);
    let rich = gtk::TextView::with_buffer(&rich_buffer);
    rich.set_widget_name("rich-editor");
    rich.set_wrap_mode(gtk::WrapMode::WordChar);
    rich.set_top_margin(24);
    rich.set_bottom_margin(24);
    rich.set_left_margin(24);
    rich.set_right_margin(24);
    let source_buffer = gtk::TextBuffer::new(None);
    let source = gtk::TextView::with_buffer(&source_buffer);
    source.set_widget_name("source-editor");
    source.set_monospace(true);
    source.set_wrap_mode(gtk::WrapMode::WordChar);
    source.set_top_margin(24);
    source.set_bottom_margin(24);
    source.set_left_margin(24);
    source.set_right_margin(24);
    formatting::append_controls(&format_bar, &rich_buffer);
    let rich_scroll = gtk::ScrolledWindow::new();
    rich_scroll.set_child(Some(&rich));
    let source_scroll = gtk::ScrolledWindow::new();
    source_scroll.set_child(Some(&source));
    editor_stack.add_named(&rich_scroll, Some("rich"));
    editor_stack.add_named(&source_scroll, Some("source"));
    editor_stack.set_visible_child_name("rich");
    view.add_top_bar(&format_bar);
    view.set_content(Some(&editor_stack));

    let state_for_mode = Rc::clone(state);
    let stack_for_mode = editor_stack.clone();
    let rich_for_mode = rich_buffer.clone();
    let source_for_mode = source_buffer.clone();
    mode.connect_toggled(move |button| {
        let source_mode = button.is_active();
        state_for_mode.source_mode.set(source_mode);
        if source_mode {
            source_for_mode.set_text(&rich_for_mode.text(
                &rich_for_mode.start_iter(),
                &rich_for_mode.end_iter(),
                false,
            ));
            stack_for_mode.set_visible_child_name("source");
            button.set_label("Rich Text");
        } else {
            rich_for_mode.set_text(&source_for_mode.text(
                &source_for_mode.start_iter(),
                &source_for_mode.end_iter(),
                false,
            ));
            stack_for_mode.set_visible_child_name("rich");
            button.set_label("Carve Source");
        }
    });
    let state_for_trash = Rc::clone(state);
    let stack_for_trash = stack.clone();
    let toast_for_trash = toast_overlay.clone();
    trash.connect_clicked(move |_| {
        let note_for_undo = state_for_trash.current_note.borrow().clone();
        match trash_current_note(&state_for_trash) {
            Ok(true) => {
                refresh_browser(&state_for_trash);
                stack_for_trash.set_visible_child_name("browser");
                let toast = adw::Toast::new("Moved note to Trash");
                toast.set_button_label(Some("Undo"));
                let state_for_undo = Rc::clone(&state_for_trash);
                toast.connect_button_clicked(move |_| {
                    if let Some(note) = note_for_undo.as_ref()
                        && state_for_undo.client.restore_note(note.id).is_ok()
                    {
                        refresh_browser(&state_for_undo);
                    }
                });
                toast_for_trash.add_toast(toast);
            }
            Ok(false) => {}
            Err(error) => toast_for_trash.add_toast(adw::Toast::new(&format!(
                "Could not move note to Trash: {error}"
            ))),
        }
    });
    let state_for_back = Rc::clone(state);
    let stack_for_back = stack.clone();
    let rich_for_back = rich_buffer.clone();
    let source_for_back = source_buffer.clone();
    let toast_for_back = toast_overlay.clone();
    back.connect_clicked(move |_| {
        let has_note = state_for_back.current_note.borrow().is_some();
        let source_mode = state_for_back.source_mode.get();
        if has_note {
            let buffer = if source_mode {
                &source_for_back
            } else {
                &rich_for_back
            };
            let source = buffer.text(&buffer.start_iter(), &buffer.end_iter(), false);
            match save_current_note(&state_for_back, &source) {
                Ok(Some(_saved)) => {
                    stack_for_back.set_visible_child_name("browser");
                }
                Ok(None) => stack_for_back.set_visible_child_name("browser"),
                Err(error) => toast_for_back
                    .add_toast(adw::Toast::new(&format!("Could not save note: {error}"))),
            }
        } else {
            stack_for_back.set_visible_child_name("browser");
        }
    });
    let state_for_rich_save = Rc::clone(state);
    let rich_for_save = rich_buffer.clone();
    let source_for_rich_save = source_buffer.clone();
    let toast_for_rich_save = toast_overlay.clone();
    rich_buffer.connect_changed(move |_| {
        schedule_autosave(
            &state_for_rich_save,
            &rich_for_save,
            &source_for_rich_save,
            &toast_for_rich_save,
        );
    });
    let state_for_source_save = Rc::clone(state);
    let rich_for_source_save = rich_buffer.clone();
    let source_for_source_save = source_buffer.clone();
    let toast_for_source_save = toast_overlay.clone();
    source_buffer.connect_changed(move |_| {
        schedule_autosave(
            &state_for_source_save,
            &rich_for_source_save,
            &source_for_source_save,
            &toast_for_source_save,
        );
    });
    let _rich_image_paste = install_image_paste(&rich, &rich_buffer, state, toast_overlay);
    let _source_image_paste = install_image_paste(&source, &source_buffer, state, toast_overlay);
    let state_for_visible = Rc::clone(state);
    let rich_for_visible = rich_buffer.clone();
    let source_for_visible = source_buffer.clone();
    stack.connect_visible_child_notify(move |stack| {
        if stack.visible_child_name().as_deref() == Some("editor")
            && let Some(note) = state_for_visible.current_note.borrow().as_ref()
        {
            rich_for_visible.set_text(&note.source);
            source_for_visible.set_text(&note.source);
        }
    });
    view.upcast()
}

fn install_image_paste(
    view: &gtk::TextView,
    buffer: &gtk::TextBuffer,
    state: &Rc<AppState>,
    toast_overlay: &adw::ToastOverlay,
) -> gtk::EventControllerKey {
    let controller = gtk::EventControllerKey::new();
    let state = Rc::clone(state);
    let buffer = buffer.clone();
    let toast_overlay = toast_overlay.clone();
    let clipboard = view.display().clipboard();
    controller.connect_key_pressed(move |_controller, key, _keycode, modifiers| {
        if key != gtk::gdk::Key::v || !modifiers.contains(gtk::gdk::ModifierType::CONTROL_MASK) {
            return glib::Propagation::Proceed;
        }
        let state = Rc::clone(&state);
        let buffer = buffer.clone();
        let toast_overlay = toast_overlay.clone();
        clipboard.read_texture_async(None::<&gtk::gio::Cancellable>, move |result| {
            let Ok(Some(texture)) = result else {
                return;
            };
            let bytes = texture.save_to_png_bytes();
            match store_pasted_image(&state, bytes.as_ref()) {
                Ok(Some(path)) => buffer.insert_at_cursor(&format!("\n![Pasted image]({path})\n")),
                Ok(None) => {}
                Err(error) => toast_overlay.add_toast(adw::Toast::new(&format!(
                    "Could not store pasted image: {error}"
                ))),
            }
        });
        glib::Propagation::Proceed
    });
    view.add_controller(controller.clone());
    controller
}

fn schedule_autosave(
    state: &Rc<AppState>,
    rich_buffer: &gtk::TextBuffer,
    source_buffer: &gtk::TextBuffer,
    toast_overlay: &adw::ToastOverlay,
) {
    let delay = state.config.borrow().editor.autosave_delay_ms;
    let state = Rc::clone(state);
    let rich_buffer = rich_buffer.clone();
    let source_buffer = source_buffer.clone();
    let toast_overlay = toast_overlay.clone();
    glib::timeout_add_local_once(StdDuration::from_millis(delay), move || {
        let note = state.current_note.borrow().clone();
        let source_mode = state.source_mode.get();
        let Some(note) = note else {
            return;
        };
        let buffer = if source_mode {
            source_buffer
        } else {
            rich_buffer
        };
        let source = buffer.text(&buffer.start_iter(), &buffer.end_iter(), false);
        match state.client.save_note(note.id, note.revision, &source) {
            Ok(saved) => {
                state.current_note.replace(Some(saved));
            }
            Err(error) => {
                toast_overlay.add_toast(adw::Toast::new(&format!("Could not save note: {error}")));
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::error::Error;

    use tempfile::TempDir;

    type TestResult = Result<(), Box<dyn Error>>;
    type TestState = (TempDir, Rc<AppState>);

    fn test_state() -> Result<TestState, Box<dyn Error>> {
        let temporary_directory = tempfile::tempdir()?;
        let paths = AppPaths {
            config_dir: temporary_directory.path().join("config"),
            data_dir: temporary_directory.path().join("data"),
            cache_dir: temporary_directory.path().join("cache"),
        };
        let client = LibraryClient::open(&paths)?;
        ensure_first_category(&client);
        let state = Rc::new(AppState::new(client, Config::default()));
        Ok((temporary_directory, state))
    }

    fn find_widget(root: &gtk::Widget, name: &str) -> Option<gtk::Widget> {
        if root.widget_name() == name {
            return Some(root.clone());
        }
        let mut child = root.first_child();
        while let Some(widget) = child {
            if let Some(found) = find_widget(&widget, name) {
                return Some(found);
            }
            child = widget.next_sibling();
        }
        None
    }

    fn widget_as<T: glib::prelude::IsA<gtk::Widget> + glib::object::ObjectType>(
        root: &gtk::Widget,
        name: &str,
    ) -> Option<T> {
        find_widget(root, name).and_then(|widget| widget.downcast::<T>().ok())
    }

    fn note_row_count(root: &gtk::Widget) -> usize {
        let own_count = usize::from(root.widget_name().starts_with("note:"));
        let mut child = root.first_child();
        let mut descendants = 0;
        while let Some(widget) = child {
            descendants += note_row_count(&widget);
            child = widget.next_sibling();
        }
        own_count + descendants
    }

    fn active_dialog() -> Option<gtk::Dialog> {
        gtk::Window::list_toplevels()
            .into_iter()
            .rev()
            .find_map(|window| window.downcast::<gtk::Dialog>().ok())
    }

    fn run_main_context_until(predicate: impl Fn() -> bool) -> bool {
        let context = glib::MainContext::default();
        for _ in 0..20 {
            while context.pending() {
                context.iteration(false);
            }
            if predicate() {
                return true;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        false
    }

    #[test]
    fn state_actions_create_open_and_save_notes_without_reentrant_borrows() -> TestResult {
        let (_temporary_directory, state) = test_state()?;
        let first_category = active_category(&state)?;
        assert!(first_category.is_some());

        let created = create_note_for_active_category(&state)?;
        let Some(created) = created else {
            panic!("a seeded library must have a category");
        };
        assert_eq!(
            state.current_note.borrow().as_ref().map(|note| note.id),
            Some(created.id)
        );

        let saved = save_current_note(&state, "# Regression note\nNo RefCell panic")?;
        let Some(saved) = saved else {
            panic!("the newly created note must remain active");
        };
        assert_eq!(saved.revision.0, 2);

        state.current_note.take();
        let reopened = open_note(&state, created.id)?;
        assert_eq!(
            reopened.as_ref().map(|note| note.source.as_str()),
            Some("# Regression note\nNo RefCell panic")
        );
        Ok(())
    }

    #[test]
    fn state_actions_create_numbered_categories() -> TestResult {
        let (_temporary_directory, state) = test_state()?;
        let category = create_next_category(&state)?;
        assert_eq!(category.name, "Category 2");
        assert_eq!(state.client.categories()?.len(), 2);
        Ok(())
    }

    #[test]
    fn state_actions_rename_categories_and_trash_notes() -> TestResult {
        let (_temporary_directory, state) = test_state()?;
        let category = state.client.categories()?.remove(0);
        let renamed = rename_category(&state, category.id, "Work")?;
        assert_eq!(renamed.name, "Work");

        let created = create_note_for_active_category(&state)?;
        assert!(created.is_some());
        let Some(created) = created else {
            return Ok(());
        };
        assert!(trash_current_note(&state)?);
        assert!(state.current_note.borrow().is_none());
        assert!(state.client.recent_notes(None, 10, 0)?.is_empty());
        state.client.restore_note(created.id)?;
        assert_eq!(state.client.recent_notes(None, 10, 0)?.len(), 1);
        Ok(())
    }

    #[test]
    fn state_action_stores_pasted_images_for_the_active_note() -> TestResult {
        let (temporary_directory, state) = test_state()?;
        let created = create_note_for_active_category(&state)?;
        assert!(created.is_some());
        let image_path = store_pasted_image(&state, b"test-png-bytes")?;
        assert!(image_path.is_some());
        let Some(image_path) = image_path else {
            return Ok(());
        };
        assert!(image_path.starts_with("assets/"));
        assert!(
            temporary_directory
                .path()
                .join("data")
                .join(&image_path)
                .is_file()
        );
        Ok(())
    }

    #[test]
    fn close_action_persists_window_configuration() -> TestResult {
        let (temporary_directory, state) = test_state()?;
        let config_path = temporary_directory
            .path()
            .join("config")
            .join("config.toml");
        persist_window_config(&state, &config_path, 900, 640, true)?;
        let persisted = load(&config_path)?;
        assert_eq!(persisted.window.width, 900);
        assert_eq!(persisted.window.height, 640);
        assert!(persisted.window.maximized);
        Ok(())
    }

    #[test]
    #[ignore = "requires a graphical display; CI runs it under Xvfb"]
    // CONTEXT: GTK must be initialized on one test thread; this scenario covers
    // connected signals in their real order instead of unsafe parallel test setup.
    #[expect(clippy::too_many_lines)]
    fn gtk_interactions_cover_navigation_search_and_editor_controls() -> TestResult {
        gtk::init()?;
        let (_temporary_directory, state) = test_state()?;
        state.config.borrow_mut().editor.autosave_delay_ms = 1;

        let pasted_image_note = create_note_for_active_category(&state)?;
        assert!(pasted_image_note.is_some());
        let image_view = gtk::TextView::new();
        let image_buffer = image_view.buffer();
        let image_controller = install_image_paste(
            &image_view,
            &image_buffer,
            &state,
            &adw::ToastOverlay::new(),
        );
        let pixels = glib::Bytes::from_static(&[0_u8, 128, 255, 255]);
        let texture =
            gtk::gdk::MemoryTexture::new(1, 1, gtk::gdk::MemoryFormat::R8g8b8a8, &pixels, 4);
        image_view.display().clipboard().set_texture(&texture);
        image_controller.emit_by_name::<bool>(
            "key-pressed",
            &[
                &gtk::gdk::Key::v,
                &0_u32,
                &gtk::gdk::ModifierType::CONTROL_MASK,
            ],
        );
        assert!(run_main_context_until(|| {
            image_buffer
                .text(&image_buffer.start_iter(), &image_buffer.end_iter(), false)
                .contains("![Pasted image](assets/")
        }));

        let stack = gtk::Stack::new();
        let split_view = adw::NavigationSplitView::new();
        let browser = build_browser(&state, &stack, &split_view);
        stack.add_named(&browser, Some("browser"));
        let editor_placeholder = gtk::Box::new(gtk::Orientation::Vertical, 0);
        stack.add_named(&editor_placeholder, Some("editor"));
        stack.set_visible_child_name("browser");
        let app_menu = widget_as::<gtk::MenuButton>(&browser, "app-menu-button");
        let content_clamp = widget_as::<adw::Clamp>(&browser, "browser-content-clamp");
        assert!(app_menu.is_some() && content_clamp.is_some());
        let Some(content_clamp) = content_clamp else {
            return Ok(());
        };
        assert_eq!(content_clamp.maximum_size(), 720);

        let new_note = widget_as::<gtk::Button>(&browser, "new-note-button");
        assert!(new_note.is_some());
        let Some(new_note) = new_note else {
            return Ok(());
        };
        new_note.emit_clicked();
        assert_eq!(stack.visible_child_name().as_deref(), Some("editor"));
        let created = state.current_note.borrow().clone();
        assert!(created.is_some());
        let Some(created) = created else {
            return Ok(());
        };

        let note_list = widget_as::<gtk::ListBox>(&browser, "note-list");
        assert!(note_list.is_some());
        let Some(note_list) = note_list else {
            return Ok(());
        };
        let created_row = find_widget(note_list.upcast_ref(), &format!("note:{}", created.id));
        assert!(created_row.is_some());
        let Some(created_row) = created_row else {
            return Ok(());
        };
        let created_row = created_row.downcast::<gtk::ListBoxRow>().ok();
        assert!(created_row.is_some());
        let Some(created_row) = created_row else {
            return Ok(());
        };
        stack.set_visible_child_name("browser");
        note_list.emit_by_name::<()>("row-activated", &[&created_row]);
        assert_eq!(stack.visible_child_name().as_deref(), Some("editor"));

        let search = widget_as::<gtk::SearchEntry>(&browser, "note-search-entry");
        assert!(search.is_some());
        let Some(search) = search else {
            return Ok(());
        };
        search.set_text("Untitled");
        search.emit_by_name::<()>("search-changed", &[]);
        assert_eq!(state.search_query.borrow().as_str(), "Untitled");
        assert_eq!(note_row_count(&browser), 2);
        search.set_text("does-not-exist");
        search.emit_by_name::<()>("search-changed", &[]);
        assert_eq!(note_row_count(&browser), 0);

        let sidebar = build_sidebar(&state, &split_view);
        let category_list = widget_as::<gtk::ListBox>(&sidebar, "category-list");
        assert!(category_list.is_some());
        let Some(category_list) = category_list else {
            return Ok(());
        };
        let new_category = widget_as::<gtk::Button>(&sidebar, "new-category-button");
        assert!(new_category.is_some());
        let Some(new_category) = new_category else {
            return Ok(());
        };
        new_category.emit_clicked();
        let category_dialog = active_dialog();
        assert!(category_dialog.is_some());
        let Some(category_dialog) = category_dialog else {
            return Ok(());
        };
        let category_name =
            widget_as::<gtk::Entry>(category_dialog.upcast_ref(), "category-name-entry");
        assert!(category_name.is_some());
        let Some(category_name) = category_name else {
            return Ok(());
        };
        category_name.set_text("Projects");
        category_dialog.response(gtk::ResponseType::Accept);
        assert_eq!(state.client.categories()?.len(), 2);
        let second_category = state.client.categories()?[1].clone();
        let category_row = find_widget(
            category_list.upcast_ref(),
            &format!("category:{}", second_category.id),
        );
        assert!(category_row.is_some());
        let Some(category_row) = category_row else {
            return Ok(());
        };
        let category_row = category_row.downcast::<gtk::ListBoxRow>().ok();
        assert!(category_row.is_some());
        let Some(category_row) = category_row else {
            return Ok(());
        };
        let rename = widget_as::<gtk::Button>(
            category_row.upcast_ref(),
            &format!("rename-category:{}", second_category.id),
        );
        assert!(rename.is_some());
        let Some(rename) = rename else {
            return Ok(());
        };
        rename.emit_clicked();
        let rename_dialog = active_dialog();
        assert!(rename_dialog.is_some());
        let Some(rename_dialog) = rename_dialog else {
            return Ok(());
        };
        let rename_name =
            widget_as::<gtk::Entry>(rename_dialog.upcast_ref(), "category-name-entry");
        assert!(rename_name.is_some());
        let Some(rename_name) = rename_name else {
            return Ok(());
        };
        rename_name.set_text("Personal");
        rename_dialog.response(gtk::ResponseType::Accept);
        assert_eq!(state.client.categories()?[1].name, "Personal");
        category_list.select_row(Some(&category_row));
        assert_eq!(state.selected_category.get(), Some(second_category.id));
        let home_row = category_list.row_at_index(0);
        assert!(home_row.is_some());
        let Some(home_row) = home_row else {
            return Ok(());
        };
        category_list.select_row(Some(&home_row));
        assert_eq!(state.selected_category.get(), None);

        split_view.set_collapsed(true);
        split_view.set_show_content(true);
        let show_categories = widget_as::<gtk::Button>(&browser, "show-categories-button");
        assert!(show_categories.is_some());
        let Some(show_categories) = show_categories else {
            return Ok(());
        };
        show_categories.emit_clicked();
        assert!(!split_view.shows_content());

        stack.remove(&editor_placeholder);
        let editor = build_editor(&state, &stack, &adw::ToastOverlay::new());
        stack.add_named(&editor, Some("editor"));
        stack.set_visible_child_name("editor");
        let rich = widget_as::<gtk::TextView>(&editor, "rich-editor");
        let source = widget_as::<gtk::TextView>(&editor, "source-editor");
        let mode = widget_as::<gtk::ToggleButton>(&editor, "editor-mode-toggle");
        let back = widget_as::<gtk::Button>(&editor, "back-to-notes-button");
        let bold = widget_as::<gtk::Button>(&editor, "format-bold-button");
        let trash = widget_as::<gtk::Button>(&editor, "delete-note-button");
        assert!(
            rich.is_some()
                && source.is_some()
                && mode.is_some()
                && back.is_some()
                && bold.is_some()
                && trash.is_some()
        );
        let (Some(rich), Some(source), Some(mode), Some(back), Some(bold), Some(trash)) =
            (rich, source, mode, back, bold, trash)
        else {
            return Ok(());
        };
        assert_eq!(
            rich.buffer().text(
                &rich.buffer().start_iter(),
                &rich.buffer().end_iter(),
                false
            ),
            created.source
        );
        rich.buffer().set_text("format me");
        let start = rich.buffer().start_iter();
        let end = rich.buffer().end_iter();
        rich.buffer().select_range(&start, &end);
        bold.emit_clicked();
        assert_eq!(
            rich.buffer().text(
                &rich.buffer().start_iter(),
                &rich.buffer().end_iter(),
                false
            ),
            "*format me*"
        );
        mode.set_active(true);
        assert!(state.source_mode.get());
        assert_eq!(
            source.buffer().text(
                &source.buffer().start_iter(),
                &source.buffer().end_iter(),
                false
            ),
            "*format me*"
        );
        mode.set_active(false);
        assert!(!state.source_mode.get());
        mode.set_active(true);
        source.buffer().set_text("# Edited from source mode");
        let client = state.client.clone();
        assert!(run_main_context_until(|| {
            client
                .note(created.id)
                .ok()
                .flatten()
                .is_some_and(|note| note.source == "# Edited from source mode")
        }));
        back.emit_clicked();
        assert_eq!(stack.visible_child_name().as_deref(), Some("browser"));
        assert_eq!(
            state.client.note(created.id)?.map(|note| note.source),
            Some("# Edited from source mode".to_owned())
        );
        stack.set_visible_child_name("editor");
        trash.emit_clicked();
        assert!(
            state
                .client
                .note(created.id)?
                .is_some_and(|note| note.trashed_at.is_some())
        );
        Ok(())
    }
}
