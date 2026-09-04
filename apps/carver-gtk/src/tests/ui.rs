//! Display-backed interaction coverage for the MVU window surface.

use carver_config::Config;
use gtk::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::{ActionRowExt, PreferencesRowExt};
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
    gtk::init()?;
    crate::mvu::tests::runtime_should_render_and_complete_each_initial_resource()?;
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
    assert_eq!(
        widget_as::<adw::SwitchRow>(preferences_dialog.upcast_ref(), "remote-images-setting")
            .map(|row| row.subtitle()),
        Some(Some(
            "Download images referenced by notes when they are displayed.".into()
        ))
    );
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
    assert_eq!(
        widget_as::<adw::SwitchRow>(
            preferences_dialog.upcast_ref(),
            "source-syntax-highlighting-setting"
        )
        .map(|row| row.subtitle()),
        Some(Some("Colour Carve markup in Source mode.".into()))
    );
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
    assert!(widget_as::<gtk::MenuButton>(&root, "sidebar-settings-menu-button").is_some());
    assert!(widget_as::<gtk::MenuButton>(&root, "app-menu-button").is_none());
    assert!(run_main_context_until(|| {
        find_widget(sidebar.upcast_ref(), &format!("category:{}", category.id)).is_some()
    }));
    let new_note = widget_as::<gtk::Button>(&root, "new-note-button").ok_or("new note button")?;
    new_note.emit_clicked();
    assert!(run_main_context_until(|| client
        .recent_notes(None, 10, 0)
        .is_ok_and(|notes| notes.len() == 1)));
    let note = client
        .recent_notes(None, 10, 0)?
        .pop()
        .ok_or("created note")?;
    let all_notes = sidebar
        .first_child()
        .and_downcast::<gtk::ListBoxRow>()
        .ok_or("refreshed all notes row")?;
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
        widget_as::<adw::WindowTitle>(&root, "browser-window-title")
            .is_some_and(|title| title.title() == "Notes")
            && find_widget(&root, &format!("note-category:{}", note.id)).is_none()
    }));
    let all_notes = sidebar
        .first_child()
        .and_downcast::<gtk::ListBoxRow>()
        .ok_or("refreshed all notes row")?;
    sidebar.select_row(Some(&all_notes));
    assert!(run_main_context_until(|| {
        find_widget(&root, &format!("note-category:{}", note.id)).is_some()
    }));
    let note_row = find_widget(&root, &format!("note:{}", note.id))
        .and_downcast::<gtk::ListBoxRow>()
        .ok_or("note row")?;
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
        "carve-dark"
    } else {
        "carve-light"
    };
    assert_eq!(source_scheme.as_deref(), Some(expected_source_scheme));
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
    assert_shared_toolbar_controls(&root)?;
    source_mode.set_active(true);
    assert_eq!(
        toolbar,
        widget_as::<gtk::Box>(&root, "formatting-toolbar").ok_or("source toolbar")?
    );
    assert_shared_toolbar_controls(&root)?;
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
    assert!(run_main_context_until(
        || !toolbar.is_sensitive() && !source_path.is_visible()
    ));
    source_mode.set_active(true);
    assert!(run_main_context_until(|| toolbar.is_sensitive()));
    assert_split_preview_tracks_source_scroll(&root, &source, &source_mode)?;
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
    }));
    let moved_note_row = find_widget(&root, &format!("note:{}", note.id))
        .and_downcast::<gtk::ListBoxRow>()
        .ok_or("moved note row")?;
    moved_note_row.activate();
    assert!(run_main_context_until(|| {
        route_stack.visible_child_name().as_deref() == Some("editor")
    }));
    widget_as::<gtk::Button>(&root, "delete-note-button")
        .ok_or("delete note")?
        .emit_clicked();
    assert!(run_main_context_until(|| client
        .note(note.id)
        .ok()
        .flatten()
        .is_some_and(|deleted| deleted.trashed_at.is_some())));
    assert!(run_main_context_until(|| {
        widget_as::<gtk::Button>(&root, &format!("restore-note:{}", note.id)).is_some()
    }));
    widget_as::<gtk::Button>(&root, &format!("restore-note:{}", note.id))
        .ok_or("restore note")?
        .emit_clicked();
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
