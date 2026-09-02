//! GTK frontend tests.

use std::{cell::Cell, error::Error, rc::Rc};

use carver_config::{AppPaths, Config, EditorMode, load};
use carver_sdk::LibraryClient;
use carver_storage_sqlite::SqliteLibrary;
use gtk::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;
use tempfile::TempDir;

use crate::{
    app::ensure_first_category,
    browser::build_browser,
    controller::{
        AppState, active_category, create_next_category, create_note_for_active_category,
        open_note, rename_category, save_current_note, store_pasted_image, trash_current_note,
    },
    dialogs::{category_trash_dialog, persist_window_config},
    editor::{
        buffer_text, build_editor, install_editor_shortcuts, install_image_paste,
        install_list_continuation, install_source_shortcuts, render_rich_markup,
    },
    formatting,
    note_move::{move_note_to_category, show_move_note_dialog},
    sidebar::{build_sidebar, refresh_sidebar},
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
    paths.ensure_exists()?;
    let storage = SqliteLibrary::open(&paths.database_file(), &paths.assets_dir())?;
    let client = LibraryClient::spawn(storage)?;
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
    assert_eq!(
        reopened.as_ref().map(|note| note.updated_at),
        Some(saved.updated_at)
    );
    assert_eq!(
        reopened.as_ref().map(|note| note.revision),
        Some(saved.revision)
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
    config.editor.last_mode = EditorMode::Source;
    let source_state = AppState::new(state.client.clone(), config);

    assert!(source_state.source_mode.get());
    assert!(!source_state.rendered_mode.get());

    let mut config = Config::default();
    config.editor.last_mode = EditorMode::Rendered;
    let rendered_state = AppState::new(state.client.clone(), config);
    assert!(!rendered_state.source_mode.get());
    assert!(rendered_state.rendered_mode.get());
    Ok(())
}

#[test]
fn state_persists_the_last_explicit_editor_surface() -> TestResult {
    let (temporary_directory, state) = test_state()?;
    let path = temporary_directory.path().join("config/config.toml");
    let persisted_state = AppState::new_with_assets(
        state.client.clone(),
        Config::default(),
        None,
        Some(path.clone()),
    );

    persisted_state.set_last_editor_mode(EditorMode::Rendered)?;

    let loaded = load(&path)?;
    assert_eq!(loaded.editor.last_mode, EditorMode::Rendered);
    let restored = AppState::new(state.client.clone(), loaded);
    assert!(restored.rendered_mode.get());
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
fn source_split_preference_persists_to_toml() -> TestResult {
    let (temporary_directory, state) = test_state()?;
    let config_path = temporary_directory
        .path()
        .join("config")
        .join("config.toml");
    let persistent_state = AppState::new_with_assets(
        state.client.clone(),
        Config::default(),
        None,
        Some(config_path.clone()),
    );

    persistent_state.set_source_split_view(true)?;

    assert!(load(&config_path)?.editor.source_split_view);
    assert!(persistent_state.config.borrow().editor.source_split_view);
    Ok(())
}

#[test]
#[ignore = "requires a graphical display; CI runs it under Xvfb"]
// CONTEXT: GTK must be initialized on one test thread; this scenario covers
// connected signals in their real order instead of unsafe parallel test setup.
#[expect(clippy::too_many_lines)]
fn gtk_interactions_cover_navigation_search_and_editor_controls() -> TestResult {
    gtk::init()?;
    formatting::tests::restored_link_selection_replaces_instead_of_duplicating_text();
    formatting::tests::rich_code_block_command_serializes_as_fenced_carve();
    formatting::tests::code_block_tag_uses_compact_type_and_line_spacing()?;
    crate::editor::source_commands::graphical_commands_cover_editor_buffer_operations();
    let (_temporary_directory, state) = test_state()?;
    state.config.borrow_mut().editor.autosave_delay_ms = 1;

    let pasted_image_note = create_note_for_active_category(&state)?;
    assert!(pasted_image_note.is_some());
    let image_view = gtk::TextView::new();
    image_view.set_widget_name("rich-editor");
    let image_window = gtk::Window::builder()
        .default_width(640)
        .default_height(480)
        .child(&image_view)
        .build();
    image_window.present();
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
        let anchor_position = image_buffer.start_iter();
        let serialized_source = buffer_text(&image_buffer);
        anchor_position.child_anchor().is_some_and(|anchor| {
            anchor
                .widgets()
                .first()
                .and_then(|widget| widget.downcast_ref::<gtk::Picture>())
                .is_some_and(|picture| {
                    picture.can_shrink()
                        && picture.content_fit() == gtk::ContentFit::Contain
                        && picture.width_request() == 1
                        && picture.height_request() == 1
                })
        }) && !image_buffer
            .text(&image_buffer.start_iter(), &image_buffer.end_iter(), false)
            .contains("![Pasted image](assets/")
            && serialized_source.starts_with("![Pasted image](assets/")
            && !serialized_source.ends_with('\n')
    }));
    image_window.close();

    let list_view = gtk::TextView::new();
    let list_buffer = list_view.buffer();
    formatting::install_tags(&list_buffer);
    list_buffer.set_text("• first");
    let list_start = list_buffer.start_iter();
    let marker_end = list_buffer.iter_at_offset(2);
    let list_end = list_buffer.end_iter();
    list_buffer.apply_tag_by_name("rich-structural", &list_start, &marker_end);
    list_buffer.apply_tag_by_name("rich-list-bullet", &list_start, &list_end);
    list_buffer.place_cursor(&list_end);
    let list_controller = install_list_continuation(&list_view, &list_buffer);
    list_controller.emit_by_name::<bool>(
        "key-pressed",
        &[
            &gtk::gdk::Key::Return,
            &0_u32,
            &gtk::gdk::ModifierType::empty(),
        ],
    );
    assert_eq!(
        list_buffer.text(&list_buffer.start_iter(), &list_buffer.end_iter(), false),
        "• first\n• "
    );
    list_buffer.insert_at_cursor("second");
    list_controller.emit_by_name::<bool>(
        "key-pressed",
        &[
            &gtk::gdk::Key::Return,
            &0_u32,
            &gtk::gdk::ModifierType::empty(),
        ],
    );
    assert_eq!(
        list_buffer.text(&list_buffer.start_iter(), &list_buffer.end_iter(), false),
        "• first\n• second\n• "
    );
    list_controller.emit_by_name::<bool>(
        "key-pressed",
        &[
            &gtk::gdk::Key::Return,
            &0_u32,
            &gtk::gdk::ModifierType::empty(),
        ],
    );
    assert_eq!(
        list_buffer.text(&list_buffer.start_iter(), &list_buffer.end_iter(), false),
        "• first\n• second\n"
    );

    let shortcut_view = gtk::TextView::new();
    let shortcut_buffer = shortcut_view.buffer();
    formatting::install_tags(&shortcut_buffer);
    let shortcut_controller = install_editor_shortcuts(&shortcut_view, &shortcut_buffer);
    shortcut_buffer.set_text("formatted text");
    shortcut_buffer.select_range(&shortcut_buffer.start_iter(), &shortcut_buffer.end_iter());
    shortcut_controller.emit_by_name::<bool>(
        "key-pressed",
        &[
            &gtk::gdk::Key::b,
            &0_u32,
            &gtk::gdk::ModifierType::CONTROL_MASK,
        ],
    );
    assert!(
        shortcut_buffer
            .start_iter()
            .tags()
            .iter()
            .any(|tag| tag.name().as_deref() == Some("rich-bold"))
    );
    shortcut_controller.emit_by_name::<bool>(
        "key-pressed",
        &[
            &gtk::gdk::Key::i,
            &0_u32,
            &gtk::gdk::ModifierType::CONTROL_MASK,
        ],
    );
    assert!(
        shortcut_buffer
            .start_iter()
            .tags()
            .iter()
            .any(|tag| tag.name().as_deref() == Some("rich-italic"))
    );
    shortcut_buffer.set_text("First item\nSecond item");
    shortcut_buffer.select_range(&shortcut_buffer.start_iter(), &shortcut_buffer.end_iter());
    shortcut_controller.emit_by_name::<bool>(
        "key-pressed",
        &[
            &gtk::gdk::Key::_8,
            &0_u32,
            &(gtk::gdk::ModifierType::CONTROL_MASK | gtk::gdk::ModifierType::SHIFT_MASK),
        ],
    );
    assert_eq!(buffer_text(&shortcut_buffer), "- First item\n- Second item");

    let source_shortcut_view = gtk::TextView::new();
    let source_shortcut_buffer = source_shortcut_view.buffer();
    let source_shortcut_controller =
        install_source_shortcuts(&source_shortcut_view, &source_shortcut_buffer);
    source_shortcut_buffer.set_text("source shortcut");
    source_shortcut_buffer.select_range(
        &source_shortcut_buffer.start_iter(),
        &source_shortcut_buffer.end_iter(),
    );
    source_shortcut_controller.emit_by_name::<bool>(
        "key-pressed",
        &[
            &gtk::gdk::Key::b,
            &0_u32,
            &gtk::gdk::ModifierType::CONTROL_MASK,
        ],
    );
    assert_eq!(buffer_text(&source_shortcut_buffer), "*source shortcut*");

    let rich_roundtrip_view = gtk::TextView::new();
    let rich_roundtrip_buffer = rich_roundtrip_view.buffer();
    let source = "# Carve WYSIWYG Demo\n\nThis is a *WYSIWYG editor* that outputs /Carve markup/.\n\n## Inline marks\n\n- *Strong* → `*text*`\n- /Emphasis/ → `/text/`\n- _Underline_ → `_text_`\n- =Highlight= → `=text=`\n- ~Strike~ → `~text~`\n- {+Inserted+} → `{+text+}`\n- {-Deleted-} → `{-text-}`\n- [HTML]{abbr=\"HyperText Markup Language\"} → `[HTML]{abbr=\"...\"}`\n\n## Task list\n\n- [x] Task lists round-trip to `- [x]`\n- [ ] Toggle the checkbox and watch the source\n\n> Edit the content and watch the Carve source below.\n\n```php\necho \"Hello, Carve!\";\n```";
    render_rich_markup(&rich_roundtrip_view, &rich_roundtrip_buffer, source, None);
    assert_eq!(buffer_text(&rich_roundtrip_buffer), source);

    let stack = gtk::Stack::new();
    let split_view = adw::NavigationSplitView::new();
    let browser_toast_overlay = adw::ToastOverlay::new();
    let browser = build_browser(&state, &stack, &split_view, &browser_toast_overlay);
    stack.add_named(&browser, Some("browser"));
    let editor_placeholder = gtk::Box::new(gtk::Orientation::Vertical, 0);
    stack.add_named(&editor_placeholder, Some("editor"));
    let trash_overlay = adw::ToastOverlay::new();
    let trash_page = build_trash(&state, &stack, &trash_overlay);
    stack.add_named(&trash_page, Some("trash"));
    stack.set_visible_child_name("browser");
    let app_menu = widget_as::<gtk::MenuButton>(&browser, "app-menu-button");
    let content_clamp = widget_as::<adw::Clamp>(&browser, "browser-content-clamp");
    let browser_pages = widget_as::<gtk::Stack>(&browser, "browser-content-pages");
    let browser_status = widget_as::<adw::StatusPage>(&browser, "browser-empty-status");
    let browser_empty_new_note =
        widget_as::<gtk::Button>(&browser, "browser-empty-new-note-button");
    assert!(
        app_menu.is_some()
            && content_clamp.is_some()
            && browser_pages.is_some()
            && browser_status.is_some()
            && browser_empty_new_note.is_some()
    );
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
    assert!(run_main_context_until(|| {
        stack.visible_child_name().as_deref() == Some("editor")
            && state.current_note.borrow().is_some()
    }));
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
    assert!(run_main_context_until(|| {
        find_widget(note_list.upcast_ref(), &format!("note:{}", created.id)).is_some()
    }));
    assert!(
        widget_as::<gtk::MenuButton>(note_list.upcast_ref(), &format!("note-menu:{}", created.id),)
            .is_some()
    );
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
    assert!(run_main_context_until(|| {
        stack.visible_child_name().as_deref() == Some("editor")
    }));

    let search = widget_as::<gtk::SearchEntry>(&browser, "note-search-entry");
    assert!(search.is_some());
    let Some(search) = search else {
        return Ok(());
    };
    search.set_text("Untitled");
    search.emit_by_name::<()>("search-changed", &[]);
    assert_eq!(state.search_query.borrow().as_str(), "Untitled");
    assert!(run_main_context_until(|| note_row_count(&browser) == 2));
    search.set_text("does-not-exist");
    search.emit_by_name::<()>("search-changed", &[]);
    assert!(run_main_context_until(|| note_row_count(&browser) == 0));
    assert!(run_main_context_until(|| {
        browser_pages
            .as_ref()
            .and_then(gtk::Stack::visible_child_name)
            .as_deref()
            == Some("empty")
            && browser_status
                .as_ref()
                .is_some_and(|status| status.title() == "No matching notes")
            && browser_empty_new_note
                .as_ref()
                .is_some_and(|button| !button.is_visible())
    }));
    search.set_text("");
    search.emit_by_name::<()>("search-changed", &[]);

    let sidebar = build_sidebar(&state, &split_view);
    let test_window = gtk::Window::new();
    test_window.set_default_size(1_400, 900);
    let test_layout = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    test_layout.append(&sidebar);
    test_layout.append(&stack);
    test_window.set_child(Some(&test_layout));
    test_window.present();
    let category_list = widget_as::<gtk::ListBox>(&sidebar, "category-list");
    assert!(category_list.is_some());
    let Some(category_list) = category_list else {
        return Ok(());
    };
    state.client.create_category("Projects")?;
    refresh_sidebar(&state);
    assert_eq!(state.client.categories()?.len(), 2);
    let second_category = state.client.categories()?[1].clone();
    assert!(run_main_context_until(|| {
        find_widget(
            category_list.upcast_ref(),
            &format!("category-count:{}", second_category.id),
        )
        .is_some()
    }));
    let count_label = widget_as::<gtk::Label>(
        category_list.upcast_ref(),
        &format!("category-count:{}", second_category.id),
    );
    assert_eq!(
        count_label.map(|label| label.text().to_string()),
        Some("0 notes".to_owned())
    );
    let all_notes_count = widget_as::<gtk::Label>(category_list.upcast_ref(), "all-notes-count");
    let mut active_note_count = 0;
    for category in state.client.categories()? {
        active_note_count += state.client.note_count(category.id)?;
    }
    assert_eq!(
        all_notes_count.map(|label| label.text().to_string()),
        Some(if active_note_count == 1 {
            "1 note".to_owned()
        } else {
            format!("{active_note_count} notes")
        })
    );
    let move_dialog = show_move_note_dialog(
        &state,
        created.id,
        created.category_id,
        &created.title,
        &browser_toast_overlay,
        Some(&test_window),
    );
    let move_search = widget_as::<gtk::SearchEntry>(move_dialog.upcast_ref(), "move-note-search");
    let move_categories =
        widget_as::<gtk::ListBox>(move_dialog.upcast_ref(), "move-note-category-list");
    assert!(move_search.is_some() && move_categories.is_some());
    let Some(move_search) = move_search else {
        return Ok(());
    };
    let Some(move_categories) = move_categories else {
        return Ok(());
    };
    assert!(run_main_context_until(|| move_categories
        .first_child()
        .is_some()));
    move_search.set_text("does-not-exist");
    move_search.emit_by_name::<()>("search-changed", &[]);
    assert!(run_main_context_until(|| {
        move_categories
            .first_child()
            .and_then(|row| row.downcast::<gtk::ListBoxRow>().ok())
            .and_then(|row| row.child())
            .and_then(|child| child.downcast::<gtk::Label>().ok())
            .is_some_and(|label| label.text() == "No categories found")
    }));
    move_dialog.close();
    let source_category = state.client.categories()?[0].clone();
    move_note_to_category(
        &state,
        created.id,
        source_category.id,
        second_category.clone(),
        &browser_toast_overlay,
    );
    assert!(run_main_context_until(|| {
        state
            .client
            .note(created.id)
            .ok()
            .flatten()
            .is_some_and(|note| note.category_id == second_category.id)
            && widget_as::<gtk::Label>(
                category_list.upcast_ref(),
                &format!("category-count:{}", second_category.id),
            )
            .is_some_and(|count| count.text() == "1 note")
    }));
    move_note_to_category(
        &state,
        created.id,
        second_category.id,
        source_category,
        &browser_toast_overlay,
    );
    assert!(run_main_context_until(|| {
        state
            .client
            .note(created.id)
            .ok()
            .flatten()
            .is_some_and(|note| note.category_id != second_category.id)
            && widget_as::<gtk::Label>(
                category_list.upcast_ref(),
                &format!("category-count:{}", second_category.id),
            )
            .is_some_and(|count| count.text() == "0 notes")
    }));
    rename_category(&state, second_category.id, "Personal")?;
    refresh_sidebar(&state);
    assert_eq!(state.client.categories()?[1].name, "Personal");
    assert!(run_main_context_until(|| {
        find_widget(
            category_list.upcast_ref(),
            &format!("category:{}", second_category.id),
        )
        .is_some()
    }));
    let category_row = find_widget(
        category_list.upcast_ref(),
        &format!("category:{}", second_category.id),
    )
    .and_then(|row| row.downcast::<gtk::ListBoxRow>().ok());
    assert!(category_row.is_some());
    let Some(category_row) = category_row else {
        return Ok(());
    };
    category_list.select_row(Some(&category_row));
    assert_eq!(state.selected_category.get(), Some(second_category.id));
    assert!(run_main_context_until(|| {
        browser_pages
            .as_ref()
            .and_then(gtk::Stack::visible_child_name)
            .as_deref()
            == Some("empty")
            && browser_status
                .as_ref()
                .is_some_and(|status| status.title() == "No notes in Personal")
            && browser_empty_new_note
                .as_ref()
                .is_some_and(gtk::prelude::WidgetExt::is_visible)
    }));
    assert!(category_row.has_css_class("category-actions-visible"));
    let actions = widget_as::<gtk::Box>(
        category_row.upcast_ref(),
        &format!("category-actions:{}", second_category.id),
    );
    assert!(
        actions
            .as_ref()
            .is_some_and(gtk::prelude::WidgetExt::is_visible)
    );
    let home_row = category_list.row_at_index(0);
    assert!(home_row.is_some());
    let Some(home_row) = home_row else {
        return Ok(());
    };
    category_list.select_row(Some(&home_row));
    assert_eq!(state.selected_category.get(), None);
    assert!(
        actions
            .as_ref()
            .is_some_and(|actions| !actions.is_visible())
    );

    let delete = widget_as::<gtk::Button>(
        category_row.upcast_ref(),
        &format!("delete-category:{}", second_category.id),
    );
    assert!(delete.is_some());
    let Some(delete) = delete else {
        return Ok(());
    };
    assert!(delete.has_css_class("flat"));
    let confirmed = Rc::new(Cell::new(false));
    let confirmed_for_dialog = Rc::clone(&confirmed);
    let confirmation = category_trash_dialog("Personal", move || confirmed_for_dialog.set(true));
    assert!(confirmation.has_response("trash"));
    assert_eq!(
        confirmation.response_appearance("trash"),
        adw::ResponseAppearance::Destructive
    );
    confirmation.emit_by_name::<()>("response", &[&"cancel"]);
    assert!(!confirmed.get());
    confirmation.emit_by_name::<()>("response", &[&"trash"]);
    assert!(confirmed.get());
    category_list.select_row(Some(&category_row));
    assert!(run_main_context_until(|| {
        browser_empty_new_note
            .as_ref()
            .is_some_and(gtk::prelude::WidgetExt::is_visible)
    }));
    let empty_new_note = browser_empty_new_note.as_ref();
    assert!(empty_new_note.is_some());
    let Some(empty_new_note) = empty_new_note else {
        return Ok(());
    };
    empty_new_note.emit_clicked();
    assert!(run_main_context_until(|| {
        state
            .client
            .note_count(second_category.id)
            .is_ok_and(|count| count == 1)
            && stack.visible_child_name().as_deref() == Some("editor")
            && widget_as::<gtk::Label>(
                category_list.upcast_ref(),
                &format!("category-count:{}", second_category.id),
            )
            .is_some_and(|count| count.text() == "1 note")
    }));
    state.current_note.replace(state.client.note(created.id)?);
    state.client.trash_category(second_category.id)?;
    refresh_sidebar(&state);
    assert!(run_main_context_until(|| {
        state
            .client
            .categories()
            .is_ok_and(|categories| categories.len() == 1)
            && find_widget(
                category_list.upcast_ref(),
                &format!("category:{}", second_category.id),
            )
            .is_none()
    }));

    let toggle_categories = widget_as::<gtk::ToggleButton>(&browser, "toggle-categories-button");
    assert!(toggle_categories.is_some());
    let Some(toggle_categories) = toggle_categories else {
        return Ok(());
    };
    toggle_categories.emit_clicked();
    assert!(
        split_view.is_collapsed() && split_view.shows_content() && !toggle_categories.is_active()
    );
    toggle_categories.emit_clicked();
    assert!(
        !split_view.is_collapsed() && split_view.shows_content() && toggle_categories.is_active()
    );

    stack.remove(&editor_placeholder);
    state.config.borrow_mut().editor.source_split_view = true;
    let editor = build_editor(&state, &stack, &adw::ToastOverlay::new(), &split_view);
    stack.add_named(&editor, Some("editor"));
    stack.set_visible_child_name("editor");
    let rich = widget_as::<gtk::TextView>(&editor, "rich-editor");
    let source = widget_as::<gtk::TextView>(&editor, "source-editor");
    let rich_mode = widget_as::<gtk::ToggleButton>(&editor, "editor-mode-rich");
    let source_mode = widget_as::<gtk::ToggleButton>(&editor, "editor-mode-source");
    let rendered_mode = widget_as::<gtk::ToggleButton>(&editor, "editor-mode-rendered");
    let back = widget_as::<gtk::Button>(&editor, "back-to-notes-button");
    let bold = widget_as::<gtk::Button>(&editor, "format-bold-button");
    let bullet = widget_as::<gtk::Button>(&editor, "format-bullet-button");
    let ordered = widget_as::<gtk::Button>(&editor, "format-ordered-button");
    let heading = widget_as::<gtk::MenuButton>(&editor, "format-heading-button");
    let rich_inline_code = widget_as::<gtk::Button>(&editor, "format-code-button");
    let source_bold = widget_as::<gtk::Button>(&editor, "source-format-bold-button");
    let source_bullet = widget_as::<gtk::Button>(&editor, "source-format-bullet-button");
    let source_inline_code = widget_as::<gtk::Button>(&editor, "source-format-code-button");
    let rich_code_block = widget_as::<gtk::Button>(&editor, "format-code-block-button");
    let source_code_block = widget_as::<gtk::Button>(&editor, "source-format-code-block-button");
    let more_formatting = widget_as::<gtk::MenuButton>(&editor, "format-more-button");
    let source_more_formatting = widget_as::<gtk::MenuButton>(&editor, "source-format-more-button");
    let mode_switcher = widget_as::<gtk::Box>(&editor, "editor-mode-switcher");
    let formatting_bar = widget_as::<gtk::Stack>(&editor, "formatting-toolbar");
    let editor_toggle_categories =
        widget_as::<gtk::ToggleButton>(&editor, "editor-toggle-categories-button");
    let split_preview = widget_as::<gtk::ToggleButton>(&editor, "source-split-toggle");
    let trash = widget_as::<gtk::Button>(&editor, "delete-note-button");
    assert!(
        rich.is_some()
            && source.is_some()
            && rich_mode.is_some()
            && source_mode.is_some()
            && rendered_mode.is_some()
            && back.is_some()
            && bold.is_some()
            && bullet.is_some()
            && ordered.is_some()
            && heading.is_some()
            && rich_inline_code.is_some()
            && source_bold.is_some()
            && source_bullet.is_some()
            && source_inline_code.is_some()
            && rich_code_block.is_some()
            && source_code_block.is_some()
            && more_formatting.is_some()
            && source_more_formatting.is_some()
            && mode_switcher.is_some()
            && formatting_bar.is_some()
            && editor_toggle_categories.is_some()
            && split_preview.is_some()
            && trash.is_some()
    );
    assert_eq!(
        rich_inline_code
            .as_ref()
            .and_then(gtk::prelude::ButtonExt::icon_name)
            .as_deref(),
        Some("text-editor-symbolic")
    );
    assert_eq!(
        rich_code_block
            .as_ref()
            .and_then(gtk::prelude::ButtonExt::icon_name)
            .as_deref(),
        Some("utilities-terminal-symbolic")
    );
    assert_eq!(
        source_inline_code
            .as_ref()
            .and_then(gtk::prelude::ButtonExt::icon_name)
            .as_deref(),
        Some("text-editor-symbolic")
    );
    assert_eq!(
        source_code_block
            .as_ref()
            .and_then(gtk::prelude::ButtonExt::icon_name)
            .as_deref(),
        Some("utilities-terminal-symbolic")
    );
    let (
        Some(rich),
        Some(source),
        Some(rich_mode),
        Some(source_mode),
        Some(rendered_mode),
        Some(back),
        Some(bold),
        Some(bullet),
        Some(ordered),
        Some(heading),
        Some(source_bold),
        Some(source_bullet),
        Some(more_formatting),
        Some(source_more_formatting),
        Some(mode_switcher),
        Some(formatting_bar),
        Some(editor_toggle_categories),
        Some(split_preview),
        Some(trash),
    ) = (
        rich,
        source,
        rich_mode,
        source_mode,
        rendered_mode,
        back,
        bold,
        bullet,
        ordered,
        heading,
        source_bold,
        source_bullet,
        more_formatting,
        source_more_formatting,
        mode_switcher,
        formatting_bar,
        editor_toggle_categories,
        split_preview,
        trash,
    )
    else {
        return Ok(());
    };
    let mut ancestor = mode_switcher.parent();
    let mut is_in_header_bar = false;
    while let Some(parent) = ancestor {
        if parent.is::<adw::HeaderBar>() {
            is_in_header_bar = true;
            break;
        }
        ancestor = parent.parent();
    }
    assert!(is_in_header_bar);
    assert_eq!(
        rich_mode.tooltip_text().as_deref(),
        Some("Edit with rich text")
    );
    assert_eq!(
        source_mode.tooltip_text().as_deref(),
        Some("Edit Carve markup")
    );
    assert_eq!(
        rendered_mode.tooltip_text().as_deref(),
        Some("Read-only preview")
    );
    assert!(formatting_bar.is_sensitive());
    assert_eq!(
        editor_toggle_categories.icon_name().as_deref(),
        Some("sidebar-show-symbolic")
    );
    split_view.set_collapsed(true);
    assert!(!editor_toggle_categories.is_active());
    editor_toggle_categories.emit_clicked();
    assert!(!split_view.is_collapsed());
    assert!(!split_preview.is_sensitive());
    assert_eq!(
        more_formatting.icon_name().as_deref(),
        Some("view-more-symbolic")
    );
    assert_eq!(
        source_more_formatting.icon_name().as_deref(),
        Some("view-more-symbolic")
    );
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
        "format me"
    );
    assert!(
        rich.buffer()
            .start_iter()
            .tags()
            .iter()
            .any(|tag| tag.name().as_deref() == Some("rich-bold"))
    );
    assert_eq!(
        bullet.icon_name().as_deref(),
        Some("view-list-bullet-symbolic")
    );
    assert_eq!(
        ordered.icon_name().as_deref(),
        Some("view-list-ordered-symbolic")
    );
    assert_eq!(
        heading.icon_name().as_deref(),
        Some("format-text-rich-symbolic")
    );
    source_mode.set_active(true);
    assert!(state.source_mode.get());
    assert_eq!(state.config.borrow().editor.last_mode, EditorMode::Source);
    assert!(split_preview.is_sensitive());
    assert!(split_preview.is_active());
    let source_split = widget_as::<gtk::Paned>(&editor, "source-split-view");
    assert!(source_split.is_some());
    let Some(source_split) = source_split else {
        return Ok(());
    };
    let split_rendered_preview = source_split.end_child();
    assert!(
        split_rendered_preview
            .as_ref()
            .is_some_and(gtk::prelude::WidgetExt::is_visible)
    );
    // Isolate the editor from the fixed test sidebar. This supplies it a real
    // narrow allocation, just as the adaptive main layout does in the app.
    stack.remove(&editor);
    let narrow_window = gtk::Window::builder()
        .default_width(640)
        .default_height(900)
        .child(&editor)
        .build();
    narrow_window.present();
    assert!(run_main_context_until(|| {
        !split_preview.is_sensitive()
            && split_rendered_preview
                .as_ref()
                .is_some_and(|preview| !preview.is_visible())
    }));
    assert!(split_preview.is_active());
    narrow_window.set_child(Option::<&gtk::Widget>::None);
    narrow_window.close();
    let restored_window = gtk::Window::builder()
        .default_width(1_400)
        .default_height(900)
        .child(&editor)
        .build();
    restored_window.present();
    assert!(run_main_context_until(|| {
        split_preview.is_active()
            && split_preview.is_sensitive()
            && split_rendered_preview
                .as_ref()
                .is_some_and(gtk::prelude::WidgetExt::is_visible)
    }));
    restored_window.set_child(Option::<&gtk::Widget>::None);
    restored_window.close();
    stack.add_named(&editor, Some("editor"));
    stack.set_visible_child_name("editor");
    assert_eq!(
        source.buffer().text(
            &source.buffer().start_iter(),
            &source.buffer().end_iter(),
            false
        ),
        "*format me*"
    );
    source.buffer().set_text("source text");
    source
        .buffer()
        .select_range(&source.buffer().start_iter(), &source.buffer().end_iter());
    source_bold.emit_clicked();
    assert_eq!(buffer_text(&source.buffer()), "*source text*");
    source
        .buffer()
        .select_range(&source.buffer().start_iter(), &source.buffer().end_iter());
    source_bullet.emit_clicked();
    assert_eq!(buffer_text(&source.buffer()), "- *source text*");
    split_preview.set_active(true);
    assert!(split_preview.is_active());
    assert!(state.config.borrow().editor.source_split_view);
    source.buffer().set_text("# Persisted by mode switch");
    rendered_mode.set_active(true);
    assert!(state.rendered_mode.get());
    assert_eq!(state.config.borrow().editor.last_mode, EditorMode::Rendered);
    assert!(!split_preview.is_sensitive());
    assert!(state.config.borrow().editor.source_split_view);
    source_mode.set_active(true);
    assert_eq!(buffer_text(&source.buffer()), "# Persisted by mode switch");
    assert!(split_preview.is_active());
    rich_mode.set_active(true);
    assert!(!state.source_mode.get());
    assert_eq!(state.config.borrow().editor.last_mode, EditorMode::Rich);
    rich.buffer().set_text("A bullet from the toolbar");
    let end = rich.buffer().end_iter();
    rich.buffer().place_cursor(&end);
    bullet.emit_clicked();
    source_mode.set_active(true);
    assert_eq!(
        source.buffer().text(
            &source.buffer().start_iter(),
            &source.buffer().end_iter(),
            false
        ),
        "- A bullet from the toolbar"
    );
    rich_mode.set_active(true);
    rich.buffer()
        .set_text("First item\nSecond item\nThird item");
    let selected_start = rich.buffer().start_iter();
    let selected_end = rich.buffer().end_iter();
    rich.buffer().select_range(&selected_start, &selected_end);
    bullet.emit_clicked();
    let first_line_start = rich.buffer().start_iter();
    let mut first_line_end = first_line_start;
    first_line_end.forward_to_line_end();
    rich.buffer()
        .remove_tag_by_name("rich-list-bullet", &first_line_start, &first_line_end);
    source_mode.set_active(true);
    assert_eq!(
        source.buffer().text(
            &source.buffer().start_iter(),
            &source.buffer().end_iter(),
            false
        ),
        "- First item\n- Second item\n- Third item"
    );
    rich_mode.set_active(true);
    rich.buffer().set_text("A number from the toolbar");
    let end = rich.buffer().end_iter();
    rich.buffer().place_cursor(&end);
    ordered.emit_clicked();
    source_mode.set_active(true);
    assert_eq!(
        source.buffer().text(
            &source.buffer().start_iter(),
            &source.buffer().end_iter(),
            false
        ),
        "1. A number from the toolbar"
    );
    rich_mode.set_active(true);
    rich.buffer().set_text("Carver");
    let link_start = rich.buffer().start_iter();
    let link_end = rich.buffer().end_iter();
    formatting::apply_link_tag(
        &rich.buffer(),
        &link_start,
        &link_end,
        "https://example.com",
    );
    source_mode.set_active(true);
    assert_eq!(
        source.buffer().text(
            &source.buffer().start_iter(),
            &source.buffer().end_iter(),
            false
        ),
        "[Carver](https://example.com)"
    );
    source
        .buffer()
        .set_text("|= Unsupported table |\n| Cell | ");
    rich_mode.set_active(true);
    assert!(rendered_mode.is_active());
    assert!(state.rendered_mode.get());
    // Automatic fallback uses Preview but preserves the explicit Edit choice
    // for the next app launch.
    assert_eq!(state.config.borrow().editor.last_mode, EditorMode::Rich);
    rich_mode.set_active(true);
    source_mode.set_active(true);
    source.buffer().set_text("# Project\n- first\n1. second");
    rich_mode.set_active(true);
    assert_eq!(
        rich.buffer().text(
            &rich.buffer().start_iter(),
            &rich.buffer().end_iter(),
            false
        ),
        "Project\n• first\n1. second"
    );
    assert!(
        rich.buffer()
            .start_iter()
            .tags()
            .iter()
            .any(|tag| tag.name().as_deref() == Some("rich-heading-1"))
    );
    source_mode.set_active(true);
    assert_eq!(
        source.buffer().text(
            &source.buffer().start_iter(),
            &source.buffer().end_iter(),
            false
        ),
        "# Project\n- first\n1. second"
    );
    source.buffer().set_text("# Heading 2\n\n# h2 testttttttttt\n\nthis is a note blabla hello world\n\n![Pasted image](assets/managed.png)\n\n*blabla*\n\n*glalala*");
    rich_mode.set_active(true);
    assert!(rich_mode.is_active());
    assert!(!state.rendered_mode.get());
    rich_mode.set_active(true);
    rich.buffer()
        .set_text("First paragraph\n\nSecond paragraph\n");
    source_mode.set_active(true);
    assert_eq!(
        source.buffer().text(
            &source.buffer().start_iter(),
            &source.buffer().end_iter(),
            false
        ),
        "First paragraph\n\nSecond paragraph\n"
    );
    source
        .buffer()
        .set_text("First paragraph\n\nSecond paragraph\n");
    rich_mode.set_active(true);
    source_mode.set_active(true);
    assert_eq!(
        source.buffer().text(
            &source.buffer().start_iter(),
            &source.buffer().end_iter(),
            false
        ),
        "First paragraph\n\nSecond paragraph\n"
    );
    source.buffer().set_text("# Edited from source mode");
    let client = state.client.clone();
    assert!(run_main_context_until(|| {
        client
            .note(created.id)
            .ok()
            .flatten()
            .is_some_and(|note| note.source == "# Edited from source mode")
    }));
    assert!(run_main_context_until(|| {
        state
            .current_note
            .borrow()
            .as_ref()
            .is_some_and(|note| note.source == "# Edited from source mode")
    }));
    let note_before_back = state.current_note.borrow().clone();
    assert!(note_before_back.is_some());
    let Some(note_before_back) = note_before_back else {
        return Ok(());
    };
    search.set_text("");
    search.emit_by_name::<()>("search-changed", &[]);
    back.emit_clicked();
    assert!(run_main_context_until(|| {
        stack.visible_child_name().as_deref() == Some("browser")
            && state
                .client
                .note(created.id)
                .ok()
                .flatten()
                .is_some_and(|note| note.source == "# Edited from source mode")
    }));
    assert_eq!(
        state.client.note(created.id)?.map(|note| note.source),
        Some("# Edited from source mode".to_owned())
    );
    let note_after_back = state.client.note(created.id)?;
    assert_eq!(
        note_after_back.as_ref().map(|note| note.updated_at),
        Some(note_before_back.updated_at)
    );
    assert_eq!(
        note_after_back.as_ref().map(|note| note.revision),
        Some(note_before_back.revision)
    );
    assert!(run_main_context_until(|| {
        find_widget(&browser, &format!("note-title:{}", created.id))
            .and_then(|widget| widget.downcast::<gtk::Label>().ok())
            .is_some_and(|label| label.text() == "Edited from source mode")
    }));
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
    assert!(run_main_context_until(|| {
        state
            .client
            .note(created.id)
            .ok()
            .flatten()
            .is_some_and(|note| note.trashed_at.is_some())
    }));
    let open_trash = widget_as::<gtk::Button>(&sidebar, "open-trash-button");
    assert!(open_trash.is_some());
    let Some(open_trash) = open_trash else {
        return Ok(());
    };
    open_trash.emit_clicked();
    assert_eq!(stack.visible_child_name().as_deref(), Some("trash"));
    assert!(category_list.selected_row().is_none());
    assert!(run_main_context_until(|| {
        widget_as::<gtk::Button>(
            &trash_page,
            &format!("restore-category:{}", second_category.id),
        )
        .is_some()
    }));
    let restore_category = widget_as::<gtk::Button>(
        &trash_page,
        &format!("restore-category:{}", second_category.id),
    );
    assert!(restore_category.is_some());
    let Some(restore_category) = restore_category else {
        return Ok(());
    };
    restore_category.emit_clicked();
    assert!(run_main_context_until(|| {
        state
            .client
            .categories()
            .is_ok_and(|categories| categories.len() == 2)
    }));
    assert!(run_main_context_until(|| {
        widget_as::<gtk::Button>(&trash_page, &format!("restore-note:{}", created.id)).is_some()
    }));
    let restore_note =
        widget_as::<gtk::Button>(&trash_page, &format!("restore-note:{}", created.id));
    assert!(restore_note.is_some());
    let Some(restore_note) = restore_note else {
        return Ok(());
    };
    restore_note.emit_clicked();
    assert!(run_main_context_until(|| {
        state
            .client
            .note(created.id)
            .ok()
            .flatten()
            .is_some_and(|note| note.trashed_at.is_none())
    }));
    state.client.trash_category(second_category.id)?;
    refresh_trash(&state);
    let empty_trash = widget_as::<gtk::Button>(&trash_page, "empty-trash-button");
    assert!(run_main_context_until(|| {
        empty_trash
            .as_ref()
            .is_some_and(gtk::prelude::WidgetExt::is_sensitive)
    }));
    state.client.empty_trash()?;
    refresh_trash(&state);
    assert!(state.client.trash_contents()?.is_empty());
    assert_eq!(state.client.categories()?.len(), 1);
    test_window.close();
    Ok(())
}
