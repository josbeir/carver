//! Display-backed interaction coverage for the MVU window surface.

use std::{cell::Cell, rc::Rc, time::Duration};

use carver_config::{Config, SourceSyntaxStyle};
use gtk::gio::prelude::FileExt;
use gtk::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::{ActionRowExt, AdwDialogExt, ComboRowExt, PreferencesRowExt};
use sourceview5::prelude::*;
use webkit6::prelude::*;

use super::support::{TestResult, find_widget, run_main_context_until, test_state, widget_as};

#[test]
#[ignore = "requires a graphical display; CI runs it under headless Weston"]
#[expect(
    clippy::too_many_lines,
    reason = "one display-backed scenario covers MVU surface transitions on GTK's single initialization thread"
)]
fn mvu_window_should_keep_sidebar_and_browser_card_presentation() -> TestResult {
    gtk::disable_portals();
    glib::set_application_name("Carver test");
    gtk::init()?;
    crate::app::load_styles();
    let display = gtk::gdk::Display::default().ok_or("display")?;
    assert!(
        gtk::IconTheme::for_display(&display).has_icon("carver-agent-codex-symbolic"),
        "registered agent icons should be discoverable by GTK's icon theme"
    );
    assert!(
        gtk::IconTheme::for_display(&display).has_icon("package-x-generic-symbolic"),
        "the Adwaita package icon should be available to the category picker"
    );
    assert!(
        gtk::IconTheme::for_display(&display).has_icon("bookmark-new-symbolic"),
        "the Adwaita bookmark icon should be available to the category picker"
    );
    assert!(
        gtk::IconTheme::for_display(&display).has_icon("x-office-calendar-symbolic"),
        "the Adwaita calendar icon should be available to the category picker"
    );
    assert!(
        gtk::IconTheme::for_display(&display).has_icon("system-users-symbolic"),
        "the Adwaita people icon should be available to the category picker"
    );
    crate::mvu::tests::runtime_should_render_and_complete_each_initial_resource()?;
    crate::mvu::tests::runtime_should_refresh_visible_resources_after_a_separate_client_mutates_the_library()?;
    crate::editor::source_commands::tests::gtk_source_commands_cover_selection_and_block_operations(
    );
    let (temporary_directory, client) = test_state()?;
    let category = client.create_category("Notes")?;
    let destination = client.create_category("Projects")?;
    let application = adw::Application::new(
        Some("io.github.josbeir.Carver.Tests"),
        gtk::gio::ApplicationFlags::empty(),
    );
    application.register(None::<&gtk::gio::Cancellable>)?;
    let config_path = temporary_directory.path().join("config.toml");
    let mut config = Config::default();
    config.editor.autosave_delay_ms = 1;
    config.editor.source_line_numbers = true;
    config.editor.source_highlight_current_line = true;
    config.editor.source_syntax_style = SourceSyntaxStyle::WritingFocus;
    let window =
        crate::app::build_window_for_test(&application, client.clone(), &config, &config_path)?;
    let (preferences_dialog, about_dialog) = crate::dialogs::present_dialogs_for_test(
        &window,
        &config,
        &crate::mvu::AppDispatcher::default(),
    );
    assert_eq!(
        about_dialog.application_icon(),
        crate::app::APPLICATION_ICON
    );
    assert_eq!(
        about_dialog.issue_url(),
        "https://github.com/josbeir/carver/issues"
    );
    assert!(window.lookup_action("connect-agent").is_some());
    let agent_setup = crate::dialogs::show_agent_setup_dialog_for_test(&window);
    let agent = widget_as::<adw::ComboRow>(agent_setup.upcast_ref(), "agent-setup-agent")
        .ok_or("agent setup selection")?;
    let allow_agent_write =
        widget_as::<adw::SwitchRow>(agent_setup.upcast_ref(), "agent-setup-allow-write")
            .ok_or("agent write switch")?;
    let agent_command =
        widget_as::<adw::ActionRow>(agent_setup.upcast_ref(), "agent-setup-command")
            .ok_or("agent setup command")?;
    let claude_card = widget_as::<adw::ActionRow>(agent_setup.upcast_ref(), "agent-card-1")
        .ok_or("Claude Code agent card")?;
    claude_card.emit_by_name::<()>("activated", &[]);
    assert_eq!(agent.selected(), 1);
    assert!(
        agent_command
            .subtitle()
            .is_some_and(|command| command.contains("claude mcp add"))
    );
    allow_agent_write.set_active(true);
    assert!(
        agent_command
            .subtitle()
            .is_some_and(|command| command.contains("--allow-write"))
    );
    agent_setup.close();
    assert_eq!(
        widget_as::<adw::SwitchRow>(preferences_dialog.upcast_ref(), "remote-images-setting")
            .map(|row| row.subtitle()),
        Some(Some(
            "Download images referenced by notes when they are displayed.".into()
        ))
    );
    let formatting_toolbar_setting = widget_as::<adw::SwitchRow>(
        preferences_dialog.upcast_ref(),
        "formatting-toolbar-setting",
    )
    .ok_or("formatting toolbar setting")?;
    assert!(formatting_toolbar_setting.is_active());
    assert_eq!(
        formatting_toolbar_setting.subtitle(),
        Some("Show formatting controls at the bottom of the editor.".into())
    );
    let mut purist_config = config.clone();
    purist_config.editor.show_formatting_toolbar = false;
    let purist_window = crate::app::build_window_for_test(
        &application,
        client.clone(),
        &purist_config,
        &temporary_directory.path().join("purist-config.toml"),
    )?;
    let purist_root = purist_window.child().ok_or("purist window content")?;
    assert!(
        widget_as::<gtk::Box>(&purist_root, "formatting-toolbar-bar")
            .is_some_and(|toolbar_bar| !toolbar_bar.is_visible())
    );
    purist_window.close();
    assert_eq!(
        widget_as::<adw::SwitchRow>(
            preferences_dialog.upcast_ref(),
            "source-line-numbers-setting"
        )
        .map(|row| row.subtitle()),
        Some(Some(
            "Show source line positions in the editor gutter.".into()
        ))
    );
    assert_eq!(
        widget_as::<adw::SwitchRow>(
            preferences_dialog.upcast_ref(),
            "source-current-line-setting"
        )
        .map(|row| row.subtitle()),
        Some(Some(
            "Shade the line containing the cursor in Source mode.".into()
        ))
    );
    let syntax_style = widget_as::<adw::ComboRow>(
        preferences_dialog.upcast_ref(),
        "source-syntax-style-setting",
    )
    .ok_or("source syntax style setting")?;
    assert_eq!(syntax_style.title(), "Syntax style");
    assert_eq!(
        syntax_style.subtitle(),
        Some("Choose how much markup colour appears in Source mode.".into())
    );
    assert_eq!(syntax_style.selected(), 1);
    assert_eq!(
        widget_as::<adw::ActionRow>(preferences_dialog.upcast_ref(), "source-font-setting")
            .map(|row| row.title()),
        Some("Source font".into())
    );
    assert!(
        widget_as::<gtk::Label>(preferences_dialog.upcast_ref(), "source-font-value").is_some()
    );
    assert!(
        widget_as::<adw::ActionRow>(preferences_dialog.upcast_ref(), "source-font-reset-row")
            .is_some_and(|row| !row.is_visible())
    );
    let source_font =
        widget_as::<adw::ActionRow>(preferences_dialog.upcast_ref(), "source-font-setting")
            .ok_or("source font setting")?;
    source_font.emit_by_name::<()>("activated", &[]);
    let mut custom_font_config = config.clone();
    custom_font_config.editor.source_font = Some("Adwaita Mono 13".to_owned());
    let (custom_font_preferences, _) = crate::dialogs::present_dialogs_for_test(
        &window,
        &custom_font_config,
        &crate::mvu::AppDispatcher::default(),
    );
    let reset_font = widget_as::<adw::ActionRow>(
        custom_font_preferences.upcast_ref(),
        "source-font-reset-row",
    )
    .ok_or("source font reset")?;
    assert!(reset_font.is_visible());
    reset_font.emit_by_name::<()>("activated", &[]);
    assert!(!reset_font.is_visible());
    assert_eq!(
        window.icon_name().as_deref(),
        Some(crate::app::APPLICATION_ICON)
    );
    let root = window.child().ok_or("window content")?;
    let sidebar = widget_as::<gtk::ListBox>(&root, "category-list").ok_or("category list")?;
    assert!(widget_as::<gtk::Button>(&root, "new-category-button").is_some());
    let settings_menu = widget_as::<gtk::MenuButton>(&root, "sidebar-settings-menu-button")
        .ok_or("sidebar settings menu")?;
    assert_eq!(
        settings_menu.menu_model().map(|model| model.n_items()),
        Some(4)
    );
    assert!(widget_as::<gtk::MenuButton>(&root, "app-menu-button").is_none());
    assert!(window.lookup_action("keyboard-shortcuts").is_some());
    let window_controllers = window.observe_controllers();
    let window_shortcuts = (0..window_controllers.n_items())
        .filter_map(|index| window_controllers.item(index))
        .filter_map(|controller| controller.downcast::<gtk::EventControllerKey>().ok())
        .find(|controller| controller.name().as_deref() == Some("window-shortcuts"))
        .ok_or("window shortcuts")?;
    assert_eq!(
        window_shortcuts.propagation_phase(),
        gtk::PropagationPhase::Capture
    );
    let keyboard_shortcuts = crate::dialogs::show_keyboard_shortcuts_dialog(&window);
    assert_eq!(
        keyboard_shortcuts.widget_name(),
        "keyboard-shortcuts-dialog"
    );
    keyboard_shortcuts.close();
    assert!(run_main_context_until(|| {
        find_widget(sidebar.upcast_ref(), &format!("category:{}", category.id)).is_some()
    }));
    let browser_view =
        widget_as::<adw::ToolbarView>(&root, "browser-surface").ok_or("browser view")?;
    let browser_controllers = browser_view.observe_controllers();
    let browser_shortcuts = (0..browser_controllers.n_items())
        .filter_map(|index| browser_controllers.item(index))
        .filter_map(|controller| controller.downcast::<gtk::EventControllerKey>().ok())
        .find(|controller| controller.name().as_deref() == Some("browser-shortcuts"))
        .ok_or("browser shortcuts")?;
    assert_eq!(
        browser_shortcuts.propagation_phase(),
        gtk::PropagationPhase::Capture
    );
    assert!(widget_as::<gtk::Button>(&root, "new-note-button").is_some());
    assert!(widget_as::<gtk::Button>(&root, "import-note-button").is_some());
    assert!(window.lookup_action("import-note").is_some());
    assert_eq!(
        crate::dialogs::import_format_for_file(&gtk::gio::File::for_path("import.crv")),
        Some(carver_sdk::DocumentImportFormat::Carve)
    );
    assert_eq!(
        crate::dialogs::import_format_for_file(&gtk::gio::File::for_path("import.md")),
        Some(carver_sdk::DocumentImportFormat::Markdown)
    );
    crate::dialogs::read_import_file(
        &gtk::gio::File::for_path("unsupported.txt"),
        crate::mvu::AppDispatcher::default(),
    );
    assert_eq!(
        crate::dialogs::import_message_from_bytes(
            carver_sdk::DocumentImportFormat::Markdown,
            b"# Imported",
        ),
        crate::mvu::NavigationMsg::ImportNote {
            format: carver_sdk::DocumentImportFormat::Markdown,
            source: String::from("# Imported"),
        }
    );
    assert!(matches!(
        crate::dialogs::import_message_from_bytes(carver_sdk::DocumentImportFormat::Carve, &[0xff],),
        crate::mvu::NavigationMsg::ImportFailed(_)
    ));
    crate::dialogs::show_import_file_dialog(
        window.upcast_ref(),
        crate::mvu::AppDispatcher::default(),
    );
    let new_note_handled = browser_shortcuts.emit_by_name::<bool>(
        "key-pressed",
        &[
            &gtk::gdk::Key::n,
            &0_u32,
            &gtk::gdk::ModifierType::CONTROL_MASK,
        ],
    );
    assert!(new_note_handled);
    assert!(run_main_context_until(|| client
        .recent_notes(None, 10, 0)
        .is_ok_and(|notes| notes.len() == 1)));
    let note = client
        .recent_notes(None, 10, 0)?
        .pop()
        .ok_or("created note")?;
    assert!(run_main_context_until(|| all_notes_row(&sidebar).is_some()));
    let all_notes = all_notes_row(&sidebar).ok_or("all notes row")?;
    sidebar.select_row(Some(&all_notes));
    assert!(run_main_context_until(|| {
        find_widget(&root, &format!("note-category:{}", note.id)).is_some()
            && find_widget(&root, &format!("note-updated:{}", note.id)).is_some()
            && find_widget(&root, &format!("note-excerpt:{}", note.id)).is_none()
    }));
    let note_menu = widget_as::<gtk::MenuButton>(&root, &format!("note-menu:{}", note.id))
        .ok_or("note actions")?;
    let move_button = note_menu
        .popover()
        .and_then(|popover| popover.child())
        .and_then(|actions| find_widget(&actions, &format!("move-note-button:{}", note.id)))
        .and_downcast::<gtk::Button>()
        .ok_or("move note action")?;
    move_button.emit_clicked();
    let move_search =
        widget_as::<gtk::SearchEntry>(&root, "move-note-search").ok_or("move picker search")?;
    let source_row = find_widget(&root, &format!("move-note-category:{}", category.id))
        .and_downcast::<gtk::ListBoxRow>()
        .ok_or("current move category")?;
    assert!(
        source_row
            .child()
            .and_downcast::<gtk::Button>()
            .is_some_and(|button| !button.is_sensitive())
    );
    move_search.set_text("missing");
    assert!(run_main_context_until(|| {
        find_widget(&root, &format!("move-note-category:{}", destination.id)).is_none()
    }));
    move_search.set_text("pro");
    let destination_row = find_widget(&root, &format!("move-note-category:{}", destination.id))
        .and_downcast::<gtk::ListBoxRow>()
        .ok_or("filtered move destination")?;
    let destination_button = destination_row
        .child()
        .and_downcast::<gtk::Button>()
        .ok_or("move destination button")?;
    assert!(destination_button.is_sensitive());
    destination_button.emit_clicked();
    assert!(run_main_context_until(|| client
        .note(note.id)
        .ok()
        .flatten()
        .is_some_and(|moved| moved.category_id == destination.id)));
    assert!(run_main_context_until(|| {
        find_widget(sidebar.upcast_ref(), &format!("category:{}", category.id)).is_some()
    }));
    let category_row = find_widget(sidebar.upcast_ref(), &format!("category:{}", category.id))
        .and_downcast::<gtk::ListBoxRow>()
        .ok_or("category row")?;
    sidebar.select_row(Some(&category_row));
    assert!(run_main_context_until(|| {
        widget_as::<gtk::Label>(&root, "browser-hero-title")
            .is_some_and(|title| title.text() == "Notes")
            && widget_as::<gtk::Button>(&root, "edit-selected-category-button").is_some()
            && widget_as::<gtk::Button>(&root, "trash-selected-category-button").is_some()
            && find_widget(
                sidebar.upcast_ref(),
                &format!("category-actions:{}", category.id),
            )
            .is_none()
            && find_widget(&root, &format!("note-category:{}", note.id)).is_none()
    }));
    let destination_category_row = find_widget(
        sidebar.upcast_ref(),
        &format!("category:{}", destination.id),
    )
    .and_downcast::<gtk::ListBoxRow>()
    .ok_or("destination category row")?;
    sidebar.select_row(Some(&destination_category_row));
    assert!(run_main_context_until(|| {
        widget_as::<gtk::Label>(&root, "browser-hero-title")
            .is_some_and(|title| title.text() == "Projects")
            && find_widget(&root, &format!("note:{}", note.id)).is_some()
            && find_widget(&root, "note-group:today").is_some()
            && find_widget(&root, &format!("note-category:{}", note.id)).is_none()
    }));
    assert!(run_main_context_until(|| all_notes_row(&sidebar).is_some()));
    let all_notes = all_notes_row(&sidebar).ok_or("all notes row")?;
    sidebar.select_row(Some(&all_notes));
    assert!(run_main_context_until(|| {
        widget_as::<gtk::Label>(&root, "browser-hero-title")
            .is_some_and(|title| title.text() == "All notes")
            && widget_as::<gtk::Button>(&root, "edit-selected-category-button").is_none()
            && find_widget(&root, &format!("note-category:{}", note.id)).is_some()
            && find_widget(&root, "note-group:today").is_some()
    }));
    let note_row = find_widget(&root, &format!("note:{}", note.id))
        .and_downcast::<gtk::ListBoxRow>()
        .ok_or("note row")?;
    assert!(note_row.has_css_class("card"));
    assert!(note_row.has_css_class("activatable"));
    note_row.activate();
    let route_stack = widget_as::<gtk::Stack>(&root, "content-route-stack").ok_or("route stack")?;
    assert!(run_main_context_until(|| {
        route_stack.visible_child_name().as_deref() == Some("editor")
    }));
    let controllers = route_stack.observe_controllers();
    let mouse_back = (0..controllers.n_items())
        .filter_map(|index| controllers.item(index))
        .find_map(|controller| controller.downcast::<gtk::EventControllerLegacy>().ok())
        .filter(|controller| controller.name().as_deref() == Some("editor-mouse-back-controller"))
        .ok_or("editor mouse back controller")?;
    assert_eq!(
        mouse_back.propagation_phase(),
        gtk::PropagationPhase::Capture
    );
    let touchpad_back = (0..controllers.n_items())
        .filter_map(|index| controllers.item(index))
        .find_map(|controller| controller.downcast::<gtk::EventControllerScroll>().ok())
        .filter(|controller| {
            controller.name().as_deref() == Some("editor-touchpad-back-controller")
        })
        .ok_or("editor touchpad back controller")?;
    assert_eq!(
        touchpad_back.propagation_phase(),
        gtk::PropagationPhase::Capture
    );
    assert_eq!(
        touchpad_back.flags(),
        gtk::EventControllerScrollFlags::BOTH_AXES
    );
    let source = widget_as::<gtk::TextView>(&root, "source-editor").ok_or("source editor")?;
    let source_view =
        widget_as::<sourceview5::View>(&root, "source-editor").ok_or("GtkSourceView")?;
    let source_buffer = source_view
        .buffer()
        .downcast::<sourceview5::Buffer>()
        .map_err(|_| "GtkSourceBuffer")?;
    assert_eq!(
        source_buffer
            .language()
            .map(|language| language.id().to_string()),
        Some(String::from("carve"))
    );
    let source_scheme = source_buffer
        .style_scheme()
        .map(|scheme| scheme.id().to_string());
    let expected_source_scheme = if adw::StyleManager::default().is_dark() {
        "carve-writing-focus-dark"
    } else {
        "carve-writing-focus-light"
    };
    assert_eq!(source_scheme.as_deref(), Some(expected_source_scheme));
    let source_style_scheme = source_buffer.style_scheme().ok_or("source style scheme")?;
    for (level, expected_scale) in [
        (1, "1.45"),
        (2, "1.30"),
        (3, "1.18"),
        (4, "1.10"),
        (5, "1.04"),
        (6, "1.00"),
    ] {
        let style = source_style_scheme
            .style(&format!("carve:heading-{level}"))
            .ok_or("source heading style")?;
        assert_eq!(style.scale().as_deref(), Some(expected_scale));
    }
    assert!(source_view.shows_line_numbers());
    assert!(source_view.is_highlight_current_line());
    assert!(source_buffer.is_highlight_syntax());
    assert_eq!(source_view.pixels_above_lines(), 3);
    assert_eq!(source_view.pixels_below_lines(), 3);
    let source_mode =
        widget_as::<gtk::ToggleButton>(&root, "editor-mode-source").ok_or("source mode")?;
    let rich_mode = widget_as::<gtk::ToggleButton>(&root, "editor-mode-rich").ok_or("rich mode")?;
    let toolbar =
        widget_as::<gtk::Box>(&root, "formatting-toolbar").ok_or("shared formatting toolbar")?;
    let toolbar_bar =
        widget_as::<gtk::Box>(&root, "formatting-toolbar-bar").ok_or("formatting toolbar bar")?;
    assert!(toolbar_bar.is_visible());
    assert_shared_toolbar_controls(&root)?;
    source_mode.set_active(true);
    assert_eq!(
        toolbar,
        widget_as::<gtk::Box>(&root, "formatting-toolbar").ok_or("source toolbar")?
    );
    assert_shared_toolbar_controls(&root)?;
    let source_controllers = source_view.observe_controllers();
    let source_shortcuts = (0..source_controllers.n_items())
        .filter_map(|index| source_controllers.item(index))
        .filter_map(|controller| controller.downcast::<gtk::EventControllerKey>().ok())
        .find(|controller| controller.name().as_deref() == Some("source-format-shortcuts"))
        .ok_or("source format shortcuts")?;
    source.buffer().set_text("First line");
    source.buffer().place_cursor(&source.buffer().end_iter());
    let handled = source_shortcuts.emit_by_name::<bool>(
        "key-pressed",
        &[
            &gtk::gdk::Key::Return,
            &0_u32,
            &gtk::gdk::ModifierType::SHIFT_MASK,
        ],
    );
    assert!(handled);
    assert_eq!(
        source.buffer().text(
            &source.buffer().start_iter(),
            &source.buffer().end_iter(),
            false
        ),
        "First line\\\n"
    );
    let copy_note = widget_as::<gtk::Button>(&root, "copy-note-button").ok_or("copy note")?;
    let export_menu =
        widget_as::<gtk::MenuButton>(&root, "export-note-button").ok_or("export menu")?;
    let gtk_window = window.clone().upcast::<gtk::Window>();
    assert_eq!(
        export_menu.menu_model().map(|model| model.n_items()),
        Some(2)
    );
    assert!(
        export_menu
            .popover()
            .and_downcast::<gtk::PopoverMenu>()
            .is_some()
    );
    let export_request = crate::mvu::EditorExportDialogRequest {
        request_id: 91,
        session: crate::mvu::EditorSessionId(1),
        note_id: note.id,
        source: "# Exported note".to_owned(),
        filename_stem: "Exported note".to_owned(),
    };
    let export_options = crate::editor::show_export_options_dialog(
        export_request,
        Some(&gtk_window),
        crate::mvu::AppDispatcher::default(),
    );
    assert_eq!(
        widget_as::<adw::ComboRow>(export_options.upcast_ref(), "export-format-setting")
            .map(|row| row.title()),
        Some("Format".into())
    );
    assert_eq!(
        widget_as::<adw::SwitchRow>(export_options.upcast_ref(), "export-assets-setting")
            .map(|row| row.title()),
        Some("Include managed images".into())
    );
    export_options.emit_by_name::<()>("response", &[&"cancel"]);
    let export_warning = crate::editor::show_export_warning_dialog(
        &crate::mvu::EditorExportWarningRequest {
            request_id: 92,
            session: crate::mvu::EditorSessionId(1),
            warnings: vec!["Markdown cannot preserve this construct".to_owned()],
        },
        Some(&gtk_window),
        crate::mvu::AppDispatcher::default(),
    );
    export_warning.emit_by_name::<()>("response", &[&"cancel"]);
    let export_directory = tempfile::tempdir()?;
    let pdf_path = export_directory.path().join("exported-note.pdf");
    let pdf_uri = gtk::gio::File::for_path(&pdf_path).uri().to_string();
    crate::editor::export_rendered_snapshot(
        "# Exported note\n\nPDF body",
        false,
        false,
        &pdf_uri,
        Some(&gtk_window),
        None,
        crate::mvu::AppDispatcher::default(),
        93,
    );
    assert!(run_main_context_until(|| {
        std::fs::read(&pdf_path).is_ok_and(|bytes| bytes.starts_with(b"%PDF"))
    }));
    assert_native_print_dialog_cancels_without_invalid_window(&gtk_window)?;
    source.buffer().set_text("# Copied note");
    copy_note.emit_clicked();
    let clipboard = source.display().clipboard();
    assert!(run_main_context_until(|| {
        clipboard.formats().contain_mime_type("text/html")
            && clipboard
                .formats()
                .contain_mime_type("text/plain;charset=utf-8")
    }));
    let copied_text = std::rc::Rc::new(std::cell::RefCell::new(None));
    let copied_text_for_callback = std::rc::Rc::clone(&copied_text);
    clipboard.read_text_async(None::<&gtk::gio::Cancellable>, move |result| {
        *copied_text_for_callback.borrow_mut() = result.ok().flatten().map(|text| text.to_string());
    });
    assert!(run_main_context_until(|| copied_text.borrow().is_some()));
    assert_eq!(copied_text.borrow().as_deref(), Some("Copied note\n"));
    let find_bar = widget_as::<gtk::SearchBar>(&root, "editor-find-bar").ok_or("find bar")?;
    let find_entry =
        widget_as::<gtk::SearchEntry>(&root, "editor-find-entry").ok_or("find entry")?;
    let find_count = widget_as::<gtk::Label>(&root, "editor-find-count").ok_or("find count")?;
    let find_next = widget_as::<gtk::Button>(&root, "editor-find-next").ok_or("find next")?;
    let find_previous =
        widget_as::<gtk::Button>(&root, "editor-find-previous").ok_or("find previous")?;
    let find_close = widget_as::<gtk::Button>(&root, "editor-find-close").ok_or("find close")?;
    let editor_view =
        widget_as::<adw::ToolbarView>(&root, "editor-surface").ok_or("editor view")?;
    let controllers = editor_view.observe_controllers();
    let editor_shortcuts = (0..controllers.n_items())
        .filter_map(|index| controllers.item(index))
        .filter_map(|controller| controller.downcast::<gtk::EventControllerKey>().ok())
        .find(|controller| controller.name().as_deref() == Some("editor-window-shortcuts"))
        .ok_or("editor shortcuts")?;
    assert_eq!(
        editor_shortcuts.propagation_phase(),
        gtk::PropagationPhase::Capture
    );
    let find_shortcut = (0..controllers.n_items())
        .filter_map(|index| controllers.item(index))
        .filter_map(|controller| controller.downcast::<gtk::EventControllerKey>().ok())
        .find(|controller| controller.name().as_deref() == Some("editor-find-shortcuts"))
        .ok_or("find shortcut controller")?;
    assert_eq!(
        find_shortcut.propagation_phase(),
        gtk::PropagationPhase::Capture
    );
    source.buffer().set_text("needle one\nNeedle two");
    source.buffer().place_cursor(&source.buffer().start_iter());
    let handled = find_shortcut.emit_by_name::<bool>(
        "key-pressed",
        &[
            &gtk::gdk::Key::f,
            &0_u32,
            &gtk::gdk::ModifierType::CONTROL_MASK,
        ],
    );
    assert!(handled);
    assert!(find_bar.is_search_mode());
    find_entry.set_text("needle");
    assert!(run_main_context_until(|| find_count.text() == "2 matches"));
    assert!(find_next.is_sensitive() && find_previous.is_sensitive());
    assert_eq!(
        source
            .buffer()
            .selection_bounds()
            .map(|(start, end)| source.buffer().text(&start, &end, false).to_string()),
        Some("needle".to_owned())
    );
    find_next.emit_clicked();
    assert_eq!(
        source
            .buffer()
            .selection_bounds()
            .map(|(start, end)| source.buffer().text(&start, &end, false).to_string()),
        Some("Needle".to_owned())
    );
    find_previous.emit_clicked();
    assert_eq!(
        source
            .buffer()
            .selection_bounds()
            .map(|(start, end)| source.buffer().text(&start, &end, false).to_string()),
        Some("needle".to_owned())
    );
    find_close.emit_clicked();
    assert!(!find_bar.is_search_mode());
    source.buffer().set_text("# Source\n\nA paragraph");
    assert!(run_main_context_until(|| client
        .note(note.id)
        .ok()
        .flatten()
        .is_some_and(|saved| saved.source == "# Source\n\nA paragraph")));
    exercise_source_formatting_controls(&root, &source.buffer())?;
    source.buffer().set_text("*fully bold*");
    source_buffer.ensure_highlight(&source.buffer().start_iter(), &source.buffer().end_iter());
    assert!(
        source_buffer.iter_has_context_class(&source.buffer().iter_at_offset(2), "carve-emphasis")
    );
    select_all(&source.buffer());
    assert!(run_main_context_until(|| {
        widget_as::<gtk::ToggleButton>(&root, "format-bold-button")
            .is_some_and(|button| button.is_active())
    }));
    source.buffer().set_text("*bold* plain");
    select_all(&source.buffer());
    assert!(run_main_context_until(|| {
        widget_as::<gtk::ToggleButton>(&root, "format-bold-button")
            .is_some_and(|button| !button.is_active())
    }));
    source.buffer().set_text("# *bold*");
    source
        .buffer()
        .place_cursor(&source.buffer().iter_at_offset(4));
    let source_path = widget_as::<gtk::Label>(&root, "source-ast-path").ok_or("source AST path")?;
    assert!(run_main_context_until(|| {
        source_path.is_visible()
            && source_path.text() == "h1 › bold"
            && widget_as::<gtk::ToggleButton>(&root, "format-bold-button")
                .is_some_and(|button| button.is_active())
    }));
    source_buffer.ensure_highlight(&source.buffer().start_iter(), &source.buffer().end_iter());
    assert!(
        source_buffer.iter_has_context_class(&source.buffer().iter_at_offset(1), "carve-heading")
    );
    source
        .buffer()
        .set_text("---\ntitle: Carve Feature Demo\ndate: 2026-06-02\n---\n\n# Note");
    source
        .buffer()
        .place_cursor(&source.buffer().iter_at_offset(8));
    assert!(run_main_context_until(|| {
        source_path.is_visible() && source_path.text() == "frontmatter"
    }));
    source_buffer.ensure_highlight(&source.buffer().start_iter(), &source.buffer().end_iter());
    assert!(
        source_buffer
            .iter_has_context_class(&source.buffer().iter_at_offset(0), "carve-frontmatter")
    );
    assert!(
        source_buffer
            .iter_has_context_class(&source.buffer().iter_at_offset(4), "carve-frontmatter-key")
    );
    source.buffer().set_text("Before\n\n---\n\n# After");
    let divider_offset = 8;
    let heading_offset = 13;
    source_buffer.ensure_highlight(&source.buffer().start_iter(), &source.buffer().end_iter());
    assert!(source_buffer.iter_has_context_class(
        &source.buffer().iter_at_offset(divider_offset),
        "carve-thematic-break"
    ));
    assert!(source_buffer.iter_has_context_class(
        &source.buffer().iter_at_offset(heading_offset),
        "carve-heading"
    ));
    source
        .buffer()
        .set_text("# One\n## Two\n### Three\n#### Four\n##### Five\n###### Six");
    source_buffer.ensure_highlight(&source.buffer().start_iter(), &source.buffer().end_iter());
    for (offset, class) in [
        (0, "carve-heading-1"),
        (6, "carve-heading-2"),
        (13, "carve-heading-3"),
        (23, "carve-heading-4"),
        (33, "carve-heading-5"),
        (44, "carve-heading-6"),
    ] {
        let iter = source.buffer().iter_at_offset(offset);
        assert!(source_buffer.iter_has_context_class(&iter, "carve-heading"));
        assert!(source_buffer.iter_has_context_class(&iter, class));
    }
    source
        .buffer()
        .set_text(":: Carve\n: A post-Markdown lightweight markup language.");
    source
        .buffer()
        .place_cursor(&source.buffer().iter_at_offset(13));
    assert!(run_main_context_until(|| {
        source_path.is_visible() && source_path.text() == "dl › dd"
    }));
    source_buffer.ensure_highlight(&source.buffer().start_iter(), &source.buffer().end_iter());
    assert!(
        source_buffer
            .iter_has_context_class(&source.buffer().iter_at_offset(1), "carve-definition-term")
    );
    assert!(source_buffer.iter_has_context_class(
        &source.buffer().iter_at_offset(9),
        "carve-definition-marker"
    ));
    source
        .buffer()
        .set_text("|= Fruit |=> Price |=~ Stock |\n| Apple | 1.20 | In |");
    source_buffer.ensure_highlight(&source.buffer().start_iter(), &source.buffer().end_iter());
    assert!(
        source_buffer
            .iter_has_context_class(&source.buffer().iter_at_offset(10), "carve-table-marker")
    );
    assert!(
        source_buffer
            .iter_has_context_class(&source.buffer().iter_at_offset(11), "carve-table-marker")
    );
    assert!(
        !source_buffer.iter_has_context_class(&source.buffer().iter_at_offset(4), "carve-emphasis")
    );
    assert!(
        source_buffer
            .iter_has_context_class(&source.buffer().iter_at_offset(39), "carve-table-marker")
    );
    source.buffer().set_text("1. list item");
    source
        .buffer()
        .place_cursor(&source.buffer().iter_at_offset(4));
    assert!(run_main_context_until(|| {
        source_path.is_visible() && source_path.text() == "ol › li"
    }));
    source.buffer().set_text("plain text");
    source
        .buffer()
        .place_cursor(&source.buffer().iter_at_offset(3));
    assert!(run_main_context_until(|| {
        source_path.is_visible() && source_path.text() == "p"
    }));
    let rendered_mode =
        widget_as::<gtk::ToggleButton>(&root, "editor-mode-rendered").ok_or("rendered mode")?;
    rendered_mode.set_active(true);
    assert!(run_main_context_until(|| !toolbar.is_sensitive()
        && !source_path.is_visible()
        && !find_bar.is_search_mode()));
    source_mode.set_active(true);
    assert!(run_main_context_until(|| toolbar.is_sensitive()));
    assert_split_preview_tracks_source_scroll(&root, &source, &source_mode)?;
    source.buffer().set_text("rich find target");
    rich_mode.set_active(true);
    let rich = widget_as::<webkit6::WebView>(&root, "rich-editor").ok_or("rich editor")?;
    let rich_source = std::rc::Rc::new(std::cell::RefCell::new(None));
    let rich_source_for_callback = std::rc::Rc::clone(&rich_source);
    rich.evaluate_javascript(
        "window.carverEditor?.source() ?? null",
        None,
        None,
        None::<&gtk::gio::Cancellable>,
        move |result| {
            *rich_source_for_callback.borrow_mut() =
                result.ok().map(|value| value.to_str().clone());
        },
    );
    assert!(run_main_context_until(|| rich_source.borrow().is_some()));
    assert_eq!(rich_source.borrow().as_deref(), Some("rich find target"));
    find_bar.set_search_mode(true);
    find_entry.set_text("target");
    assert!(run_main_context_until(|| find_count.text() == "1 match"));
    find_close.emit_clicked();
    assert!(!find_bar.is_search_mode());
    let bold = widget_as::<gtk::ToggleButton>(&root, "format-bold-button").ok_or("bold")?;
    rich.evaluate_javascript(
        "window.carverEditor.command('bold')",
        None,
        None,
        None::<&gtk::gio::Cancellable>,
        |_| {},
    );
    assert!(run_main_context_until(|| bold.is_active()));
    rich.evaluate_javascript(
        "window.carverEditor.command('bold')",
        None,
        None,
        None::<&gtk::gio::Cancellable>,
        |_| {},
    );
    assert!(run_main_context_until(|| !bold.is_active()));
    source_mode.set_active(true);
    source
        .buffer()
        .set_text("![First](assets/first.png){width=\"50%\"}");
    rich_mode.set_active(true);
    let image_width = std::rc::Rc::new(std::cell::RefCell::new(None));
    let image_width_callback = std::rc::Rc::clone(&image_width);
    rich.evaluate_javascript(
        "document.querySelector('#editor img')?.style.width ?? null",
        None,
        None,
        None::<&gtk::gio::Cancellable>,
        move |result| {
            *image_width_callback.borrow_mut() = result.ok().map(|value| value.to_str().clone());
        },
    );
    assert!(run_main_context_until(|| image_width.borrow().is_some()));
    assert_eq!(image_width.borrow().as_deref(), Some("50%"));
    rich.evaluate_javascript(
        "window.carverEditor.insertImage('assets/second.png')",
        None,
        None,
        None::<&gtk::gio::Cancellable>,
        |_| {},
    );
    assert!(run_main_context_until(|| source
        .buffer()
        .text(
            &source.buffer().start_iter(),
            &source.buffer().end_iter(),
            false
        )
        .contains("assets/second.png")));
    source_mode.set_active(true);
    source
        .buffer()
        .set_text("# Searchable note\n\nBrowser grouping");
    assert!(run_main_context_until(|| client
        .note(note.id)
        .ok()
        .flatten()
        .is_some_and(
            |saved| saved.source == "# Searchable note\n\nBrowser grouping"
        )));
    let open_trash = widget_as::<gtk::Button>(&root, "open-trash-button").ok_or("open trash")?;
    open_trash.emit_clicked();
    assert!(run_main_context_until(|| {
        route_stack.visible_child_name().as_deref() == Some("trash")
    }));
    let trash_back =
        widget_as::<gtk::Button>(&root, "back-from-trash-button").ok_or("trash back")?;
    trash_back.emit_clicked();
    assert!(run_main_context_until(|| {
        route_stack.visible_child_name().as_deref() == Some("browser")
    }));
    let search = widget_as::<gtk::SearchEntry>(&root, "note-search-entry").ok_or("search")?;
    assert!(run_main_context_until(|| {
        find_widget(&root, &format!("note:{}", note.id)).is_some()
            && find_widget(&root, "note-group:today")
                .and_downcast::<gtk::ListBoxRow>()
                .is_some_and(|row| !row.is_selectable())
    }));
    search.set_text("Searchable");
    assert!(run_main_context_until(|| {
        find_widget(&root, &format!("note:{}", note.id)).is_some()
            && find_widget(&root, "note-group:today").is_none()
    }));
    search.set_text("not-present");
    assert!(run_main_context_until(|| widget_as::<gtk::Box>(
        &root,
        "browser-search-empty-card"
    )
    .is_some_and(|card| card.is_visible())));
    search.set_text("");
    assert!(run_main_context_until(|| widget_as::<gtk::Box>(
        &root,
        "browser-search-empty-card"
    )
    .is_some_and(|card| !card.is_visible())));
    assert!(run_main_context_until(|| {
        find_widget(&root, &format!("note:{}", note.id)).is_some()
            && find_widget(&root, "note-group:today").is_some()
    }));
    let moved_note_row = find_widget(&root, &format!("note:{}", note.id))
        .and_downcast::<gtk::ListBoxRow>()
        .ok_or("moved note row")?;
    moved_note_row.activate();
    assert!(run_main_context_until(|| {
        route_stack.visible_child_name().as_deref() == Some("editor")
    }));
    let delete_note = widget_as::<gtk::Button>(&root, "delete-note-button").ok_or("delete note")?;
    assert_eq!(
        delete_note.tooltip_text().as_deref(),
        Some("Move Note to Trash (Ctrl+D)")
    );
    let delete_handled = editor_shortcuts.emit_by_name::<bool>(
        "key-pressed",
        &[
            &gtk::gdk::Key::d,
            &0_u32,
            &gtk::gdk::ModifierType::CONTROL_MASK,
        ],
    );
    assert!(delete_handled);
    assert!(run_main_context_until(|| client
        .note(note.id)
        .ok()
        .flatten()
        .is_some_and(|deleted| deleted.trashed_at.is_some())));
    assert!(run_main_context_until(|| {
        widget_as::<gtk::Button>(&root, &format!("restore-note:{}", note.id)).is_some()
    }));
    let restore_note = widget_as::<gtk::Button>(&root, &format!("restore-note:{}", note.id))
        .ok_or("restore note")?;
    assert!(restore_note.is_visible());
    assert!(restore_note.has_css_class("flat"));
    assert_eq!(restore_note.tooltip_text().as_deref(), Some("Restore"));
    restore_note.emit_clicked();
    assert!(run_main_context_until(|| client
        .note(note.id)
        .ok()
        .flatten()
        .is_some_and(|restored| restored.trashed_at.is_none())));
    window.close();
    Ok(())
}

