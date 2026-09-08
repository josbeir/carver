use gtk::prelude::*;
use libadwaita::prelude::*;

use super::*;

pub(crate) fn runtime_should_render_and_complete_each_initial_resource()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary_directory = tempfile::tempdir()?;
    let paths = AppPaths {
        config_dir: temporary_directory.path().join("config"),
        data_dir: temporary_directory.path().join("data"),
        cache_dir: temporary_directory.path().join("cache"),
    };
    paths.ensure_exists()?;
    let client = LibraryClient::spawn(SqliteLibrary::open(
        &paths.database_file(),
        &paths.assets_dir(),
    )?)?;
    let stack = gtk::Stack::new();
    for name in ["browser", "editor", "trash"] {
        stack.add_named(&gtk::Box::new(gtk::Orientation::Vertical, 0), Some(name));
    }
    let browser_list = gtk::ListBox::new();
    let browser_pages = gtk::Stack::new();
    let browser_status = libadwaita::StatusPage::new();
    browser_pages.add_named(
        &gtk::Box::new(gtk::Orientation::Vertical, 0),
        Some("contents"),
    );
    browser_pages.add_named(&browser_status, Some("empty"));
    let browser = browser_view_refs(browser_list, browser_pages, browser_status.clone());
    let runtime = AppRuntime::new(
        client.clone(),
        AppModel::new(&Config::default()),
        crate::view::ViewRefs::new(stack, browser_status, libadwaita::StatusPage::new())
            .with_browser(browser),
    );

    runtime.dispatch(AppMsg::Navigation(NavigationMsg::Started));
    assert!(crate::ui::tests::support::run_main_context_until(|| {
        matches!(runtime.model().sidebar.state, LoadState::Ready(_))
            && matches!(runtime.model().browser.notes.state, LoadState::Ready(_))
    }));
    assert_eq!(client.categories()?.len(), 1);

    runtime.dispatch(AppMsg::Navigation(NavigationMsg::ImportNote {
        format: DocumentImportFormat::Markdown,
        source: String::from("# Imported\n\n- [x] Converted"),
    }));
    assert!(crate::ui::tests::support::run_main_context_until(|| {
        runtime.model().editor.as_ref().is_some_and(|document| {
            document.source.contains("# Imported") && document.source.contains("- [x] Converted")
        })
    }));
    assert_eq!(client.recent_notes(None, 10, 0)?.len(), 1);

    runtime.dispatch(AppMsg::Browser(BrowserMsg::SearchChanged(
        "needle".to_owned(),
    )));
    assert!(crate::ui::tests::support::run_main_context_until(|| {
        matches!(runtime.model().browser.notes.state, LoadState::Ready(_))
    }));

    runtime.dispatch(AppMsg::Navigation(NavigationMsg::ShowTrash));
    assert!(crate::ui::tests::support::run_main_context_until(|| {
        matches!(runtime.model().trash.state, LoadState::Ready(_))
    }));

    runtime.dispatch(AppMsg::Sidebar(SidebarMsg::Reload));
    let LoadState::Loading(request_id) = runtime.model().sidebar.state else {
        return Err("sidebar should be loading".into());
    };
    runtime.dispatch(AppMsg::Library(LibraryReply::SidebarLoaded {
        request_id,
        result: Err(UiError::new("offline")),
    }));
    assert_eq!(
        runtime.model().sidebar.state,
        LoadState::Failed(UiError::new("offline"))
    );

    runtime_should_write_the_current_carve_snapshot(&runtime, &temporary_directory)?;

    let dispatcher = AppDispatcher::default();
    {
        let detached_stack = gtk::Stack::new();
        detached_stack.add_named(
            &gtk::Box::new(gtk::Orientation::Vertical, 0),
            Some("browser"),
        );
        let detached_runtime = AppRuntime::new(
            client.clone(),
            AppModel::new(&Config::default()),
            crate::view::ViewRefs::new(
                detached_stack,
                libadwaita::StatusPage::new(),
                libadwaita::StatusPage::new(),
            ),
        );
        detached_runtime.bind_dispatcher(&dispatcher);
        assert!(dispatcher.dispatch(AppMsg::Navigation(NavigationMsg::ShowBrowser)));
    }
    assert!(!dispatcher.dispatch(AppMsg::Navigation(NavigationMsg::ShowBrowser)));
    Ok(())
}

fn browser_view_refs(
    list: gtk::ListBox,
    pages: gtk::Stack,
    status: libadwaita::StatusPage,
) -> crate::ui::browser::BrowserViewRefs {
    crate::ui::browser::BrowserViewRefs {
        favorites_section: gtk::Box::new(gtk::Orientation::Vertical, 0),
        favorites: gtk::ListBox::new(),
        list,
        pages,
        search_bar: gtk::SearchBar::new(),
        search_entry: gtk::SearchEntry::new(),
        search_toggle: gtk::ToggleButton::new(),
        search_empty_card: gtk::Box::new(gtk::Orientation::Vertical, 0),
        category_empty_card: gtk::Box::new(gtk::Orientation::Vertical, 0),
        empty_new_note_button: gtk::Button::new(),
        category_empty_new_note_button: gtk::Button::new(),
        category_hero: gtk::Box::new(gtk::Orientation::Vertical, 0),
        status,
    }
}

