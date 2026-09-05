//! `SQLite` persistence for Carver's managed note library.

#![forbid(unsafe_code)]

use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};

use carver_domain::{
    Category, CategoryId, CategorySummary, Note, NoteId, NoteSummary, Revision, SearchHit,
    TrashContents, TrashPurgeResult, TrashedCategorySummary, TrashedNoteSummary, derive_content,
};
use carver_sdk::LibraryBackend;
use rusqlite::{Connection, OptionalExtension, params};
use sha2::{Digest, Sha256};
use thiserror::Error;
use time::OffsetDateTime;
use uuid::Uuid;

/// SQLite-backed managed library.
pub struct SqliteLibrary {
    connection: Connection,
    assets_dir: PathBuf,
}

/// Storage-layer failures.
#[derive(Debug, Error)]
pub enum StorageError {
    /// A category name was empty after trimming whitespace.
    #[error("category name cannot be empty")]
    InvalidCategoryName,
    /// An asset extension was not part of the supported image format allowlist.
    #[error("unsupported asset extension: {0}")]
    UnsupportedAssetExtension(String),
    /// `SQLite` reported a problem.
    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),
    /// Asset filesystem work failed.
    #[error("asset I/O failed: {0}")]
    Io(#[from] std::io::Error),
    /// A stored identifier or timestamp was malformed.
    #[error("corrupt stored value: {0}")]
    Corrupt(String),
    /// An optimistic save did not match the current note revision.
    #[error("note was changed by another session")]
    Conflict,
    /// A note or destination category was missing or no longer active.
    #[error("note or destination category is unavailable")]
    MoveUnavailable,
}

impl SqliteLibrary {
    /// Opens a library and applies all known migrations.
    ///
    /// # Errors
    ///
    /// Returns an error when the database, its parent directory, managed assets, or
    /// migration schema cannot be opened or prepared.
    pub fn open(database_path: &Path, assets_dir: &Path) -> Result<Self, StorageError> {
        if let Some(parent) = database_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::create_dir_all(assets_dir)?;
        let connection = Connection::open(database_path)?;
        connection.execute_batch(
            "PRAGMA foreign_keys = ON;
             PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;",
        )?;
        let library = Self {
            connection,
            assets_dir: assets_dir.to_owned(),
        };
        library.migrate()?;
        library.cleanup_orphan_assets()?;
        Ok(library)
    }

    /// Creates a category at the end of the sidebar.
    ///
    /// # Errors
    ///
    /// Returns an error when the next position cannot be queried or the category
    /// cannot be persisted.
    pub fn create_category(
        &self,
        name: &str,
        now: OffsetDateTime,
    ) -> Result<Category, StorageError> {
        let name = category_name(name)?;
        let category = Category {
            id: CategoryId::new(),
            name: name.to_owned(),
            position: self.next_category_position()?,
            created_at: now,
            updated_at: now,
            trashed_at: None,
        };
        self.connection.execute(
            "INSERT INTO categories (id, name, position, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![category.id.to_string(), category.name, category.position, timestamp(now), timestamp(now)],
        )?;
        Ok(category)
    }