fn exercise_source_formatting_controls(
    editor: &gtk::Widget,
    buffer: &gtk::TextBuffer,
) -> TestResult {
    for name in [
        "format-bold-button",
        "format-italic-button",
        "format-strike-button",
        "format-underline-button",
        "format-highlight-button",
        "format-superscript-button",
        "format-subscript-button",
        "format-bullet-button",
        "format-ordered-button",
        "format-task-button",
        "format-code-button",
        "format-code-block-button",
    ] {
        buffer.set_text("format me");
        select_all(buffer);
        widget_as::<gtk::ToggleButton>(editor, name)
            .ok_or(name)?
            .emit_clicked();
    }
    buffer.set_text("Level 1\nLevel 2\nLevel 3\nLevel 4");
    select_all(buffer);
    widget_as::<gtk::ToggleButton>(editor, "format-ordered-button")
        .ok_or("format-ordered-button")?
        .emit_clicked();
    if buffer.text(&buffer.start_iter(), &buffer.end_iter(), false)
        != "1. Level 1\n2. Level 2\n3. Level 3\n4. Level 4"
    {
        return Err("source ordered-list serialization".into());
    }
    let heading = widget_as::<gtk::MenuButton>(editor, "format-heading-button")
        .ok_or("format heading picker")?;
    let choices = heading
        .popover()
        .and_then(|popover| popover.child())
        .ok_or("formatting choices")?;
    let mut child = choices.first_child();
    while let Some(widget) = child {
        child = widget.next_sibling();
        buffer.set_text("format me");
        select_all(buffer);
        widget
            .downcast::<gtk::ToggleButton>()
            .map_err(|_| "formatting choice")?
            .emit_clicked();
    }
    let table =
        widget_as::<gtk::MenuButton>(editor, "format-table-button").ok_or("source table picker")?;
    let table_content = table
        .popover()
        .and_then(|popover| popover.child())
        .and_downcast::<gtk::Box>()
        .ok_or("source table picker content")?;
    let grid = table_content
        .first_child()
        .and_then(|dimensions| dimensions.next_sibling())
        .and_downcast::<gtk::Grid>()
        .ok_or("source table size grid")?;
    buffer.set_text("");
    grid.first_child()
        .and_downcast::<gtk::Button>()
        .ok_or("source table first cell")?
        .emit_clicked();
    if !buffer
        .text(&buffer.start_iter(), &buffer.end_iter(), false)
        .contains("|= |")
    {
        return Err("source table picker insert".into());
    }
    Ok(())
}

