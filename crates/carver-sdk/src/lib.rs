//! Portable Carver application API for GTK and future native frontends.

#![forbid(unsafe_code)]

use std::sync::{Arc, Mutex};

use carver_config::AppPaths;
pub use carver_domain::{Category, CategoryId, Note, NoteId, NoteSummary, Revision, SearchHit};
use carver_storage_sqlite::{SqliteLibrary, StorageError};
use thiserror::Error;
use time::OffsetDateTime;

/// Cloneable, UI-independent application facade.
#[derive(Clone)]
pub struct LibraryClient {
    storage: Arc<Mutex<SqliteLibrary>>,
}

/// SDK-level failures.
#[derive(Debug, Error)]
pub enum LibraryError {
    /// Persistence failed.
    #[error(transparent)]
    Storage(#[from] StorageError),
    /// The storage worker became unavailable.
    #[error("library storage is unavailable")]
    Unavailable,
}

impl LibraryClient {
    /// Opens the user's managed Carver library.
    ///
    /// # Errors
    ///
    /// Returns an error when paths cannot be prepared or the SQLite library cannot open.
    pub fn open(paths: &AppPaths) -> Result<Self, LibraryError> {
        paths
            .ensure_exists()
            .map_err(|error| LibraryError::Storage(StorageError::Corrupt(error.to_string())))?;
        let storage = SqliteLibrary::open(&paths.database_file(), &paths.assets_dir())?;
        Ok(Self {
            storage: Arc::new(Mutex::new(storage)),
        })
    }

    /// Creates a category.
    ///
    /// # Errors
    ///
    /// Returns an error when the library cannot persist the category.
    pub fn create_category(&self, name: &str) -> Result<Category, LibraryError> {
        self.storage()?
            .create_category(name, OffsetDateTime::now_utc())
            .map_err(Into::into)
    }

    /// Lists sidebar categories.
    ///
    /// # Errors
    ///
    /// Returns an error when the library cannot be queried.
    pub fn categories(&self) -> Result<Vec<Category>, LibraryError> {
        self.storage()?.list_categories().map_err(Into::into)
    }

    /// Renames an existing category.
    ///
    /// # Errors
    ///
    /// Returns an error when the category cannot be updated.
    pub fn rename_category(
        &self,
        category_id: CategoryId,
        name: &str,
    ) -> Result<Category, LibraryError> {
        self.storage()?
            .rename_category(category_id, name, OffsetDateTime::now_utc())
            .map_err(Into::into)
    }

    /// Creates a blank note in a category.
    ///
    /// # Errors
    ///
    /// Returns an error when the category is unavailable or the note cannot be persisted.
    pub fn create_note(&self, category_id: CategoryId) -> Result<Note, LibraryError> {
        self.storage()?
            .create_note(category_id, OffsetDateTime::now_utc())
            .map_err(Into::into)
    }

    /// Loads one note.
    ///
    /// # Errors
    ///
    /// Returns an error when the library cannot be queried.
    pub fn note(&self, note_id: NoteId) -> Result<Option<Note>, LibraryError> {
        self.storage()?.note(note_id).map_err(Into::into)
    }

    /// Persists a source update guarded by its current revision.
    ///
    /// # Errors
    ///
    /// Returns an error on a revision conflict or when the update cannot be persisted.
    pub fn save_note(
        &self,
        note_id: NoteId,
        revision: Revision,
        source: &str,
    ) -> Result<Note, LibraryError> {
        self.storage()?
            .save_note(note_id, revision, source, OffsetDateTime::now_utc())
            .map_err(Into::into)
    }

    /// Moves a note to the in-app trash.
    ///
    /// # Errors
    ///
    /// Returns an error when the note cannot be moved to trash.
    pub fn trash_note(&self, note_id: NoteId) -> Result<(), LibraryError> {
        self.storage()?
            .trash_note(note_id, OffsetDateTime::now_utc())
            .map_err(Into::into)
    }

    /// Restores a note from the in-app trash.
    ///
    /// # Errors
    ///
    /// Returns an error when the note cannot be restored.
    pub fn restore_note(&self, note_id: NoteId) -> Result<(), LibraryError> {
        self.storage()?.restore_note(note_id).map_err(Into::into)
    }

    /// Returns the latest active notes, optionally filtered to a category.
    ///
    /// # Errors
    ///
    /// Returns an error when the library cannot be queried.
    pub fn recent_notes(
        &self,
        category_id: Option<CategoryId>,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<NoteSummary>, LibraryError> {
        self.storage()?
            .recent_notes(category_id, limit, offset)
            .map_err(Into::into)
    }

    /// Searches active notes by title and body.
    ///
    /// # Errors
    ///
    /// Returns an error when the full-text index cannot be queried.
    pub fn search(
        &self,
        query: &str,
        category_id: Option<CategoryId>,
        limit: usize,
    ) -> Result<Vec<SearchHit>, LibraryError> {
        self.storage()?
            .search_notes(query, category_id, limit)
            .map_err(Into::into)
    }

    /// Stores image bytes as a managed note asset and returns its relative Carve path.
    ///
    /// # Errors
    ///
    /// Returns an error when the asset cannot be safely persisted with the note.
    pub fn store_asset(
        &self,
        note_id: NoteId,
        extension: &str,
        bytes: &[u8],
    ) -> Result<String, LibraryError> {
        self.storage()?
            .store_asset(note_id, extension, bytes)
            .map_err(Into::into)
    }

    fn storage(&self) -> Result<std::sync::MutexGuard<'_, SqliteLibrary>, LibraryError> {
        self.storage.lock().map_err(|_| LibraryError::Unavailable)
    }
}
