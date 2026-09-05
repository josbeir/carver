//! UI-neutral persistence contract for Carver libraries.
//!
//! This crate owns the boundary between the domain model and persistence adapters. It intentionally
//! has no dependency on SQLite, configuration, or a frontend so each layer depends inward.

#![forbid(unsafe_code)]

use std::error::Error;

use carver_domain::{
    Category, CategoryAppearance, CategoryId, CategorySummary, Note, NoteId, NoteSummary, Revision,
    SearchHit, TrashContents, TrashPurgeResult,
};
use time::OffsetDateTime;

/// Monotonically increases whenever a library mutation commits.
///
/// Consumers use this value to determine whether a wake-up signal represents a visible library
/// change. It is deliberately opaque: callers must only compare values for equality.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LibraryRevision(pub u64);

/// Persistence port implemented by local and future remote Carver backends.
///
/// Implementations are owned by one worker thread. They must not retain GTK objects or call UI
/// code.
#[expect(
    clippy::missing_errors_doc,
    reason = "the backend trait documents its single associated error contract once"
)]
pub trait LibraryBackend: Send + 'static {
    /// Backend-specific error returned by an operation.
    type Error: Error + Send + Sync + 'static;

    /// Reads the current semantic library revision.
    fn change_revision(&self) -> Result<LibraryRevision, Self::Error>;
    /// Creates a category at the supplied time.
    fn create_category(&self, name: &str, now: OffsetDateTime) -> Result<Category, Self::Error>;
    /// Creates a category with an explicit visual identity at the supplied time.
    fn create_category_with_appearance(
        &self,
        name: &str,
        appearance: CategoryAppearance,
        now: OffsetDateTime,
    ) -> Result<Category, Self::Error>;
    /// Lists active categories in their display order.
    fn categories(&self) -> Result<Vec<Category>, Self::Error>;
    /// Lists active categories with their active-note counts in display order.
    fn categories_with_note_counts(&self) -> Result<Vec<CategorySummary>, Self::Error>;
    /// Counts active notes in a category.
    fn note_count(&self, category_id: CategoryId) -> Result<usize, Self::Error>;
    /// Renames a category at the supplied time.
    fn rename_category(
        &self,
        category_id: CategoryId,
        name: &str,
        now: OffsetDateTime,
    ) -> Result<Category, Self::Error>;
    /// Updates an active category's name and visual identity at the supplied time.
    fn update_category(
        &self,
        category_id: CategoryId,
        name: &str,
        appearance: CategoryAppearance,
        now: OffsetDateTime,
    ) -> Result<Category, Self::Error>;
    /// Moves a category to trash at the supplied time.
    fn trash_category(
        &self,
        category_id: CategoryId,
        now: OffsetDateTime,
    ) -> Result<(), Self::Error>;
    /// Restores a category from trash at the supplied time.
    fn restore_category(
        &self,
        category_id: CategoryId,
        now: OffsetDateTime,
    ) -> Result<(), Self::Error>;
    /// Creates a blank note at the supplied time.
    fn create_note(
        &self,
        category_id: CategoryId,
        now: OffsetDateTime,
    ) -> Result<Note, Self::Error>;
    /// Creates a note with canonical Carve source at the supplied time.
    fn create_note_with_source(
        &self,
        category_id: CategoryId,
        source: &str,
        now: OffsetDateTime,
    ) -> Result<Note, Self::Error>;
    /// Reads one note, excluding trashed notes.
    fn note(&self, note_id: NoteId) -> Result<Option<Note>, Self::Error>;
    /// Saves source guarded by its revision at the supplied time.
    fn save_note(
        &self,
        note_id: NoteId,
        revision: Revision,
        source: &str,
        now: OffsetDateTime,
    ) -> Result<Note, Self::Error>;
    /// Moves an active note to an active category without changing its content timestamp.
    fn move_note(
        &self,
        note_id: NoteId,
        category_id: CategoryId,
        now: OffsetDateTime,
    ) -> Result<Note, Self::Error>;
    /// Moves a note to trash at the supplied time.
    fn trash_note(&self, note_id: NoteId, now: OffsetDateTime) -> Result<(), Self::Error>;
    /// Restores a note from trash.
    fn restore_note(&self, note_id: NoteId) -> Result<(), Self::Error>;
    /// Lists recoverable trash contents.
    fn trash_contents(&self) -> Result<TrashContents, Self::Error>;
    /// Permanently removes trashed content and unreferenced managed assets.
    fn empty_trash(&self) -> Result<TrashPurgeResult, Self::Error>;
    /// Returns recent active notes, optionally filtered by category.
    fn recent_notes(
        &self,
        category_id: Option<CategoryId>,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<NoteSummary>, Self::Error>;
    /// Searches active notes by title and body.
    fn search(
        &self,
        query: &str,
        category_id: Option<CategoryId>,
        limit: usize,
    ) -> Result<Vec<SearchHit>, Self::Error>;
    /// Stores managed image bytes and returns their relative Carve path.
    fn store_asset(
        &self,
        note_id: NoteId,
        extension: &str,
        bytes: &[u8],
    ) -> Result<String, Self::Error>;
    /// Reads managed image bytes for one note.
    fn note_asset_bytes(
        &self,
        note_id: NoteId,
        relative_path: &str,
    ) -> Result<Option<Vec<u8>>, Self::Error>;
}