pub(crate) fn runtime_should_refresh_visible_resources_after_a_separate_client_mutates_the_library()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary_directory = tempfile::tempdir()?;
    let paths = AppPaths {
        config_dir: temporary_directory.path().join("config"),
        data_dir: temporary_directory.path().join("data"),
        cache_dir: temporary_directory.path().join("cache"),
    };
    paths.ensure_exists()?;
    let client = LibraryClient::spawn(SqliteLibrary::open(
        &paths.database_file(),
        &paths.assets_dir(),
    )?)?;
    let stack = gtk::Stack::new();
    for name in ["browser", "editor", "trash"] {
        stack.add_named(&gtk::Box::new(gtk::Orientation::Vertical, 0), Some(name));
    }
    let window = libadwaita::Window::new();
    let overlay = libadwaita::ToastOverlay::new();
    overlay.set_child(Some(&stack));
    window.set_content(Some(&overlay));
    window.present();
    let dispatcher = AppDispatcher::default();
    let browser_status = libadwaita::StatusPage::new();
    let runtime = AppRuntime::new(
        client,
        AppModel::new(&Config::default()),
        crate::view::ViewRefs::new(stack, browser_status, libadwaita::StatusPage::new())
            .with_toast_overlay(overlay)
            .with_dispatcher(dispatcher.clone()),
    );
    runtime.bind_dispatcher(&dispatcher);
    runtime.monitor_library(&paths.database_file(), dispatcher)?;
    runtime.dispatch(AppMsg::Navigation(NavigationMsg::Started));
    if !crate::ui::tests::support::run_main_context_until(|| {
        matches!(runtime.model().sidebar.state, LoadState::Ready(_))
            && matches!(runtime.model().browser.notes.state, LoadState::Ready(_))
            && runtime.model().library_revision.is_some()
    }) {
        return Err("initial library resources did not load".into());
    }

    let agent_client = carver_sdk::open_local_library(&paths.database_file(), &paths.assets_dir())?;
    let category = agent_client.create_category("From agent")?;
    let note = agent_client.create_note_with_source(category.id, "# Created by agent")?;

    if crate::ui::tests::support::run_main_context_until(|| {
        let model = runtime.model();
        matches!(
            model.sidebar.state,
            LoadState::Ready(ref categories)
                if categories.iter().any(|summary| summary.category.id == category.id)
        ) && matches!(
            model.browser.notes.state,
            LoadState::Ready(ref notes) if notes.iter().any(|summary| summary.id == note.id)
        )
    }) {
        runtime.dispatch(AppMsg::Navigation(NavigationMsg::OpenNote(note.id)));
        if !crate::ui::tests::support::run_main_context_until(|| runtime.model().editor.is_some()) {
            return Err("editor did not load".into());
        }
        let session = runtime.model().editor.ok_or("editor")?.session;
        let saved = agent_client.save_note(note.id, note.revision, "# Updated by agent")?;
        if !crate::ui::tests::support::run_main_context_until(|| {
            runtime.model().editor.is_some_and(|document| {
                document.session == session
                    && document.revision == saved.revision
                    && document.source == saved.source
                    && document.save_state == super::EditorSaveState::Clean
            })
        }) {
            return Err("separate client save did not refresh the open editor".into());
        }
        assert_external_conflict(&runtime, &agent_client, &saved, &window)?;
        assert_external_deletion(&runtime, &agent_client, note.id, &window)?;
        window.close();
        Ok(())
    } else {
        Err("separate client mutation did not refresh visible resources".into())
    }
}

fn runtime_should_write_the_current_carve_snapshot<B: LibraryBackend>(
    runtime: &AppRuntime<B>,
    temporary_directory: &tempfile::TempDir,
) -> Result<(), Box<dyn std::error::Error>> {
    let export_note_id = NoteId::new();
    runtime.dispatch(AppMsg::Editor(EditorMsg::Load {
        note_id: export_note_id,
        revision: Revision(1),
        source: String::from("# Exported draft\n\nUnsaved body"),
    }));
    runtime.dispatch(AppMsg::Editor(EditorMsg::ExportDialogRequested));
    let export_dialog = runtime
        .model()
        .editor_export_dialog_request
        .ok_or("export dialog request")?;
    let export_path = temporary_directory.path().join("exported-draft.crv");
    let export_target = gtk::gio::File::for_path(&export_path).uri().to_string();
    runtime.dispatch(AppMsg::Editor(EditorMsg::ExportRequested {
        request_id: export_dialog.request_id,
        format: EditorExportFormat::Carve,
        include_assets: false,
        target_uri: export_target,
    }));
    assert!(crate::ui::tests::support::run_main_context_until(|| {
        std::fs::read_to_string(&export_path)
            .is_ok_and(|source| source == "# Exported draft\n\nUnsaved body")
    }));
    Ok(())
}