fn assert_native_print_dialog_cancels_without_invalid_window(parent: &gtk::Window) -> TestResult {
    let cancelled = Rc::new(Cell::new(false));
    let cancelled_for_timeout = Rc::clone(&cancelled);
    let parent_weak = parent.downgrade();
    let attempts = Rc::new(Cell::new(0_u8));
    let attempts_for_timeout = Rc::clone(&attempts);
    glib::timeout_add_local(Duration::from_millis(20), move || {
        let Some(parent) = parent_weak.upgrade() else {
            return glib::ControlFlow::Break;
        };
        let dialog = gtk::Window::list_toplevels()
            .into_iter()
            .filter_map(|widget| widget.downcast::<gtk::Window>().ok())
            .find(|candidate| {
                candidate.transient_for().is_some_and(|host| {
                    host.transient_for()
                        .is_some_and(|ancestor| ancestor == parent)
                })
            });
        if let Some(dialog) = dialog {
            cancelled_for_timeout.set(true);
            dialog.close();
            return glib::ControlFlow::Break;
        }
        if attempts_for_timeout.get() == 50 {
            return glib::ControlFlow::Break;
        }
        attempts_for_timeout.update(|attempt| attempt + 1);
        glib::ControlFlow::Continue
    });
    crate::editor::export_rendered_snapshot(
        "# Printable note\n\nBody",
        false,
        true,
        "",
        Some(parent),
        None,
        crate::mvu::AppDispatcher::default(),
        94,
    );
    if !run_main_context_until(|| cancelled.get()) {
        return Err("native print dialog did not appear".into());
    }
    Ok(())
}

