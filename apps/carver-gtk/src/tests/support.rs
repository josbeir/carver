//! Shared fixtures and widget helpers for GTK tests.

use std::error::Error;

use carver_config::AppPaths;
use carver_sdk::LibraryClient;
use carver_storage_sqlite::SqliteLibrary;
use gtk::prelude::*;
use libadwaita as adw;
use tempfile::TempDir;

pub(crate) type TestResult = Result<(), Box<dyn Error>>;
pub(crate) type TestLibraryClient = LibraryClient<SqliteLibrary>;
pub(crate) type TestState = (TempDir, TestLibraryClient);

pub(crate) fn test_state() -> Result<TestState, Box<dyn Error>> {
    let temporary_directory = tempfile::tempdir()?;
    let paths = AppPaths {
        config_dir: temporary_directory.path().join("config"),
        data_dir: temporary_directory.path().join("data"),
        cache_dir: temporary_directory.path().join("cache"),
    };
    paths.ensure_exists()?;
    let storage = SqliteLibrary::open(&paths.database_file(), &paths.assets_dir())?;
    Ok((temporary_directory, LibraryClient::spawn(storage)?))
}

/// Finds a named widget, preferring the copy inside the selected workspace tab.
///
/// Every open note owns a full editor, so the same widget name can appear in each
/// tab. Without this preference a background tab's editor would shadow the active
/// one, which breaks width- and visibility-dependent assertions.
pub(crate) fn find_widget(root: &gtk::Widget, name: &str) -> Option<gtk::Widget> {
    let mut matches = Vec::new();
    collect_widgets(root, name, &mut matches);
    if matches.len() <= 1 {
        return matches.into_iter().next();
    }
    if let Some(active) = selected_tab_content(root)
        && let Some(found) = matches.iter().find(|widget| contains(&active, widget))
    {
        return Some(found.clone());
    }
    matches.into_iter().next()
}

fn collect_widgets(root: &gtk::Widget, name: &str, matches: &mut Vec<gtk::Widget>) {
    if root.widget_name() == name {
        matches.push(root.clone());
    }
    let mut child = root.first_child();
    while let Some(widget) = child {
        collect_widgets(&widget, name, matches);
        child = widget.next_sibling();
    }
}

/// Returns the content widget of the workspace tab that is currently selected.
fn selected_tab_content(root: &gtk::Widget) -> Option<gtk::Widget> {
    let tab_view = first_widget(root, "workspace-tabs")?;
    let page = tab_view.downcast::<adw::TabView>().ok()?.selected_page()?;
    Some(page.child())
}

fn first_widget(root: &gtk::Widget, name: &str) -> Option<gtk::Widget> {
    if root.widget_name() == name {
        return Some(root.clone());
    }
    let mut child = root.first_child();
    while let Some(widget) = child {
        if let Some(found) = first_widget(&widget, name) {
            return Some(found);
        }
        child = widget.next_sibling();
    }
    None
}

fn contains(ancestor: &gtk::Widget, widget: &gtk::Widget) -> bool {
    let mut current = Some(widget.clone());
    while let Some(node) = current {
        if node == *ancestor {
            return true;
        }
        current = node.parent();
    }
    false
}

pub(crate) fn widget_as<T: glib::prelude::IsA<gtk::Widget> + glib::object::ObjectType>(
    root: &gtk::Widget,
    name: &str,
) -> Option<T> {
    find_widget(root, name).and_then(|widget| widget.downcast::<T>().ok())
}

pub(crate) fn run_main_context_until(predicate: impl Fn() -> bool) -> bool {
    run_main_context_until_for(std::time::Duration::from_secs(5), predicate)
}

/// Runs the GTK event loop until a predicate succeeds or its allotted time expires.
pub(crate) fn run_main_context_until_for(
    timeout: std::time::Duration,
    predicate: impl Fn() -> bool,
) -> bool {
    let context = glib::MainContext::default();
    let deadline = std::time::Instant::now() + timeout;
    while std::time::Instant::now() < deadline {
        while context.pending() {
            context.iteration(false);
        }
        if predicate() {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    false
}