fn assert_external_conflict(
    runtime: &AppRuntime<SqliteLibrary>,
    agent: &LibraryClient<SqliteLibrary>,
    note: &Note,
    window: &libadwaita::Window,
) -> Result<(), Box<dyn std::error::Error>> {
    runtime.dispatch(AppMsg::Editor(EditorMsg::SourceChanged(
        "Local draft".into(),
    )));
    let saved = agent.save_note(note.id, note.revision, "# Another agent edit")?;
    assert!(crate::ui::tests::support::run_main_context_until(|| {
        runtime
            .model()
            .editor
            .is_some_and(|document| document.external_change.is_some())
    }));
    assert_eq!(
        runtime.model().editor.ok_or("editor")?.source,
        "Local draft"
    );
    assert!(crate::ui::tests::support::run_main_context_until(|| {
        find_button(window.upcast_ref(), "Reload…").is_some()
    }));
    find_button(window.upcast_ref(), "Reload…")
        .ok_or("reload button")?
        .emit_clicked();
    let dialog = window
        .visible_dialog()
        .ok_or("reload confirmation")?
        .downcast::<libadwaita::AlertDialog>()
        .map_err(|_| "alert dialog")?;
    assert_eq!(dialog.default_response().as_deref(), Some("cancel"));
    find_button(dialog.upcast_ref(), "Cancel")
        .ok_or("cancel button")?
        .emit_clicked();
    assert_eq!(
        runtime.model().editor.ok_or("editor")?.source,
        "Local draft"
    );
    assert!(crate::ui::tests::support::run_main_context_until(|| {
        window.visible_dialog().is_none() && find_button(window.upcast_ref(), "Reload…").is_some()
    }));
    find_button(window.upcast_ref(), "Reload…")
        .ok_or("reload button")?
        .emit_clicked();
    let dialog = window
        .visible_dialog()
        .ok_or("reload confirmation")?
        .downcast::<libadwaita::AlertDialog>()
        .map_err(|_| "alert dialog")?;
    find_button(dialog.upcast_ref(), "Discard Edits and Reload")
        .ok_or("confirm button")?
        .emit_clicked();
    assert!(crate::ui::tests::support::run_main_context_until(|| {
        runtime.model().editor.is_some_and(|document| {
            document.source == saved.source
                && document.revision == saved.revision
                && document.external_change.is_none()
        })
    }));
    let persisted = agent.note(note.id)?.ok_or("persisted note")?;
    assert_eq!(
        (persisted.revision, persisted.updated_at),
        (saved.revision, saved.updated_at)
    );
    Ok(())
}

fn find_button(root: &gtk::Widget, label: &str) -> Option<gtk::Button> {
    if let Some(button) = root.downcast_ref::<gtk::Button>()
        && button.label().as_deref() == Some(label)
    {
        return Some(button.clone());
    }
    let mut child = root.first_child();
    while let Some(widget) = child {
        if let Some(button) = find_button(&widget, label) {
            return Some(button);
        }
        child = widget.next_sibling();
    }
    None
}

fn assert_external_deletion(
    runtime: &AppRuntime<SqliteLibrary>,
    agent: &LibraryClient<SqliteLibrary>,
    note_id: NoteId,
    window: &libadwaita::Window,
) -> Result<(), Box<dyn std::error::Error>> {
    runtime.dispatch(AppMsg::Editor(EditorMsg::SourceChanged(
        "Draft before deletion".into(),
    )));
    agent.trash_note(note_id)?;
    assert!(crate::ui::tests::support::run_main_context_until(|| {
        runtime.model().editor.is_some_and(|document| {
            document.external_change == Some(super::super::model::ExternalChange::Deleted)
        })
    }));
    assert!(crate::ui::tests::support::run_main_context_until(|| {
        window.visible_dialog().is_none()
            && find_button(window.upcast_ref(), "Close Note…").is_some()
    }));
    find_button(window.upcast_ref(), "Close Note…")
        .ok_or("close note")?
        .emit_clicked();
    let dialog = window.visible_dialog().ok_or("deletion confirmation")?;
    find_button(dialog.upcast_ref(), "Cancel")
        .ok_or("cancel")?
        .emit_clicked();
    assert_eq!(
        runtime.model().editor.ok_or("editor")?.source,
        "Draft before deletion"
    );
    assert!(crate::ui::tests::support::run_main_context_until(|| {
        window.visible_dialog().is_none()
            && find_button(window.upcast_ref(), "Close Note…").is_some()
    }));
    find_button(window.upcast_ref(), "Close Note…")
        .ok_or("close note")?
        .emit_clicked();
    let dialog = window.visible_dialog().ok_or("deletion confirmation")?;
    find_button(dialog.upcast_ref(), "Discard Draft and Open Trash")
        .ok_or("discard")?
        .emit_clicked();
    assert!(crate::ui::tests::support::run_main_context_until(|| {
        let model = runtime.model();
        model.editor.is_none()
            && model.route == Route::Trash
            && matches!(model.trash.state, LoadState::Ready(_))
    }));
    assert!(
        agent
            .note(note_id)?
            .ok_or("trashed note")?
            .trashed_at
            .is_some()
    );
    Ok(())
}
