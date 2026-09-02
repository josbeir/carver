//! GTK frontend tests.

use std::{error::Error, rc::Rc};

use carver_config::{AppPaths, Config, EditorMode, load};
use carver_sdk::LibraryClient;
use gtk::prelude::*;
use libadwaita as adw;
use tempfile::TempDir;

use crate::{
    app::ensure_first_category,
    browser::build_browser,
    controller::{
        AppState, active_category, create_next_category, create_note_for_active_category,
        open_note, rename_category, save_current_note, store_pasted_image, trash_current_note,
    },
    dialogs::persist_window_config,
    editor::{build_editor, install_image_paste},
    sidebar::build_sidebar,
    trash::{build_trash, refresh_trash},
};

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
fn state_uses_the_configured_initial_editor_mode() -> TestResult {
    let (_temporary_directory, state) = test_state()?;
    let mut config = Config::default();
    config.editor.default_mode = EditorMode::Source;
    let source_state = AppState::new(state.client.clone(), config);

    assert!(source_state.source_mode.get());
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
    let texture = gtk::gdk::MemoryTexture::new(1, 1, gtk::gdk::MemoryFormat::R8g8b8a8, &pixels, 4);
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
    let trash_overlay = adw::ToastOverlay::new();
    let trash_page = build_trash(&state, &stack, &trash_overlay);
    stack.add_named(&trash_page, Some("trash"));
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
    let test_window = gtk::Window::new();
    let test_layout = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    test_layout.append(&sidebar);
    test_layout.append(&stack);
    test_window.set_child(Some(&test_layout));
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
    let count_label = widget_as::<gtk::Label>(
        category_list.upcast_ref(),
        &format!("category-count:{}", second_category.id),
    );
    assert_eq!(
        count_label.map(|label| label.text().to_string()),
        Some("0 notes".to_owned())
    );
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
    let rename_name = widget_as::<gtk::Entry>(rename_dialog.upcast_ref(), "category-name-entry");
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

    let delete = widget_as::<gtk::Button>(
        category_row.upcast_ref(),
        &format!("delete-category:{}", second_category.id),
    );
    assert!(delete.is_some());
    let Some(delete) = delete else {
        return Ok(());
    };
    delete.emit_clicked();
    assert_eq!(state.client.categories()?.len(), 1);
    assert!(
        find_widget(
            category_list.upcast_ref(),
            &format!("category:{}", second_category.id)
        )
        .is_none()
    );

    let hide_categories = widget_as::<gtk::Button>(&sidebar, "hide-categories-button");
    assert!(hide_categories.is_some());
    let Some(hide_categories) = hide_categories else {
        return Ok(());
    };
    hide_categories.emit_clicked();
    assert!(split_view.is_collapsed() && split_view.shows_content());
    let toggle_categories = widget_as::<gtk::ToggleButton>(&browser, "toggle-categories-button");
    assert!(toggle_categories.is_some());
    let Some(toggle_categories) = toggle_categories else {
        return Ok(());
    };
    toggle_categories.emit_clicked();
    assert!(
        !split_view.is_collapsed() && split_view.shows_content() && toggle_categories.is_active()
    );
    toggle_categories.emit_clicked();
    assert!(
        split_view.is_collapsed() && split_view.shows_content() && !toggle_categories.is_active()
    );

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
    search.set_text("");
    search.emit_by_name::<()>("search-changed", &[]);
    back.emit_clicked();
    assert_eq!(stack.visible_child_name().as_deref(), Some("browser"));
    assert_eq!(
        state.client.note(created.id)?.map(|note| note.source),
        Some("# Edited from source mode".to_owned())
    );
    let refreshed_title = find_widget(&browser, &format!("note-title:{}", created.id));
    let refreshed_title = refreshed_title.and_then(|widget| widget.downcast::<gtk::Label>().ok());
    assert_eq!(
        refreshed_title
            .as_ref()
            .map(|label| label.text().to_string()),
        Some("Edited from source mode".to_owned())
    );
    stack.set_visible_child_name("editor");
    trash.emit_clicked();
    assert!(
        state
            .client
            .note(created.id)?
            .is_some_and(|note| note.trashed_at.is_some())
    );
    let open_trash = widget_as::<gtk::Button>(&sidebar, "open-trash-button");
    assert!(open_trash.is_some());
    let Some(open_trash) = open_trash else {
        return Ok(());
    };
    open_trash.emit_clicked();
    assert_eq!(stack.visible_child_name().as_deref(), Some("trash"));
    let restore_category = widget_as::<gtk::Button>(
        &trash_page,
        &format!("restore-category:{}", second_category.id),
    );
    assert!(restore_category.is_some());
    let Some(restore_category) = restore_category else {
        return Ok(());
    };
    restore_category.emit_clicked();
    assert_eq!(state.client.categories()?.len(), 2);
    let restore_note =
        widget_as::<gtk::Button>(&trash_page, &format!("restore-note:{}", created.id));
    assert!(restore_note.is_some());
    let Some(restore_note) = restore_note else {
        return Ok(());
    };
    restore_note.emit_clicked();
    assert!(
        state
            .client
            .note(created.id)?
            .is_some_and(|note| note.trashed_at.is_none())
    );
    state.client.trash_category(second_category.id)?;
    refresh_trash(&state);
    let empty_trash = widget_as::<gtk::Button>(&trash_page, "empty-trash-button");
    assert!(
        empty_trash
            .as_ref()
            .is_some_and(gtk::prelude::WidgetExt::is_sensitive)
    );
    let Some(empty_trash) = empty_trash else {
        return Ok(());
    };
    empty_trash.emit_clicked();
    let empty_dialog = active_dialog();
    assert!(empty_dialog.is_some());
    let Some(empty_dialog) = empty_dialog else {
        return Ok(());
    };
    empty_dialog.response(gtk::ResponseType::Accept);
    assert!(state.client.trash_contents()?.is_empty());
    assert_eq!(state.client.categories()?.len(), 1);
    test_window.close();
    Ok(())
}