    /// Lists active categories in explicit sidebar order.
    ///
    /// # Errors
    ///
    /// Returns an error when categories cannot be read or stored values are corrupt.
    pub fn list_categories(&self) -> Result<Vec<Category>, StorageError> {
        let mut statement = self.connection.prepare(
            "SELECT id, name, position, created_at, updated_at, trashed_at
             FROM categories WHERE trashed_at IS NULL ORDER BY position, name COLLATE NOCASE",
        )?;
        statement
            .query_map([], category_from_row)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    /// Lists active categories with their active-note counts in sidebar order.
    ///
    /// # Errors
    ///
    /// Returns an error when category summaries cannot be read or stored values are corrupt.
    pub fn list_category_summaries(&self) -> Result<Vec<CategorySummary>, StorageError> {
        let mut statement = self.connection.prepare(
            "SELECT c.id, c.name, c.position, c.created_at, c.updated_at, c.trashed_at,
                    COUNT(n.id)
             FROM categories c
             LEFT JOIN notes n ON n.category_id = c.id AND n.trashed_at IS NULL
             WHERE c.trashed_at IS NULL
             GROUP BY c.id
             ORDER BY c.position, c.name COLLATE NOCASE",
        )?;
        statement
            .query_map([], category_summary_from_row)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    /// Counts the active notes in one active category.
    ///
    /// # Errors
    ///
    /// Returns an error when the count cannot be queried or converted for this platform.
    pub fn note_count(&self, category_id: CategoryId) -> Result<usize, StorageError> {
        let count: i64 = self.connection.query_row(
            "SELECT COUNT(*) FROM notes n JOIN categories c ON c.id = n.category_id
             WHERE n.category_id = ?1 AND n.trashed_at IS NULL AND c.trashed_at IS NULL",
            [category_id.to_string()],
            |row| row.get(0),
        )?;
        usize::try_from(count)
            .map_err(|_| StorageError::Corrupt("note count does not fit usize".to_owned()))
    }

    /// Moves a category to trash; its active notes become hidden through the parent relationship.
    ///
    /// # Errors
    ///
    /// Returns an error when the category cannot be updated.
    pub fn trash_category(
        &self,
        category_id: CategoryId,
        now: OffsetDateTime,
    ) -> Result<(), StorageError> {
        self.connection.execute(
            "UPDATE categories SET trashed_at = ?2, updated_at = ?2 WHERE id = ?1",
            params![category_id.to_string(), timestamp(now)],
        )?;
        Ok(())
    }

    /// Restores a previously trashed category.
    ///
    /// # Errors
    ///
    /// Returns an error when the category cannot be updated.
    pub fn restore_category(
        &self,
        category_id: CategoryId,
        now: OffsetDateTime,
    ) -> Result<(), StorageError> {
        self.connection.execute(
            "UPDATE categories SET trashed_at = NULL, updated_at = ?2 WHERE id = ?1",
            params![category_id.to_string(), timestamp(now)],
        )?;
        Ok(())
    }

    /// Renames an active category while retaining its position and notes.
    ///
    /// # Errors
    ///
    /// Returns an error when the name is empty, the category is absent, or the
    /// category cannot be persisted.
    pub fn rename_category(
        &self,
        category_id: CategoryId,
        name: &str,
        now: OffsetDateTime,
    ) -> Result<Category, StorageError> {
        let name = category_name(name)?;
        let affected = self.connection.execute(
            "UPDATE categories SET name = ?2, updated_at = ?3 WHERE id = ?1 AND trashed_at IS NULL",
            params![category_id.to_string(), name, timestamp(now)],
        )?;
        if affected != 1 {
            return Err(StorageError::Corrupt("category was not found".to_owned()));
        }
        self.connection
            .query_row(
                "SELECT id, name, position, created_at, updated_at, trashed_at FROM categories WHERE id = ?1",
                [category_id.to_string()],
                category_from_row,
            )
            .map_err(Into::into)
    }

    /// Creates a note with empty Carve source.
    ///
    /// # Errors
    ///
    /// Returns an error when the note or its search index entry cannot be persisted.
    pub fn create_note(
        &self,
        category_id: CategoryId,
        now: OffsetDateTime,
    ) -> Result<Note, StorageError> {
        let id = NoteId::new();
        let derived = derive_content("");
        self.connection.execute(
            "INSERT INTO notes (id, category_id, source, title, plain_text, revision, created_at, updated_at)
             VALUES (?1, ?2, '', ?3, ?4, 1, ?5, ?5)",
            params![id.to_string(), category_id.to_string(), &derived.title, &derived.plain_text, timestamp(now)],
        )?;
        self.replace_fts(id, &derived)?;
        self.note(id)?
            .ok_or_else(|| StorageError::Corrupt("new note was not persisted".to_owned()))
    }

    /// Creates a note with canonical Carve source and its derived search data.
    ///
    /// # Errors
    ///
    /// Returns an error when the note or its search index entry cannot be persisted.
    pub fn create_note_with_source(
        &self,
        category_id: CategoryId,
        source: &str,
        now: OffsetDateTime,
    ) -> Result<Note, StorageError> {
        let id = NoteId::new();
        let derived = derive_content(source);
        let transaction = self.connection.unchecked_transaction()?;
        transaction.execute(
            "INSERT INTO notes (id, category_id, source, title, plain_text, revision, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, 1, ?6, ?6)",
            params![id.to_string(), category_id.to_string(), source, derived.title, derived.plain_text, timestamp(now)],
        )?;
        transaction.execute("DELETE FROM note_fts WHERE note_id = ?1", [id.to_string()])?;
        transaction.execute(
            "INSERT INTO note_fts (note_id, title, plain_text) VALUES (?1, ?2, ?3)",
            params![id.to_string(), &derived.title, &derived.plain_text],
        )?;
        transaction.commit()?;
        self.note(id)?
            .ok_or_else(|| StorageError::Corrupt("imported note was not persisted".to_owned()))
    }

    /// Loads a complete active or trashed note.
    ///
    /// # Errors
    ///
    /// Returns an error when the note cannot be read or stored values are corrupt.
    pub fn note(&self, note_id: NoteId) -> Result<Option<Note>, StorageError> {
        self.connection.query_row(
            "SELECT id, category_id, source, title, plain_text, revision, created_at, updated_at, trashed_at
             FROM notes WHERE id = ?1",
            [note_id.to_string()],
            note_from_row,
        ).optional().map_err(Into::into)
    }

    /// Saves source if the caller still owns the supplied revision.
    ///
    /// # Errors
    ///
    /// Returns an error when the revision conflicts, the note is unavailable, or the
    /// note and its search index cannot be updated.
    pub fn save_note(
        &self,
        note_id: NoteId,
        expected_revision: Revision,
        source: &str,
        now: OffsetDateTime,
    ) -> Result<Note, StorageError> {
        let existing = self.note(note_id)?.ok_or(StorageError::Conflict)?;
        if existing.revision != expected_revision || existing.trashed_at.is_some() {
            return Err(StorageError::Conflict);
        }
        if existing.source == source {
            return Ok(existing);
        }
        let derived = derive_content(source);
        let affected = self.connection.execute(
            "UPDATE notes SET source = ?3, title = ?4, plain_text = ?5, revision = revision + 1, updated_at = ?6
             WHERE id = ?1 AND revision = ?2 AND trashed_at IS NULL",
            params![note_id.to_string(), expected_revision.0, source, derived.title, derived.plain_text, timestamp(now)],
        )?;
        if affected != 1 {
            return Err(StorageError::Conflict);
        }
        self.replace_fts(note_id, &derived)?;
        self.note(note_id)?
            .ok_or_else(|| StorageError::Corrupt("saved note was not found".to_owned()))
    }

    /// Moves an active note to an active category without changing its content timestamp.
    ///
    /// # Errors
    ///
    /// Returns an error when the note or destination category is unavailable, or the update
    /// cannot be persisted.
    pub fn move_note(
        &self,
        note_id: NoteId,
        category_id: CategoryId,
        _now: OffsetDateTime,
    ) -> Result<Note, StorageError> {
        let affected = self.connection.execute(
            "UPDATE notes SET category_id = ?2, revision = revision + 1
             WHERE id = ?1 AND trashed_at IS NULL
               AND EXISTS (
                   SELECT 1 FROM categories WHERE id = ?2 AND trashed_at IS NULL
               )",
            params![note_id.to_string(), category_id.to_string()],
        )?;
        if affected != 1 {
            return Err(StorageError::MoveUnavailable);
        }
        self.note(note_id)?
            .ok_or_else(|| StorageError::Corrupt("moved note was not found".to_owned()))
    }

    /// Lists recent notes, optionally restricted to one category.
    ///
    /// # Errors
    ///
    /// Returns an error when notes cannot be read or stored values are corrupt.
    pub fn recent_notes(
        &self,
        category_id: Option<CategoryId>,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<NoteSummary>, StorageError> {
        let category = category_id.map(|id| id.to_string());
        let mut statement = self.connection.prepare(
            "SELECT n.id, n.category_id, c.name, n.title, n.plain_text, n.updated_at,
                    EXISTS(SELECT 1 FROM note_assets a WHERE a.note_id = n.id)
             FROM notes n JOIN categories c ON c.id = n.category_id
             WHERE n.trashed_at IS NULL AND c.trashed_at IS NULL
               AND (?1 IS NULL OR n.category_id = ?1)
             ORDER BY n.updated_at DESC LIMIT ?2 OFFSET ?3",
        )?;
        let limit = i64::try_from(limit).unwrap_or(i64::MAX);
        let offset = i64::try_from(offset).unwrap_or(i64::MAX);
        statement
            .query_map(params![category, limit, offset], summary_from_row)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    /// Runs a full-text search over active notes.
    ///
    /// # Errors
    ///
    /// Returns an error when the search index cannot be queried or stored values are corrupt.
    pub fn search_notes(
        &self,
        query: &str,
        category_id: Option<CategoryId>,
        limit: usize,
    ) -> Result<Vec<SearchHit>, StorageError> {
        if query.trim().is_empty() {
            return Ok(Vec::new());
        }
        let category = category_id.map(|id| id.to_string());
        let mut statement = self.connection.prepare(
            "SELECT n.id, n.category_id, c.name, n.title, n.plain_text, n.updated_at,
                    EXISTS(SELECT 1 FROM note_assets a WHERE a.note_id = n.id),
                    snippet(note_fts, 2, '', '', '…', 14)
             FROM note_fts JOIN notes n ON n.id = note_fts.note_id
             JOIN categories c ON c.id = n.category_id
             WHERE note_fts MATCH ?1 AND n.trashed_at IS NULL AND c.trashed_at IS NULL
               AND (?2 IS NULL OR n.category_id = ?2)
             ORDER BY bm25(note_fts), n.updated_at DESC LIMIT ?3",
        )?;
        let limit = i64::try_from(limit).unwrap_or(i64::MAX);
        statement
            .query_map(params![fts_query(query), category, limit], |row| {
                Ok(SearchHit {
                    note: summary_from_row(row)?,
                    snippet: row.get(7)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    /// Moves a note into the in-app trash.
    ///
    /// # Errors
    ///
    /// Returns an error when the note cannot be updated.
    pub fn trash_note(&self, note_id: NoteId, now: OffsetDateTime) -> Result<(), StorageError> {
        self.connection.execute(
            "UPDATE notes SET trashed_at = ?2 WHERE id = ?1",
            params![note_id.to_string(), timestamp(now)],
        )?;
        Ok(())
    }

    /// Restores a note from trash.
    ///
    /// # Errors
    ///
    /// Returns an error when the note cannot be updated.
    pub fn restore_note(&self, note_id: NoteId) -> Result<(), StorageError> {
        self.connection.execute(
            "UPDATE notes SET trashed_at = NULL WHERE id = ?1",
            [note_id.to_string()],
        )?;
        Ok(())
    }

    /// Lists the top-level items available for recovery from trash.
    ///
    /// Notes belonging to a trashed category are represented by that category rather than
    /// duplicated as individual recovery actions.
    ///
    /// # Errors
    ///
    /// Returns an error when the trash cannot be read or stored values are corrupt.
    pub fn trash_contents(&self) -> Result<TrashContents, StorageError> {
        let mut categories = self.connection.prepare(
            "SELECT c.id, c.name, c.position, c.created_at, c.updated_at, c.trashed_at,
                    COUNT(n.id)
             FROM categories c
             LEFT JOIN notes n ON n.category_id = c.id AND n.trashed_at IS NULL
             WHERE c.trashed_at IS NOT NULL
             GROUP BY c.id
             ORDER BY c.trashed_at DESC",
        )?;
        let categories = categories
            .query_map([], trashed_category_summary_from_row)?
            .collect::<Result<Vec<_>, _>>()?;
        let mut notes = self.connection.prepare(
            "SELECT n.id, n.category_id, c.name, n.title, n.plain_text, n.trashed_at,
                    EXISTS(SELECT 1 FROM note_assets a WHERE a.note_id = n.id)
             FROM notes n
             JOIN categories c ON c.id = n.category_id
             WHERE n.trashed_at IS NOT NULL AND c.trashed_at IS NULL
             ORDER BY n.trashed_at DESC",
        )?;
        let notes = notes
            .query_map([], trashed_note_summary_from_row)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(TrashContents { categories, notes })
    }

    /// Permanently removes all trashed notes, categories, and unreferenced managed assets.
    ///
    /// # Errors
    ///
    /// Returns an error when database or managed-asset cleanup fails.
    pub fn empty_trash(&self) -> Result<TrashPurgeResult, StorageError> {
        let transaction = self.connection.unchecked_transaction()?;
        transaction.execute(
            "DELETE FROM note_fts WHERE note_id IN (
                 SELECT n.id FROM notes n JOIN categories c ON c.id = n.category_id
                 WHERE n.trashed_at IS NOT NULL OR c.trashed_at IS NOT NULL
             )",
            [],
        )?;
        let notes_deleted = transaction.execute(
            "DELETE FROM notes WHERE id IN (
                 SELECT n.id FROM notes n JOIN categories c ON c.id = n.category_id
                 WHERE n.trashed_at IS NOT NULL OR c.trashed_at IS NOT NULL
             )",
            [],
        )?;
        let categories_deleted =
            transaction.execute("DELETE FROM categories WHERE trashed_at IS NOT NULL", [])?;
        let mut orphan_assets = transaction.prepare(
            "SELECT filename FROM assets
             WHERE NOT EXISTS (SELECT 1 FROM note_assets WHERE note_assets.asset_hash = assets.hash)",
        )?;
        let orphan_filenames = orphan_assets
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        let orphan_paths = orphan_filenames
            .iter()
            .map(|filename| self.managed_asset_path(filename))
            .collect::<Result<Vec<_>, _>>()?;
        drop(orphan_assets);
        let assets_deleted = transaction.execute(
            "DELETE FROM assets
             WHERE NOT EXISTS (SELECT 1 FROM note_assets WHERE note_assets.asset_hash = assets.hash)",
            [],
        )?;
        transaction.commit()?;
        for path in orphan_paths {
            if path.exists() {
                fs::remove_file(path)?;
            }
        }
        Ok(TrashPurgeResult {
            categories_deleted,
            notes_deleted,
            assets_deleted,
        })
    }

    /// Adds bytes to the managed asset store and returns its portable source path.
    ///
    /// # Errors
    ///
    /// Returns an error when bytes cannot be written or their metadata cannot be persisted.
    pub fn store_asset(
        &self,
        note_id: NoteId,
        extension: &str,
        bytes: &[u8],
    ) -> Result<String, StorageError> {
        let digest = format!("{:x}", Sha256::digest(bytes));
        let safe_extension = asset_extension(extension)?;
        let filename = format!("{digest}.{safe_extension}");
        let path = self.assets_dir.join(&filename);
        if !path.exists() {
            let temporary = path.with_extension("partial");
            fs::write(&temporary, bytes)?;
            fs::rename(temporary, &path)?;
        }
        self.connection.execute(
            "INSERT OR IGNORE INTO assets (hash, filename, byte_size) VALUES (?1, ?2, ?3)",
            params![
                &digest,
                &filename,
                i64::try_from(bytes.len()).unwrap_or(i64::MAX)
            ],
        )?;
        self.connection.execute(
            "INSERT OR IGNORE INTO note_assets (note_id, asset_hash) VALUES (?1, ?2)",
            params![note_id.to_string(), &digest],
        )?;
        Ok(format!("assets/{filename}"))
    }

    /// Reads a managed image only when it belongs to the requested note.
    ///
    /// # Errors
    ///
    /// Returns an error when the asset path is unsafe or the managed file cannot be read.
    pub fn note_asset_bytes(
        &self,
        note_id: NoteId,
        relative_path: &str,
    ) -> Result<Option<Vec<u8>>, StorageError> {
        let Some(filename) = relative_path.strip_prefix("assets/") else {
            return Ok(None);
        };
        let attached: Option<String> = self
            .connection
            .query_row(
                "SELECT a.filename FROM assets a JOIN note_assets na ON na.asset_hash = a.hash
             WHERE na.note_id = ?1 AND a.filename = ?2",
                params![note_id.to_string(), filename],
                |row| row.get(0),
            )
            .optional()?;
        let Some(filename) = attached else {
            return Ok(None);
        };
        Ok(Some(fs::read(self.managed_asset_path(&filename)?)?))
    }

    fn migrate(&self) -> Result<(), StorageError> {
        self.connection.execute_batch(
            "CREATE TABLE IF NOT EXISTS categories (
                id TEXT PRIMARY KEY NOT NULL, name TEXT NOT NULL CHECK(length(trim(name)) > 0),
                position INTEGER NOT NULL, created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL, trashed_at INTEGER
            );
            CREATE TABLE IF NOT EXISTS notes (
                id TEXT PRIMARY KEY NOT NULL, category_id TEXT NOT NULL REFERENCES categories(id), source TEXT NOT NULL,
                title TEXT NOT NULL, plain_text TEXT NOT NULL, revision INTEGER NOT NULL, created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL, trashed_at INTEGER
            );
            CREATE INDEX IF NOT EXISTS notes_updated_at_idx ON notes(updated_at DESC);
            CREATE INDEX IF NOT EXISTS notes_category_updated_at_idx ON notes(category_id, updated_at DESC);
            CREATE TABLE IF NOT EXISTS assets (hash TEXT PRIMARY KEY NOT NULL, filename TEXT NOT NULL UNIQUE, byte_size INTEGER NOT NULL);
            CREATE TABLE IF NOT EXISTS note_assets (
                note_id TEXT NOT NULL REFERENCES notes(id) ON DELETE CASCADE,
                asset_hash TEXT NOT NULL REFERENCES assets(hash) ON DELETE CASCADE,
                PRIMARY KEY(note_id, asset_hash)
            );
            CREATE VIRTUAL TABLE IF NOT EXISTS note_fts USING fts5(note_id UNINDEXED, title, plain_text);",
        )?;
        Ok(())
    }

    fn replace_fts(
        &self,
        note_id: NoteId,
        derived: &carver_domain::DerivedContent,
    ) -> Result<(), StorageError> {
        self.connection.execute(
            "DELETE FROM note_fts WHERE note_id = ?1",
            [note_id.to_string()],
        )?;
        self.connection.execute(
            "INSERT INTO note_fts (note_id, title, plain_text) VALUES (?1, ?2, ?3)",
            params![note_id.to_string(), derived.title, derived.plain_text],
        )?;
        Ok(())
    }

    fn next_category_position(&self) -> Result<i64, StorageError> {
        self.connection
            .query_row(
                "SELECT COALESCE(MAX(position), -1) + 1 FROM categories",
                [],
                |row| row.get(0),
            )
            .map_err(Into::into)
    }

    fn cleanup_orphan_assets(&self) -> Result<(), StorageError> {
        let mut statement = self.connection.prepare("SELECT filename FROM assets")?;
        let known_assets = statement
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<Result<HashSet<_>, _>>()?;
        for entry in fs::read_dir(&self.assets_dir)? {
            let entry = entry?;
            let path = entry.path();
            let is_partial = path
                .extension()
                .is_some_and(|extension| extension == "partial");
            let is_orphan = entry.file_type()?.is_file()
                && entry
                    .file_name()
                    .to_str()
                    .is_some_and(|filename| !known_assets.contains(filename));
            if is_partial || is_orphan {
                fs::remove_file(entry.path())?;
            }
        }
        Ok(())
    }

    fn managed_asset_path(&self, filename: &str) -> Result<PathBuf, StorageError> {
        let path = Path::new(filename);
        let is_single_filename = path.components().count() == 1
            && path.file_name().and_then(|value| value.to_str()) == Some(filename);
        if !is_single_filename {
            return Err(StorageError::Corrupt(format!(
                "managed asset filename is unsafe: {filename}"
            )));
        }
        Ok(self.assets_dir.join(path))
    }
}

impl LibraryBackend for SqliteLibrary {
    type Error = StorageError;

    fn create_category(&self, name: &str, now: OffsetDateTime) -> Result<Category, Self::Error> {
        Self::create_category(self, name, now)
    }

    fn categories(&self) -> Result<Vec<Category>, Self::Error> {
        self.list_categories()
    }

    fn categories_with_note_counts(&self) -> Result<Vec<CategorySummary>, Self::Error> {
        self.list_category_summaries()
    }

    fn note_count(&self, category_id: CategoryId) -> Result<usize, Self::Error> {
        Self::note_count(self, category_id)
    }

    fn rename_category(
        &self,
        category_id: CategoryId,
        name: &str,
        now: OffsetDateTime,
    ) -> Result<Category, Self::Error> {
        Self::rename_category(self, category_id, name, now)
    }

    fn trash_category(
        &self,
        category_id: CategoryId,
        now: OffsetDateTime,
    ) -> Result<(), Self::Error> {
        Self::trash_category(self, category_id, now)
    }

    fn restore_category(
        &self,
        category_id: CategoryId,
        now: OffsetDateTime,
    ) -> Result<(), Self::Error> {
        Self::restore_category(self, category_id, now)
    }

    fn create_note(
        &self,
        category_id: CategoryId,
        now: OffsetDateTime,
    ) -> Result<Note, Self::Error> {
        Self::create_note(self, category_id, now)
    }

    fn create_note_with_source(
        &self,
        category_id: CategoryId,
        source: &str,
        now: OffsetDateTime,
    ) -> Result<Note, Self::Error> {
        Self::create_note_with_source(self, category_id, source, now)
    }

    fn note(&self, note_id: NoteId) -> Result<Option<Note>, Self::Error> {
        Self::note(self, note_id)
    }

    fn save_note(
        &self,
        note_id: NoteId,
        revision: Revision,
        source: &str,
        now: OffsetDateTime,
    ) -> Result<Note, Self::Error> {
        Self::save_note(self, note_id, revision, source, now)
    }

    fn move_note(
        &self,
        note_id: NoteId,
        category_id: CategoryId,
        now: OffsetDateTime,
    ) -> Result<Note, Self::Error> {
        Self::move_note(self, note_id, category_id, now)
    }

    fn trash_note(&self, note_id: NoteId, now: OffsetDateTime) -> Result<(), Self::Error> {
        Self::trash_note(self, note_id, now)
    }

    fn restore_note(&self, note_id: NoteId) -> Result<(), Self::Error> {
        Self::restore_note(self, note_id)
    }

    fn trash_contents(&self) -> Result<TrashContents, Self::Error> {
        Self::trash_contents(self)
    }

    fn empty_trash(&self) -> Result<TrashPurgeResult, Self::Error> {
        Self::empty_trash(self)
    }

    fn recent_notes(
        &self,
        category_id: Option<CategoryId>,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<NoteSummary>, Self::Error> {
        Self::recent_notes(self, category_id, limit, offset)
    }

    fn search(
        &self,
        query: &str,
        category_id: Option<CategoryId>,
        limit: usize,
    ) -> Result<Vec<SearchHit>, Self::Error> {
        self.search_notes(query, category_id, limit)
    }

    fn store_asset(
        &self,
        note_id: NoteId,
        extension: &str,
        bytes: &[u8],
    ) -> Result<String, Self::Error> {
        Self::store_asset(self, note_id, extension, bytes)
    }

    fn note_asset_bytes(
        &self,
        note_id: NoteId,
        relative_path: &str,
    ) -> Result<Option<Vec<u8>>, Self::Error> {
        Self::note_asset_bytes(self, note_id, relative_path)
    }
}

fn category_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Category> {
    Ok(Category {
        id: category_id(&row.get::<_, String>(0)?).map_err(to_sql_error)?,
        name: row.get(1)?,
        position: row.get(2)?,
        created_at: parse_timestamp(row.get(3)?).map_err(to_sql_error)?,
        updated_at: parse_timestamp(row.get(4)?).map_err(to_sql_error)?,
        trashed_at: row
            .get::<_, Option<i64>>(5)?
            .map(parse_timestamp)
            .transpose()
            .map_err(to_sql_error)?,
    })
}

fn category_summary_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<CategorySummary> {
    let note_count: i64 = row.get(6)?;
    let note_count = usize::try_from(note_count).map_err(|_| {
        to_sql_error(StorageError::Corrupt(
            "note count does not fit usize".to_owned(),
        ))
    })?;
    Ok(CategorySummary {
        category: category_from_row(row)?,
        note_count,
    })
}

fn note_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Note> {
    Ok(Note {
        id: note_id(&row.get::<_, String>(0)?).map_err(to_sql_error)?,
        category_id: category_id(&row.get::<_, String>(1)?).map_err(to_sql_error)?,
        source: row.get(2)?,
        title: row.get(3)?,
        plain_text: row.get(4)?,
        revision: Revision(row.get(5)?),
        created_at: parse_timestamp(row.get(6)?).map_err(to_sql_error)?,
        updated_at: parse_timestamp(row.get(7)?).map_err(to_sql_error)?,
        trashed_at: row
            .get::<_, Option<i64>>(8)?
            .map(parse_timestamp)
            .transpose()
            .map_err(to_sql_error)?,
    })
}

fn summary_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<NoteSummary> {
    let plain_text: String = row.get(4)?;
    Ok(NoteSummary {
        id: note_id(&row.get::<_, String>(0)?).map_err(to_sql_error)?,
        category_id: category_id(&row.get::<_, String>(1)?).map_err(to_sql_error)?,
        category_name: row.get(2)?,
        title: row.get(3)?,
        excerpt: plain_text.chars().take(180).collect(),
        updated_at: parse_timestamp(row.get(5)?).map_err(to_sql_error)?,
        has_images: row.get(6)?,
    })
}

fn trashed_category_summary_from_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<TrashedCategorySummary> {
    let recoverable_note_count: i64 = row.get(6)?;
    let recoverable_note_count = usize::try_from(recoverable_note_count).map_err(|_| {
        to_sql_error(StorageError::Corrupt(
            "recoverable note count does not fit usize".to_owned(),
        ))
    })?;
    Ok(TrashedCategorySummary {
        category: category_from_row(row)?,
        recoverable_note_count,
    })
}

fn trashed_note_summary_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<TrashedNoteSummary> {
    let trashed_at: i64 = row.get(5)?;
    Ok(TrashedNoteSummary {
        id: note_id(&row.get::<_, String>(0)?).map_err(to_sql_error)?,
        category_id: category_id(&row.get::<_, String>(1)?).map_err(to_sql_error)?,
        category_name: row.get(2)?,
        title: row.get(3)?,
        excerpt: row.get::<_, String>(4)?.chars().take(180).collect(),
        trashed_at: parse_timestamp(trashed_at).map_err(to_sql_error)?,
        has_images: row.get(6)?,
    })
}

fn category_id(value: &str) -> Result<CategoryId, StorageError> {
    Uuid::parse_str(value)
        .map(CategoryId::from_uuid)
        .map_err(|error| StorageError::Corrupt(error.to_string()))
}

fn category_name(name: &str) -> Result<&str, StorageError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(StorageError::InvalidCategoryName);
    }
    Ok(name)
}

fn asset_extension(extension: &str) -> Result<&'static str, StorageError> {
    match extension
        .trim_start_matches('.')
        .to_ascii_lowercase()
        .as_str()
    {
        "png" => Ok("png"),
        "jpg" | "jpeg" => Ok("jpg"),
        "gif" => Ok("gif"),
        "webp" => Ok("webp"),
        "svg" => Ok("svg"),
        _ => Err(StorageError::UnsupportedAssetExtension(
            extension.to_owned(),
        )),
    }
}
fn note_id(value: &str) -> Result<NoteId, StorageError> {
    Uuid::parse_str(value)
        .map(NoteId::from_uuid)
        .map_err(|error| StorageError::Corrupt(error.to_string()))
}
fn timestamp(value: OffsetDateTime) -> i64 {
    value.unix_timestamp()
}
fn parse_timestamp(value: i64) -> Result<OffsetDateTime, StorageError> {
    OffsetDateTime::from_unix_timestamp(value)
        .map_err(|error| StorageError::Corrupt(error.to_string()))
}
fn to_sql_error(error: StorageError) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(error))
}
fn fts_query(query: &str) -> String {
    query
        .split_whitespace()
        .map(|term| format!("\"{}\"*", term.replace('"', "")))
        .collect::<Vec<_>>()
        .join(" AND ")
}

#[cfg(test)]
mod tests;
