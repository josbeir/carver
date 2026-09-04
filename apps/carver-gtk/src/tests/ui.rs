//! Browser, sidebar, and `WebKit` editor integration coverage run under Weston.

use gtk::prelude::*;
use libadwaita as adw;
use webkit6::prelude::*;

use carver_config::AppPaths;

use crate::{
    browser::build_browser,
    controller::rename_category,
    dialogs::category_trash_dialog,
    editor::build_editor,
    note_move::{move_note_to_category, show_move_note_dialog},
    sidebar::{build_sidebar, refresh_sidebar, sidebar_toggle_button},
    trash::{build_trash, refresh_trash},
};

use super::support::{TestResult, find_widget, run_main_context_until, test_state, widget_as};

#[test]
#[ignore = "requires a graphical display; CI runs it under headless Weston"]
#[expect(
    clippy::too_many_lines,
    reason = "one UI scenario intentionally exercises the live surface handoffs"
)]
fn gtk_surfaces_cover_navigation_and_web_editor_host() -> TestResult {
    gtk::init()?;
    crate::mvu::tests::runtime_should_render_and_complete_each_initial_resource()?;
    crate::editor::source_commands::tests::gtk_source_commands_cover_selection_and_block_operations(
    );
    let (_temporary_directory, state) = test_state()?;
    state.config.borrow_mut().editor.autosave_delay_ms = 1;
    let stack = gtk::Stack::new();
    stack.set_hexpand(true);
    let split_view = adw::NavigationSplitView::new();
    let toast = adw::ToastOverlay::new();
    let browser = build_browser(&state, &stack, &split_view, &toast);
    let (editor, editor_view) = build_editor(&state, &toast, &split_view).into_parts();
    let trash = build_trash(&state, &stack);
    stack.add_named(&browser, Some("browser"));
    stack.add_named(&editor, Some("editor"));
    stack.add_named(&trash, Some("trash"));
    stack.set_visible_child_name("browser");
    let sidebar = build_sidebar(&state, &split_view);
    let layout = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    layout.append(&sidebar);
    layout.append(&stack);
    let window = gtk::Window::builder()
        .default_width(1800)
        .default_height(800)
        .child(&layout)
        .build();
    window.present();

    let new_note =
        widget_as::<gtk::Button>(&browser, "new-note-button").ok_or("new note button")?;
    new_note.emit_clicked();
    assert!(run_main_context_until(|| state
        .current_note
        .borrow()
        .is_some()));
    let note = state.current_note.borrow().clone().ok_or("created note")?;
    assert!(run_main_context_until(|| {
        find_widget(browser.upcast_ref(), &format!("note-category:{}", note.id)).is_some()
    }));
    assert!(find_widget(browser.upcast_ref(), &format!("note-updated:{}", note.id)).is_some());

    let category_list =
        widget_as::<gtk::ListBox>(&sidebar, "category-list").ok_or("category list")?;
    assert!(run_main_context_until(|| category_list
        .first_child()
        .is_some()));
    let home_row = category_list
        .first_child()
        .and_downcast::<gtk::ListBoxRow>()
        .ok_or("all notes row")?;
    let home_content = home_row.child().ok_or("all notes content")?;
    let home_controllers = home_content.observe_controllers();
    let home_click = (0..home_controllers.n_items())
        .find_map(|index| {
            home_controllers
                .item(index)
                .and_downcast::<gtk::GestureClick>()
        })
        .ok_or("all notes click handler")?;
    home_click.emit_by_name::<()>("released", &[&1_i32, &0.0_f64, &0.0_f64]);
    assert!(run_main_context_until(|| {
        stack.visible_child_name().as_deref() == Some("browser")
    }));

    state.client.create_category("Projects")?;
    refresh_sidebar(&state);
    let second = state.client.categories()?[1].clone();
    assert!(run_main_context_until(|| find_widget(
        category_list.upcast_ref(),
        &format!("category:{}", second.id)
    )
    .is_some()));
    let project_row = find_widget(
        category_list.upcast_ref(),
        &format!("category:{}", second.id),
    )
    .and_downcast::<gtk::ListBoxRow>()
    .ok_or("project category row")?;
    category_list.select_row(Some(&project_row));
    assert!(run_main_context_until(
        || state.selected_category.get() == Some(second.id)
    ));
    let sidebar_toggle = sidebar_toggle_button(&split_view, "coverage-sidebar-toggle");
    sidebar_toggle.set_active(false);
    assert!(split_view.is_collapsed());
    sidebar_toggle.set_active(true);
    assert!(!split_view.is_collapsed());
    move_note_to_category(&state, note.id, note.category_id, second.clone(), &toast);
    assert!(run_main_context_until(|| state
        .client
        .note(note.id)
        .ok()
        .flatten()
        .is_some_and(|note| note.category_id == second.id)));

    crate::app::install_mvu_runtime_for_test(&state, editor_view);
    crate::dialogs::install_trash_actions_for_test(&window, &state);
    assert!(run_main_context_until(|| {
        state
            .mvu_model()
            .is_some_and(|model| matches!(model.sidebar.state, crate::mvu::LoadState::Ready(_)))
    }));
    assert!(run_main_context_until(|| find_widget(
        category_list.upcast_ref(),
        &format!("category:{}", second.id)
    )
    .is_some()));
    let runtime_project_row = find_widget(
        category_list.upcast_ref(),
        &format!("category:{}", second.id),
    )
    .and_downcast::<gtk::ListBoxRow>()
    .ok_or("MVU project category row")?;
    category_list.select_row(Some(&runtime_project_row));
    assert!(run_main_context_until(|| {
        state.selected_category.get() == Some(second.id)
            && state
                .browser_title
                .borrow()
                .as_ref()
                .is_some_and(|title| title.title() == "Projects")
    }));
    assert!(runtime_project_row.has_css_class("category-card"));
    assert!(
        find_widget(
            category_list.upcast_ref(),
            &format!("rename-category:{}", second.id)
        )
        .is_some()
    );
    assert!(find_widget(category_list.upcast_ref(), "all-notes-count").is_some());
    let all_notes_row = category_list
        .first_child()
        .and_downcast::<gtk::ListBoxRow>()
        .ok_or("MVU all notes row")?;
    let browser_pages =
        widget_as::<gtk::Stack>(&browser, "browser-content-pages").ok_or("browser pages")?;
    category_list.select_row(Some(&all_notes_row));
    assert_eq!(
        browser_pages.visible_child_name().as_deref(),
        Some("contents"),
        "category reloads should retain visible rows instead of flashing a status page"
    );
    assert!(run_main_context_until(|| {
        state.selected_category.get().is_none()
            && find_widget(browser.upcast_ref(), &format!("note-category:{}", note.id)).is_some()
            && find_widget(browser.upcast_ref(), &format!("note-updated:{}", note.id)).is_some()
    }));
    state.current_note.take();
    let note_row = find_widget(browser.upcast_ref(), &format!("note:{}", note.id))
        .and_downcast::<gtk::ListBoxRow>()
        .ok_or("MVU note row")?;
    note_row.activate();
    assert!(run_main_context_until(|| {
        state
            .mvu_model()
            .and_then(|model| model.editor)
            .is_some_and(|document| document.note_id == note.id)
            && stack.visible_child_name().as_deref() == Some("editor")
    }));
    let mvu_source =
        widget_as::<gtk::TextView>(&editor, "source-editor").ok_or("MVU source editor")?;
    let mvu_buffer = mvu_source.buffer();
    assert_eq!(
        mvu_buffer.text(&mvu_buffer.start_iter(), &mvu_buffer.end_iter(), false),
        note.source
    );
    let editor_session = state
        .mvu_model()
        .and_then(|model| model.editor)
        .map(|document| document.session)
        .ok_or("MVU editor session")?;
    let _ = state.dispatch_mvu(crate::mvu::AppMsg::Editor(crate::mvu::EditorMsg::Close(
        editor_session,
    )));

    let search = widget_as::<gtk::SearchEntry>(&browser, "note-search-entry").ok_or("search")?;
    search.set_text("not-present");
    search.emit_by_name::<()>("search-changed", &[]);
    let search_empty_card =
        widget_as::<gtk::Box>(&browser, "browser-search-empty-card").ok_or("search empty card")?;
    assert!(run_main_context_until(|| search_empty_card.is_visible()));
    assert!(search.is_visible());
    search.set_text("");
    search.emit_by_name::<()>("search-changed", &[]);
    assert!(run_main_context_until(|| !search_empty_card.is_visible()));

    let move_dialog = show_move_note_dialog(
        &state,
        note.id,
        second.id,
        &note.title,
        &toast,
        Some(&window),
    );
    let move_search = widget_as::<gtk::SearchEntry>(move_dialog.upcast_ref(), "move-note-search")
        .ok_or("move search")?;
    move_search.set_text("not-present");
    move_search.emit_by_name::<()>("search-changed", &[]);
    assert!(run_main_context_until(|| {
        widget_as::<gtk::ListBox>(move_dialog.upcast_ref(), "move-note-category-list")
            .is_some_and(|list| list.first_child().is_some())
    }));
    move_search.set_text("");
    move_search.emit_by_name::<()>("search-changed", &[]);
    assert!(run_main_context_until(|| {
        widget_as::<gtk::ListBox>(move_dialog.upcast_ref(), "move-note-category-list")
            .and_then(|list| list.first_child())
            .and_then(|row| row.downcast::<gtk::ListBoxRow>().ok())
            .and_then(|row| row.child())
            .is_some_and(|child| child.is::<gtk::Button>())
    }));
    let move_list = widget_as::<gtk::ListBox>(move_dialog.upcast_ref(), "move-note-category-list")
        .ok_or("move category list")?;
    let destination_button = move_list
        .first_child()
        .and_downcast::<gtk::ListBoxRow>()
        .and_then(|row| row.child())
        .and_downcast::<gtk::Button>()
        .ok_or("move destination button")?;
    destination_button.emit_clicked();
    assert!(run_main_context_until(|| state
        .client
        .note(note.id)
        .ok()
        .flatten()
        .is_some_and(|note| note.category_id != second.id)));
    let moved_note = state.client.note(note.id)?.ok_or("moved note")?;
    move_note_to_category(
        &state,
        note.id,
        moved_note.category_id,
        second.clone(),
        &toast,
    );
    assert!(run_main_context_until(|| state
        .client
        .note(note.id)
        .ok()
        .flatten()
        .is_some_and(|note| note.category_id == second.id)));
    assert!(run_main_context_until(|| {
        state
            .mvu_model()
            .is_some_and(|model| model.undo_move.is_some())
    }));
    window.activate_action("mvu.undo-move", None)?;
    assert!(run_main_context_until(|| state
        .client
        .note(note.id)
        .ok()
        .flatten()
        .is_some_and(|note| note.category_id == moved_note.category_id)));

    rename_category(&state, second.id, "Projects renamed")?;
    refresh_sidebar(&state);
    assert!(
        state
            .client
            .categories()?
            .iter()
            .any(|category| category.name == "Projects renamed")
    );
    let confirmed = std::rc::Rc::new(std::cell::Cell::new(false));
    let callback = std::rc::Rc::clone(&confirmed);
    let confirmation = category_trash_dialog("Projects renamed", move || callback.set(true));
    confirmation.emit_by_name::<()>("response", &[&"cancel"]);
    assert!(!confirmed.get());
    confirmation.emit_by_name::<()>("response", &[&"trash"]);
    assert!(confirmed.get());

    let open_trash = widget_as::<gtk::Button>(&sidebar, "open-trash-button").ok_or("open trash")?;
    open_trash.emit_clicked();
    assert!(run_main_context_until(|| {
        stack.visible_child_name().as_deref() == Some("trash")
            && state.selected_category.get().is_none()
    }));
    let trash_back =
        widget_as::<gtk::Button>(&trash, "back-from-trash-button").ok_or("trash back")?;
    trash_back.emit_clicked();
    assert!(run_main_context_until(|| stack
        .visible_child_name()
        .as_deref()
        == Some("browser")));
    let archived = state.client.create_category("Archived")?;
    state.client.trash_category(archived.id)?;
    refresh_trash(&state);
    assert!(run_main_context_until(|| {
        widget_as::<gtk::Button>(&trash, &format!("restore-category:{}", archived.id)).is_some()
    }));
    let restore_category =
        widget_as::<gtk::Button>(&trash, &format!("restore-category:{}", archived.id))
            .ok_or("restore category")?;
    restore_category.emit_clicked();
    assert!(run_main_context_until(|| {
        state
            .client
            .categories()
            .is_ok_and(|categories| categories.iter().any(|category| category.id == archived.id))
    }));

    let _ = state.dispatch_mvu(crate::mvu::AppMsg::Navigation(
        crate::mvu::NavigationMsg::OpenNote(note.id),
    ));
    assert!(run_main_context_until(|| {
        state
            .mvu_model()
            .and_then(|model| model.editor)
            .is_some_and(|document| document.note_id == note.id)
    }));
    let web = widget_as::<webkit6::WebView>(&editor, "rich-editor").ok_or("rich editor")?;
    let source = widget_as::<gtk::TextView>(&editor, "source-editor").ok_or("source editor")?;
    let rich = widget_as::<gtk::ToggleButton>(&editor, "editor-mode-rich").ok_or("rich mode")?;
    let source_mode =
        widget_as::<gtk::ToggleButton>(&editor, "editor-mode-source").ok_or("source mode")?;
    let preview =
        widget_as::<gtk::ToggleButton>(&editor, "editor-mode-rendered").ok_or("preview mode")?;
    let bold = widget_as::<gtk::ToggleButton>(&editor, "format-bold-button").ok_or("bold")?;
    let bullet = widget_as::<gtk::ToggleButton>(&editor, "format-bullet-button").ok_or("bullet")?;
    assert!(bold.has_css_class("flat"));
    source_mode.set_active(true);
    source.buffer().set_text("# Source\n\nA paragraph");
    assert!(run_main_context_until(|| {
        state
            .mvu_model()
            .and_then(|model| model.editor)
            .is_some_and(|document| document.source == "# Source\n\nA paragraph")
    }));
    let editor_note_id = state
        .mvu_model()
        .and_then(|model| model.editor)
        .map(|document| document.note_id)
        .ok_or("active MVU editor document")?;
    assert!(run_main_context_until(|| {
        state
            .client
            .note(editor_note_id)
            .ok()
            .flatten()
            .is_some_and(|saved| saved.source == "# Source\n\nA paragraph")
    }));
    preview.set_active(true);
    let rendered_preview =
        widget_as::<webkit6::WebView>(&editor, "rendered-preview").ok_or("rendered preview")?;
    let preview_padding = std::rc::Rc::new(std::cell::RefCell::new(None));
    let preview_padding_for_callback = std::rc::Rc::clone(&preview_padding);
    let rendered_preview_for_padding = rendered_preview.clone();
    glib::timeout_add_local_once(std::time::Duration::from_millis(250), move || {
        rendered_preview_for_padding.evaluate_javascript(
            "getComputedStyle(document.body).paddingTop",
            None,
            None,
            None::<&gtk::gio::Cancellable>,
            move |result| {
                *preview_padding_for_callback.borrow_mut() =
                    result.ok().map(|value| value.to_str().to_string());
            },
        );
    });
    assert!(run_main_context_until(|| preview_padding
        .borrow()
        .is_some()));
    assert_eq!(preview_padding.borrow().as_deref(), Some("24px"));
    let preview_selection = std::rc::Rc::new(std::cell::RefCell::new(None));
    let preview_selection_for_callback = std::rc::Rc::clone(&preview_selection);
    rendered_preview.evaluate_javascript(
        "getComputedStyle(document.documentElement).getPropertyValue('--selection-background').trim()",
        None,
        None,
        None::<&gtk::gio::Cancellable>,
        move |result| {
            *preview_selection_for_callback.borrow_mut() =
                result.ok().map(|value| value.to_str().to_string());
        },
    );
    assert!(run_main_context_until(|| preview_selection
        .borrow()
        .is_some()));
    assert!(
        preview_selection
            .borrow()
            .as_deref()
            .is_some_and(|value| value.starts_with("rgb("))
    );
    rich.set_active(true);
    assert!(run_main_context_until(|| rich.is_active()));
    let browser_source = std::rc::Rc::new(std::cell::RefCell::new(None));
    let browser_source_for_callback = std::rc::Rc::clone(&browser_source);
    web.evaluate_javascript(
        "window.carverEditor?.source() ?? null",
        None,
        None,
        None::<&gtk::gio::Cancellable>,
        move |result| {
            *browser_source_for_callback.borrow_mut() =
                result.ok().map(|value| value.to_str().to_string());
        },
    );
    assert!(run_main_context_until(|| browser_source.borrow().is_some()));
    assert_eq!(
        browser_source.borrow().as_deref(),
        Some("# Source\n\nA paragraph")
    );
    let editor_selection = std::rc::Rc::new(std::cell::RefCell::new(None));
    let editor_selection_for_callback = std::rc::Rc::clone(&editor_selection);
    web.evaluate_javascript(
        "getComputedStyle(document.documentElement).getPropertyValue('--selection-background').trim()",
        None,
        None,
        None::<&gtk::gio::Cancellable>,
        move |result| {
            *editor_selection_for_callback.borrow_mut() =
                result.ok().map(|value| value.to_str().to_string());
        },
    );
    assert!(run_main_context_until(|| editor_selection
        .borrow()
        .is_some()));
    assert_eq!(
        editor_selection.borrow().as_deref(),
        preview_selection.borrow().as_deref()
    );
    state.refresh_remote_image_policy(false);
    let rich_image_policy = std::rc::Rc::new(std::cell::RefCell::new(None));
    let rich_image_policy_for_callback = std::rc::Rc::clone(&rich_image_policy);
    let web_for_image_policy = web.clone();
    glib::timeout_add_local_once(std::time::Duration::from_millis(250), move || {
        web_for_image_policy.evaluate_javascript(
            "document.querySelector('meta[http-equiv=\"Content-Security-Policy\"]')?.content ?? null",
            None,
            None,
            None::<&gtk::gio::Cancellable>,
            move |result| {
                *rich_image_policy_for_callback.borrow_mut() =
                    result.ok().map(|value| value.to_str().to_string());
            },
        );
    });
    assert!(run_main_context_until(|| rich_image_policy
        .borrow()
        .is_some()));
    assert!(
        rich_image_policy
            .borrow()
            .as_deref()
            .is_some_and(|policy| policy.contains("img-src data: carver-asset: blob:"))
    );
    let heading_menu =
        widget_as::<gtk::MenuButton>(&editor, "format-heading-button").ok_or("heading menu")?;
    let heading_choices = heading_menu
        .popover()
        .and_then(|popover| popover.child())
        .ok_or("heading choices")?;
    let heading_two = heading_choices
        .first_child()
        .and_then(|normal| normal.next_sibling())
        .and_then(|heading_one| heading_one.next_sibling())
        .and_then(|heading_two| heading_two.downcast::<gtk::ToggleButton>().ok())
        .ok_or("heading two")?;
    web.evaluate_javascript(
        "window.carverEditor.command('heading', 2)",
        None,
        None,
        None::<&gtk::gio::Cancellable>,
        |_| {},
    );
    assert!(run_main_context_until(|| heading_two.is_active()));
    assert!(heading_menu.has_css_class("context-active"));
    web.evaluate_javascript(
        "window.carverEditor.command('bold')",
        None,
        None,
        None::<&gtk::gio::Cancellable>,
        |_| {},
    );
    assert!(run_main_context_until(|| bold.is_active()));
    web.evaluate_javascript(
        "window.carverEditor.command('bold')",
        None,
        None,
        None::<&gtk::gio::Cancellable>,
        |_| {},
    );
    assert!(run_main_context_until(|| !bold.is_active()));
    bullet.emit_clicked();
    assert!(run_main_context_until(|| source
        .buffer()
        .text(
            &source.buffer().start_iter(),
            &source.buffer().end_iter(),
            false,
        )
        .contains("- A paragraph")));

    exercise_source_formatting_controls(&editor, &source.buffer())?;
    assert_split_preview_tracks_source_scroll(&editor, &source, &source_mode)?;

    source_mode.set_active(true);
    source
        .buffer()
        .set_text("![First](assets/first.png){width=\"50%\"}");
    rich.set_active(true);
    let image_width = std::rc::Rc::new(std::cell::RefCell::new(None));
    let image_width_for_callback = std::rc::Rc::clone(&image_width);
    web.evaluate_javascript(
        "document.querySelector('#editor img')?.style.width ?? null",
        None,
        None,
        None::<&gtk::gio::Cancellable>,
        move |result| {
            *image_width_for_callback.borrow_mut() =
                result.ok().map(|value| value.to_str().to_string());
        },
    );
    assert!(run_main_context_until(|| image_width.borrow().is_some()));
    assert_eq!(image_width.borrow().as_deref(), Some("50%"));

    web.evaluate_javascript(
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
            false,
        )
        .contains("\n\n![Pasted image](assets/second.png)")));

    web.evaluate_javascript(
        r"
        (() => {
          const png = Uint8Array.from(atob('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVQIHWP4z8DwHwAFgAI/ScL8WQAAAABJRU5ErkJggg=='), value => value.charCodeAt(0));
          const transfer = new DataTransfer();
          transfer.items.add(new File([png], 'pasted.png', { type: 'image/png' }));
          return document.querySelector('#editor').dispatchEvent(new ClipboardEvent('paste', {
            bubbles: true, cancelable: true, clipboardData: transfer,
          }));
        })()
        ",
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
            false,
        )
        .contains("![Pasted image](assets/")));
    assert!(
        !source
            .buffer()
            .text(
                &source.buffer().start_iter(),
                &source.buffer().end_iter(),
                false,
            )
            .contains("blob:")
    );

    let source_before_blob_recovery = source
        .buffer()
        .text(
            &source.buffer().start_iter(),
            &source.buffer().end_iter(),
            false,
        )
        .to_string();
    web.evaluate_javascript(
        r"
        (() => {
          const png = Uint8Array.from(atob('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVQIHWP4z8DwHwAFgAI/ScL8WQAAAABJRU5ErkJggg=='), value => value.charCodeAt(0));
          window.carverEditor.insertImage(URL.createObjectURL(new Blob([png], { type: 'image/png' })));
        })()
        ",
        None,
        None,
        None::<&gtk::gio::Cancellable>,
        |_| {},
    );
    assert!(
        run_main_context_until(|| {
            let current = source.buffer().text(
                &source.buffer().start_iter(),
                &source.buffer().end_iter(),
                false,
            );
            current != source_before_blob_recovery
                && current.contains("assets/")
                && !current.contains("blob:")
        }),
        "blob recovery left source as: {}",
        source.buffer().text(
            &source.buffer().start_iter(),
            &source.buffer().end_iter(),
            false,
        )
    );

    let delete_note =
        widget_as::<gtk::Button>(&editor, "delete-note-button").ok_or("delete note")?;
    delete_note.emit_clicked();
    assert!(run_main_context_until(|| state
        .client
        .note(note.id)
        .ok()
        .flatten()
        .is_some_and(|note| note.trashed_at.is_some())));
    assert!(run_main_context_until(|| {
        widget_as::<gtk::Button>(&trash, &format!("restore-note:{}", note.id)).is_some()
    }));
    let restore = widget_as::<gtk::Button>(&trash, &format!("restore-note:{}", note.id))
        .ok_or("restore note")?;
    restore.emit_clicked();
    assert!(run_main_context_until(|| state
        .client
        .note(note.id)
        .ok()
        .flatten()
        .is_some_and(|note| note.trashed_at.is_none())));

    let (application_directory, application_state) = test_state()?;
    let application_paths = AppPaths {
        config_dir: application_directory.path().join("application-config"),
        data_dir: application_directory.path().join("application-data"),
        cache_dir: application_directory.path().join("application-cache"),
    };
    let application_client = crate::app::open_library_for_test(&application_paths)?;
    crate::app::ensure_first_category(&application_client);
    assert_eq!(application_client.categories()?.len(), 1);
    let application = adw::Application::new(
        Some("io.github.josbeir.Carver.Tests"),
        gtk::gio::ApplicationFlags::empty(),
    );
    application.register(None::<&gtk::gio::Cancellable>)?;
    let application_config = application_directory.path().join("application-config.toml");
    let application_window = crate::app::build_window_for_test(
        &application,
        &application_state,
        application_config.clone(),
    );
    assert_eq!(
        application_window.icon_name().as_deref(),
        Some(crate::app::APPLICATION_ICON)
    );
    crate::dialogs::present_dialogs_for_test(
        &application_window,
        &application_state,
        &application_config,
    );
    application_window.close();
    window.close();
    Ok(())
}

