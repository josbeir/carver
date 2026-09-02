//! UI-neutral state and note/category actions for the GTK frontend.

use std::cell::{Cell, RefCell};

use carver_config::{Config, EditorMode};
use carver_sdk::{Category, CategoryId, LibraryClient, Note};
use carver_storage_sqlite::SqliteLibrary;
use libadwaita as adw;

#[cfg(test)]
use carver_sdk::{LibraryError, NoteId};
#[cfg(test)]
use carver_storage_sqlite::StorageError;

/// The local library client used by the GTK frontend.
pub(crate) type AppLibraryClient = LibraryClient<SqliteLibrary>;

#[cfg(test)]
pub(crate) type AppLibraryError = LibraryError<StorageError>;

/// Mutable application state shared by GTK signal handlers.
///
/// Each mutable value has its own interior-mutable cell. This keeps borrows short
/// and prevents a UI callback from holding a mutable state borrow during storage I/O.
pub(crate) struct AppState {
    pub(crate) client: AppLibraryClient,
    pub(crate) config: RefCell<Config>,
    pub(crate) selected_category: Cell<Option<CategoryId>>,
    pub(crate) selected_category_name: RefCell<Option<String>>,
    pub(crate) categories: RefCell<Vec<Category>>,
    pub(crate) current_note: RefCell<Option<Note>>,
    pub(crate) source_mode: Cell<bool>,
    pub(crate) synchronizing_editor: Cell<bool>,
    pub(crate) autosave_generation: Cell<u64>,
    pub(crate) save_in_flight: Cell<bool>,
    pub(crate) browser_generation: Cell<u64>,
    pub(crate) sidebar_generation: Cell<u64>,
    pub(crate) trash_generation: Cell<u64>,
    pub(crate) search_query: RefCell<String>,
    pub(crate) browser_list: RefCell<Option<gtk::ListBox>>,
    pub(crate) browser_stack: RefCell<Option<gtk::Stack>>,
    pub(crate) browser_title: RefCell<Option<adw::WindowTitle>>,
    pub(crate) sidebar_list: RefCell<Option<gtk::ListBox>>,
    pub(crate) trash_list: RefCell<Option<gtk::ListBox>>,
    pub(crate) trash_content_stack: RefCell<Option<gtk::Stack>>,
    pub(crate) trash_status: RefCell<Option<adw::StatusPage>>,
    pub(crate) empty_trash_button: RefCell<Option<gtk::Button>>,
}

impl AppState {
    /// Creates state for one application window.
    pub(crate) fn new(client: AppLibraryClient, config: Config) -> Self {
        let source_mode = config.editor.default_mode == EditorMode::Source;
        Self {
            client,
            config: RefCell::new(config),
            selected_category: Cell::new(None),
            selected_category_name: RefCell::new(None),
            categories: RefCell::new(Vec::new()),
            current_note: RefCell::new(None),
            source_mode: Cell::new(source_mode),
            synchronizing_editor: Cell::new(false),
            autosave_generation: Cell::new(0),
            save_in_flight: Cell::new(false),
            browser_generation: Cell::new(0),
            sidebar_generation: Cell::new(0),
            trash_generation: Cell::new(0),
            search_query: RefCell::new(String::new()),
            browser_list: RefCell::new(None),
            browser_stack: RefCell::new(None),
            browser_title: RefCell::new(None),
            sidebar_list: RefCell::new(None),
            trash_list: RefCell::new(None),
            trash_content_stack: RefCell::new(None),
            trash_status: RefCell::new(None),
            empty_trash_button: RefCell::new(None),
        }
    }
}

/// Returns the active category, falling back to the first available category.
#[cfg(test)]
pub(crate) fn active_category(state: &AppState) -> Result<Option<CategoryId>, AppLibraryError> {
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

/// Creates a note and makes it the current note.
#[cfg(test)]
pub(crate) fn create_note_for_active_category(
    state: &AppState,
) -> Result<Option<Note>, AppLibraryError> {
    let Some(category_id) = active_category(state)? else {
        return Ok(None);
    };
    let note = state.client.create_note(category_id)?;
    state.current_note.replace(Some(note.clone()));
    Ok(Some(note))
}

/// Creates the next numbered category for test fixture setup.
#[cfg(test)]
pub(crate) fn create_next_category(state: &AppState) -> Result<Category, AppLibraryError> {
    let sequence = state.client.categories()?.len() + 1;
    state
        .client
        .create_category(&format!("Category {sequence}"))
}

/// Renames a category using the frontend-neutral SDK.
#[cfg(test)]
pub(crate) fn rename_category(
    state: &AppState,
    category_id: CategoryId,
    name: &str,
) -> Result<Category, AppLibraryError> {
    state.client.rename_category(category_id, name.trim())
}

/// Moves the active note to trash and clears the editor state.
#[cfg(test)]
pub(crate) fn trash_current_note(state: &AppState) -> Result<bool, AppLibraryError> {
    let Some(note) = state.current_note.borrow().clone() else {
        return Ok(false);
    };
    state.client.trash_note(note.id)?;
    state.current_note.take();
    Ok(true)
}

/// Loads a note into the editor state.
#[cfg(test)]
pub(crate) fn open_note(
    state: &AppState,
    note_id: NoteId,
) -> Result<Option<Note>, AppLibraryError> {
    let note = state.client.note(note_id)?;
    state.current_note.replace(note.clone());
    Ok(note)
}

/// Saves the active note's source and updates its optimistic-concurrency revision.
#[cfg(test)]
pub(crate) fn save_current_note(
    state: &AppState,
    source: &str,
) -> Result<Option<Note>, AppLibraryError> {
    let Some(note) = state.current_note.borrow().clone() else {
        return Ok(None);
    };
    let saved = state.client.save_note(note.id, note.revision, source)?;
    state.current_note.replace(Some(saved.clone()));
    Ok(Some(saved))
}

/// Stores a clipboard image for the active note and returns its Carve image path.
#[cfg(test)]
pub(crate) fn store_pasted_image(
    state: &AppState,
    bytes: &[u8],
) -> Result<Option<String>, AppLibraryError> {
    let Some(note) = state.current_note.borrow().clone() else {
        return Ok(None);
    };
    Ok(Some(state.client.store_asset(note.id, "png", bytes)?))
}
