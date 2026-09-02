//! SQLite persistence for Carver's managed note library.

#![forbid(unsafe_code)]

use std::{
    fs,
    path::{Path, PathBuf},
};

use carver_domain::{
    Category, CategoryId, Note, NoteId, NoteSummary, Revision, SearchHit, derive_content,
};
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
    /// SQLite reported a problem.
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
}

// CONTEXT: each public storage operation has a homogeneous `StorageError` contract;
// rustdoc documents the error enum once instead of duplicating identical sections.
#[expect(clippy::missing_errors_doc)]
impl SqliteLibrary {
    /// Opens a library and applies all known migrations.
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
    pub fn create_category(
        &self,
        name: &str,
        now: OffsetDateTime,
    ) -> Result<Category, StorageError> {
        let category = Category {
            id: CategoryId::new(),
            name: name.trim().to_owned(),
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

    /// Moves a category to trash; its active notes become hidden through the parent relationship.
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
    pub fn rename_category(
        &self,
        category_id: CategoryId,
        name: &str,
        now: OffsetDateTime,
    ) -> Result<Category, StorageError> {
        let name = name.trim();
        if name.is_empty() {
            return Err(StorageError::Corrupt(
                "category name cannot be empty".to_owned(),
            ));
        }
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
            params![id.to_string(), category_id.to_string(), derived.title, derived.plain_text, timestamp(now)],
        )?;
        self.replace_fts(id, &derive_content(""))?;
        self.note(id)?
            .ok_or_else(|| StorageError::Corrupt("new note was not persisted".to_owned()))
    }

    /// Loads a complete active or trashed note.
    pub fn note(&self, note_id: NoteId) -> Result<Option<Note>, StorageError> {
        self.connection.query_row(
            "SELECT id, category_id, source, title, plain_text, revision, created_at, updated_at, trashed_at
             FROM notes WHERE id = ?1",
            [note_id.to_string()],
            note_from_row,
        ).optional().map_err(Into::into)
    }

    /// Saves source if the caller still owns the supplied revision.
    pub fn save_note(
        &self,
        note_id: NoteId,
        expected_revision: Revision,
        source: &str,
        now: OffsetDateTime,
    ) -> Result<Note, StorageError> {
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

    /// Lists recent notes, optionally restricted to one category.
    pub fn recent_notes(
        &self,
        category_id: Option<CategoryId>,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<NoteSummary>, StorageError> {
        let category = category_id.map(|id| id.to_string());
        let mut statement = self.connection.prepare(
            "SELECT n.id, n.category_id, n.title, n.plain_text, n.updated_at,
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
            "SELECT n.id, n.category_id, n.title, n.plain_text, n.updated_at,
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
                    snippet: row.get(6)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    /// Moves a note into the in-app trash.
    pub fn trash_note(&self, note_id: NoteId, now: OffsetDateTime) -> Result<(), StorageError> {
        self.connection.execute(
            "UPDATE notes SET trashed_at = ?2 WHERE id = ?1",
            params![note_id.to_string(), timestamp(now)],
        )?;
        Ok(())
    }

    /// Restores a note from trash.
    pub fn restore_note(&self, note_id: NoteId) -> Result<(), StorageError> {
        self.connection.execute(
            "UPDATE notes SET trashed_at = NULL WHERE id = ?1",
            [note_id.to_string()],
        )?;
        Ok(())
    }

    /// Adds bytes to the managed asset store and returns its portable source path.
    pub fn store_asset(
        &self,
        note_id: NoteId,
        extension: &str,
        bytes: &[u8],
    ) -> Result<String, StorageError> {
        let digest = format!("{:x}", Sha256::digest(bytes));
        let safe_extension = extension.trim_start_matches('.').to_ascii_lowercase();
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
                digest,
                filename,
                i64::try_from(bytes.len()).unwrap_or(i64::MAX)
            ],
        )?;
        self.connection.execute(
            "INSERT OR IGNORE INTO note_assets (note_id, asset_hash) VALUES (?1, ?2)",
            params![note_id.to_string(), format!("{:x}", Sha256::digest(bytes))],
        )?;
        Ok(format!("assets/{filename}"))
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
        for entry in fs::read_dir(&self.assets_dir)? {
            let entry = entry?;
            if entry
                .path()
                .extension()
                .is_some_and(|extension| extension == "partial")
            {
                fs::remove_file(entry.path())?;
            }
        }
        Ok(())
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
    let plain_text: String = row.get(3)?;
    Ok(NoteSummary {
        id: note_id(&row.get::<_, String>(0)?).map_err(to_sql_error)?,
        category_id: category_id(&row.get::<_, String>(1)?).map_err(to_sql_error)?,
        title: row.get(2)?,
        excerpt: plain_text.chars().take(180).collect(),
        updated_at: parse_timestamp(row.get(4)?).map_err(to_sql_error)?,
        has_images: row.get(5)?,
    })
}

fn category_id(value: &str) -> Result<CategoryId, StorageError> {
    Uuid::parse_str(value)
        .map(CategoryId::from_uuid)
        .map_err(|error| StorageError::Corrupt(error.to_string()))
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
mod tests {
    use super::*;
    fn library() -> (tempfile::TempDir, SqliteLibrary) {
        let directory = tempfile::tempdir()
            .unwrap_or_else(|error| panic!("temporary directory failed: {error}"));
        let library = SqliteLibrary::open(
            &directory.path().join("library.sqlite3"),
            &directory.path().join("assets"),
        )
        .unwrap_or_else(|error| panic!("library open failed: {error}"));
        (directory, library)
    }
    #[test]
    fn fts_search_finds_saved_notes() {
        let (_directory, library) = library();
        let now = OffsetDateTime::now_utc();
        let category = library
            .create_category("Work", now)
            .unwrap_or_else(|error| panic!("category failed: {error}"));
        let note = library
            .create_note(category.id, now)
            .unwrap_or_else(|error| panic!("note failed: {error}"));
        let _saved = library
            .save_note(
                note.id,
                note.revision,
                "# Roadmap\n\nShip the Carve editor",
                now,
            )
            .unwrap_or_else(|error| panic!("save failed: {error}"));
        let results = library
            .search_notes("Carve", None, 20)
            .unwrap_or_else(|error| panic!("search failed: {error}"));
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].note.title, "Roadmap");
    }
    #[test]
    fn trashed_category_hides_its_notes() {
        let (_directory, library) = library();
        let now = OffsetDateTime::now_utc();
        let category = library
            .create_category("Work", now)
            .unwrap_or_else(|error| panic!("category failed: {error}"));
        let _note = library
            .create_note(category.id, now)
            .unwrap_or_else(|error| panic!("note failed: {error}"));
        library
            .trash_category(category.id, now)
            .unwrap_or_else(|error| panic!("trash failed: {error}"));
        let notes = library
            .recent_notes(None, 20, 0)
            .unwrap_or_else(|error| panic!("list failed: {error}"));
        assert!(notes.is_empty());
    }
}
