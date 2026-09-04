//! Shared fixtures and widget helpers for GTK tests.

use std::error::Error;

use carver_config::AppPaths;
use carver_sdk::LibraryClient;
use carver_storage_sqlite::SqliteLibrary;
use gtk::prelude::*;
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

pub(crate) fn find_widget(root: &gtk::Widget, name: &str) -> Option<gtk::Widget> {
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

pub(crate) fn widget_as<T: glib::prelude::IsA<gtk::Widget> + glib::object::ObjectType>(
    root: &gtk::Widget,
    name: &str,
) -> Option<T> {
    find_widget(root, name).and_then(|widget| widget.downcast::<T>().ok())
}

pub(crate) fn run_main_context_until(predicate: impl Fn() -> bool) -> bool {
    let context = glib::MainContext::default();
    for _ in 0..100 {
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