fn exercise_source_formatting_controls(
    editor: &gtk::Widget,
    buffer: &gtk::TextBuffer,
) -> TestResult {
    for name in [
        "source-format-bold-button",
        "source-format-italic-button",
        "source-format-strike-button",
        "source-format-underline-button",
        "source-format-bullet-button",
        "source-format-ordered-button",
        "source-format-task-button",
        "source-format-code-button",
        "source-format-code-block-button",
        "source-format-link-button",
    ] {
        buffer.set_text("format me");
        select_all(buffer);
        let button = widget_as::<gtk::Button>(editor, name).ok_or(name)?;
        button.emit_clicked();
    }

    for name in ["source-format-more-button", "source-format-heading-button"] {
        let menu = widget_as::<gtk::MenuButton>(editor, name).ok_or(name)?;
        let popover = menu.popover().ok_or("formatting popover")?;
        let choices = popover.child().ok_or("formatting choices")?;
        let mut child = choices.first_child();
        while let Some(widget) = child {
            child = widget.next_sibling();
            let choice = widget
                .downcast::<gtk::Button>()
                .map_err(|_| "formatting choice")?;
            buffer.set_text("format me");
            select_all(buffer);
            choice.emit_clicked();
        }
    }

    let table = widget_as::<gtk::MenuButton>(editor, "source-format-table-button")
        .ok_or("source table picker")?;
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
    let first_cell = grid
        .first_child()
        .and_downcast::<gtk::Button>()
        .ok_or("source table first cell")?;
    buffer.set_text("");
    first_cell.emit_clicked();
    if !buffer
        .text(&buffer.start_iter(), &buffer.end_iter(), false)
        .contains("|= |")
    {
        return Err("source table picker insert".into());
    }

    let image = widget_as::<gtk::MenuButton>(editor, "source-format-image-button")
        .ok_or("source image menu")?;
    let image_choices = image
        .popover()
        .and_then(|popover| popover.child())
        .and_downcast::<gtk::Box>()
        .ok_or("source image choices")?;
    let mut child = image_choices.first_child();
    let mut width_choice = None;
    while let Some(widget) = child {
        child = widget.next_sibling();
        if let Ok(button) = widget.downcast::<gtk::Button>()
            && button.label().as_deref() == Some("50%")
        {
            width_choice = Some(button);
            break;
        }
    }
    let width_choice = width_choice.ok_or("source image width choice")?;
    buffer.set_text("![Image](assets/image.png)");
    let cursor = buffer.end_iter();
    buffer.place_cursor(&cursor);
    width_choice.emit_clicked();
    if !buffer
        .text(&buffer.start_iter(), &buffer.end_iter(), false)
        .contains("width=\"50%\"")
    {
        return Err("source image width choice".into());
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
    let preview =
        widget_as::<webkit6::WebView>(editor, "source-split-preview").ok_or("split preview")?;
    source.buffer().set_text(
        &(0..180)
            .map(|index| format!("Paragraph {index}"))
            .collect::<Vec<_>>()
            .join("\n\n"),
    );
    let split_toggle =
        widget_as::<gtk::ToggleButton>(editor, "source-split-toggle").ok_or("split toggle")?;
    split_toggle.set_active(true);
    assert!(split_toggle.is_active());
    let preview_has_source = std::rc::Rc::new(std::cell::RefCell::new(None));
    let source_for_callback = std::rc::Rc::clone(&preview_has_source);
    let preview_for_source_check = preview.clone();
    glib::timeout_add_local_once(std::time::Duration::from_millis(250), move || {
        preview_for_source_check.evaluate_javascript(
            "String(document.body.innerText.includes('Paragraph 179'))",
            None,
            None,
            None::<&gtk::gio::Cancellable>,
            move |result| {
                *source_for_callback.borrow_mut() =
                    result.ok().map(|value| value.to_str() == "true");
            },
        );
    });
    assert!(run_main_context_until(|| preview_has_source
        .borrow()
        .as_ref()
        == Some(&true)));
    let source_scroll = source
        .parent()
        .and_downcast::<gtk::ScrolledWindow>()
        .ok_or("source scroll")?;
    let adjustment = source_scroll.vadjustment();
    assert!(run_main_context_until(
        || adjustment.upper() > adjustment.page_size()
    ));
    adjustment.set_value((adjustment.upper() - adjustment.page_size()) * 0.5);

    let preview_position = std::rc::Rc::new(std::cell::RefCell::new(None));
    let position_for_callback = std::rc::Rc::clone(&preview_position);
    let preview_for_query = preview.clone();
    glib::timeout_add_local_once(std::time::Duration::from_millis(80), move || {
        preview_for_query.evaluate_javascript(
            "String(window.scrollY)",
            None,
            None,
            None::<&gtk::gio::Cancellable>,
            move |result| {
                *position_for_callback.borrow_mut() = result
                    .ok()
                    .and_then(|value| value.to_str().parse::<f64>().ok());
            },
        );
    });
    assert!(
        run_main_context_until(|| preview_position
            .borrow()
            .is_some_and(|position| position > 0.0)),
        "split active: {}, source adjustment: {}/{}, preview position: {:?}, preview mapped: {}",
        split_toggle.is_active(),
        adjustment.value(),
        adjustment.upper() - adjustment.page_size(),
        preview_position.borrow(),
        preview.is_mapped(),
    );
    split_toggle.set_active(false);
    Ok(())
}