fn assert_shared_toolbar_controls(root: &gtk::Widget) -> TestResult {
    for name in [
        "format-bold-button",
        "format-italic-button",
        "format-strike-button",
        "format-underline-button",
        "format-highlight-button",
        "format-superscript-button",
        "format-subscript-button",
        "format-code-button",
        "format-code-block-button",
        "format-bullet-button",
        "format-ordered-button",
        "format-task-button",
        "format-link-button",
    ] {
        widget_as::<gtk::ToggleButton>(root, name).ok_or(name)?;
    }
    for name in [
        "format-heading-button",
        "format-table-button",
        "format-image-button",
    ] {
        widget_as::<gtk::MenuButton>(root, name).ok_or(name)?;
    }
    Ok(())
}

fn select_all(buffer: &gtk::TextBuffer) {
    let start = buffer.start_iter();
    let end = buffer.end_iter();
    buffer.select_range(&start, &end);
}

fn all_notes_row(sidebar: &gtk::ListBox) -> Option<gtk::ListBoxRow> {
    sidebar.first_child().and_downcast::<gtk::ListBoxRow>()
}

fn assert_split_preview_tracks_source_scroll(
    editor: &gtk::Widget,
    source: &gtk::TextView,
    source_mode: &gtk::ToggleButton,
) -> TestResult {
    source_mode.set_active(true);
    source.buffer().set_text(
        &(0..180)
            .map(|index| format!("Paragraph {index}"))
            .collect::<Vec<_>>()
            .join("\n\n"),
    );
    let split_toggle =
        widget_as::<gtk::ToggleButton>(editor, "source-split-toggle").ok_or("split toggle")?;
    split_toggle.set_active(true);
    let source_scroll = source
        .parent()
        .and_downcast::<gtk::ScrolledWindow>()
        .ok_or("source scroll")?;
    let adjustment = source_scroll.vadjustment();
    assert!(run_main_context_until(
        || adjustment.upper() > adjustment.page_size()
    ));
    adjustment.set_value((adjustment.upper() - adjustment.page_size()) * 0.5);
    let scroll_before_format = adjustment.value();
    let paragraph = source.buffer().text(
        &source.buffer().start_iter(),
        &source.buffer().end_iter(),
        false,
    );
    let cursor = paragraph.find("Paragraph 90").ok_or("formatting cursor")?;
    let cursor = i32::try_from(cursor).map_err(|_| "formatting cursor offset")?;
    source
        .buffer()
        .place_cursor(&source.buffer().iter_at_offset(cursor));
    let bold =
        widget_as::<gtk::ToggleButton>(editor, "format-bold-button").ok_or("source bold button")?;
    bold.emit_clicked();
    assert!(run_main_context_until(|| {
        (adjustment.value() - scroll_before_format).abs() < 1.0
            && source
                .buffer()
                .text(
                    &source.buffer().start_iter(),
                    &source.buffer().end_iter(),
                    false,
                )
                .contains("**Paragraph 90")
    }));
    split_toggle.set_active(false);
    Ok(())
}
