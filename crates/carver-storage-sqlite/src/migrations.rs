//! SQLite schema versioning and data migrations for the managed library.

use carver_domain::{derive_content, extract_note_link_targets, project_frontmatter};
use rusqlite::{Connection, Transaction, params};
use rusqlite_migration::{M, Migrations};

/// Applies every schema and data migration known to this version of Carver.
pub(crate) fn apply(connection: &mut Connection) -> Result<(), rusqlite_migration::Error> {
    migrations().to_latest(connection)
}

#[cfg(test)]
pub(crate) fn definitions() -> Migrations<'static> {
    migrations()
}

#[cfg(test)]
mod tests;

/// The complete schema for libraries created before schema versioning was introduced.
///
/// Existing libraries have SQLite's `user_version` set to zero, so this migration deliberately
/// uses idempotent DDL. Its hook fills the two category appearance columns that were added while
/// the schema was still created at application startup. Once committed, `user_version` is one and
/// this SQL is never run again.
const INITIAL_SCHEMA: &str = "
    CREATE TABLE IF NOT EXISTS categories (
        id TEXT PRIMARY KEY NOT NULL, name TEXT NOT NULL CHECK(length(trim(name)) > 0),
        icon TEXT NOT NULL DEFAULT 'folder', color TEXT NOT NULL DEFAULT 'auto', position INTEGER NOT NULL,
        created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL, trashed_at INTEGER
    );
    CREATE TABLE IF NOT EXISTS notes (
        id TEXT PRIMARY KEY NOT NULL, category_id TEXT NOT NULL REFERENCES categories(id), source TEXT NOT NULL,
        title TEXT NOT NULL, plain_text TEXT NOT NULL, revision INTEGER NOT NULL, created_at INTEGER NOT NULL,
        updated_at INTEGER NOT NULL, trashed_at INTEGER
    );
    CREATE INDEX IF NOT EXISTS notes_updated_at_idx ON notes(updated_at DESC);
    CREATE INDEX IF NOT EXISTS notes_category_updated_at_idx ON notes(category_id, updated_at DESC);
    CREATE TABLE IF NOT EXISTS assets (
        hash TEXT PRIMARY KEY NOT NULL, filename TEXT NOT NULL UNIQUE, byte_size INTEGER NOT NULL
    );
    CREATE TABLE IF NOT EXISTS note_assets (
        note_id TEXT NOT NULL REFERENCES notes(id) ON DELETE CASCADE,
        asset_hash TEXT NOT NULL REFERENCES assets(hash) ON DELETE CASCADE,
        PRIMARY KEY(note_id, asset_hash)
    );
    CREATE TABLE IF NOT EXISTS library_metadata (
        singleton INTEGER PRIMARY KEY CHECK(singleton = 1),
        change_revision INTEGER NOT NULL CHECK(change_revision >= 0)
    );
    INSERT OR IGNORE INTO library_metadata (singleton, change_revision) VALUES (1, 0);
    CREATE VIRTUAL TABLE IF NOT EXISTS note_fts USING fts5(note_id UNINDEXED, title, plain_text);
    CREATE TRIGGER IF NOT EXISTS categories_change_revision_after_insert
        AFTER INSERT ON categories
        BEGIN
            UPDATE library_metadata SET change_revision = change_revision + 1 WHERE singleton = 1;
        END;
    CREATE TRIGGER IF NOT EXISTS categories_change_revision_after_update
        AFTER UPDATE ON categories
        BEGIN
            UPDATE library_metadata SET change_revision = change_revision + 1 WHERE singleton = 1;
        END;
    CREATE TRIGGER IF NOT EXISTS categories_change_revision_after_delete
        AFTER DELETE ON categories
        BEGIN
            UPDATE library_metadata SET change_revision = change_revision + 1 WHERE singleton = 1;
        END;
    CREATE TRIGGER IF NOT EXISTS notes_change_revision_after_insert
        AFTER INSERT ON notes
        BEGIN
            UPDATE library_metadata SET change_revision = change_revision + 1 WHERE singleton = 1;
        END;
    CREATE TRIGGER IF NOT EXISTS notes_change_revision_after_update
        AFTER UPDATE ON notes
        BEGIN
            UPDATE library_metadata SET change_revision = change_revision + 1 WHERE singleton = 1;
        END;
    CREATE TRIGGER IF NOT EXISTS notes_change_revision_after_delete
        AFTER DELETE ON notes
        BEGIN
            UPDATE library_metadata SET change_revision = change_revision + 1 WHERE singleton = 1;
        END;
";

/// Reshapes managed assets from a globally shared, content-addressed table into note-owned
/// storage.
///
/// The legacy `assets`/`note_assets` tables are replaced by a single `assets` table keyed by
/// `(note_id, filename)`, and each note owns a directory on disk. The migration is intentionally
/// schema-only: it does not move legacy files or rewrite note source, so previously stored assets
/// are discarded and their flat files are left untouched. New assets are stored per note.
const ASSET_OWNERSHIP_SCHEMA: &str = "
    DROP TABLE IF EXISTS note_assets;
    DROP TABLE IF EXISTS assets;
    CREATE TABLE assets (
        note_id TEXT NOT NULL REFERENCES notes(id) ON DELETE CASCADE,
        filename TEXT NOT NULL, hash TEXT NOT NULL, byte_size INTEGER NOT NULL,
        PRIMARY KEY(note_id, filename)
    );
    CREATE INDEX IF NOT EXISTS assets_note_idx ON assets(note_id);
";

/// Indexes internal note-to-note links so outgoing links and backlinks can be queried.
///
/// The table is rebuilt per note whenever its source changes. Rows are scoped to their owning
/// note and cascade when a note is permanently removed, so the in-app trash keeps backlinks
/// intact until a hard delete.
const NOTE_LINKS_SCHEMA: &str = "
    CREATE TABLE IF NOT EXISTS note_links (
        source_note_id TEXT NOT NULL REFERENCES notes(id) ON DELETE CASCADE,
        target_note_id TEXT NOT NULL REFERENCES notes(id) ON DELETE CASCADE,
        PRIMARY KEY(source_note_id, target_note_id)
    );
    CREATE INDEX IF NOT EXISTS note_links_target_idx ON note_links(target_note_id);
";

fn migrations() -> Migrations<'static> {
    Migrations::new(vec![
        M::up_with_hook(INITIAL_SCHEMA, migrate_category_appearance_columns),
        M::up_with_hook("", migrate_note_favorite_columns),
        M::up_with_hook("", migrate_bases),
        M::up_with_hook("", migrate_derived_titles),
        M::up(ASSET_OWNERSHIP_SCHEMA),
        M::up_with_hook(NOTE_LINKS_SCHEMA, migrate_note_links),
        M::up_with_hook("", migrate_frontmatter_revision),
        M::up_with_hook("", migrate_note_search_category),
    ])
}

/// Adds the note's category name to the full-text search index.
///
/// The virtual table cannot be altered in place, so it is dropped and rebuilt from the stored
/// `title`/`plain_text` columns and the owning category's name. The `category` column is appended
/// last so the existing snippet column index (body plaintext at index two) stays valid. Trashed
/// notes keep their rows, matching the pre-migration behavior where the search query excludes
/// trashed notes and categories.
fn migrate_note_search_category(transaction: &Transaction<'_>) -> rusqlite_migration::HookResult {
    transaction.execute_batch(
        "DROP TABLE IF EXISTS note_fts;
        CREATE VIRTUAL TABLE note_fts USING fts5(note_id UNINDEXED, title, plain_text, category);
        INSERT INTO note_fts (note_id, title, plain_text, category)
            SELECT n.id, n.title, n.plain_text, c.name
            FROM notes n JOIN categories c ON c.id = n.category_id;",
    )?;
    Ok(())
}

/// Adds the frontmatter-affecting revision used to cache descriptor discovery.
///
/// The main `change_revision` bumps on every note update, including body- and title-only edits
/// that cannot change discovered descriptors. This counter only moves when a note's projected
/// frontmatter, membership, or trash state changes, so the descriptor cache survives the common
/// autosave path. The column is added defensively because legacy databases can be re-adopted from
/// `user_version = 0`.
fn migrate_frontmatter_revision(transaction: &Transaction<'_>) -> rusqlite_migration::HookResult {
    let mut statement = transaction.prepare("PRAGMA table_info(library_metadata)")?;
    let columns = statement
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<Result<Vec<_>, _>>()?;
    drop(statement);
    if !columns
        .iter()
        .any(|column| column == "frontmatter_revision")
    {
        transaction.execute_batch(
            "ALTER TABLE library_metadata ADD COLUMN frontmatter_revision INTEGER NOT NULL DEFAULT 0;",
        )?;
    }
    transaction.execute_batch(
        "CREATE TRIGGER IF NOT EXISTS notes_frontmatter_revision_after_insert
            AFTER INSERT ON notes
            BEGIN
                UPDATE library_metadata SET frontmatter_revision = frontmatter_revision + 1 WHERE singleton = 1;
            END;
        CREATE TRIGGER IF NOT EXISTS notes_frontmatter_revision_after_delete
            AFTER DELETE ON notes
            BEGIN
                UPDATE library_metadata SET frontmatter_revision = frontmatter_revision + 1 WHERE singleton = 1;
            END;
        CREATE TRIGGER IF NOT EXISTS notes_frontmatter_revision_after_update
            AFTER UPDATE ON notes
            WHEN NEW.frontmatter_json IS NOT OLD.frontmatter_json
              OR NEW.trashed_at IS NOT OLD.trashed_at
              OR NEW.category_id IS NOT OLD.category_id
            BEGIN
                UPDATE library_metadata SET frontmatter_revision = frontmatter_revision + 1 WHERE singleton = 1;
            END;
        CREATE TRIGGER IF NOT EXISTS categories_frontmatter_revision_after_update
            AFTER UPDATE ON categories
            WHEN NEW.trashed_at IS NOT OLD.trashed_at
            BEGIN
                UPDATE library_metadata SET frontmatter_revision = frontmatter_revision + 1 WHERE singleton = 1;
            END;",
    )?;
    Ok(())
}

fn migrate_note_links(transaction: &Transaction<'_>) -> rusqlite_migration::HookResult {
    let mut statement = transaction.prepare("SELECT id, source FROM notes")?;
    let notes = statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    drop(statement);

    for (id, source) in notes {
        for target in extract_note_link_targets(&source) {
            // Skip dangling targets; the foreign key would otherwise abort the migration.
            transaction.execute(
                "INSERT OR IGNORE INTO note_links (source_note_id, target_note_id)
                 SELECT ?1, ?2 WHERE EXISTS (SELECT 1 FROM notes WHERE id = ?2)",
                params![id, target.to_string()],
            )?;
        }
    }
    Ok(())
}

fn migrate_derived_titles(transaction: &Transaction<'_>) -> rusqlite_migration::HookResult {
    let mut statement = transaction.prepare("SELECT id, source, title FROM notes")?;
    let notes = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    drop(statement);

    for (id, source, title) in notes {
        let derived = derive_content(&source);
        if derived.title == title {
            continue;
        }
        transaction.execute(
            "UPDATE notes SET title = ?2 WHERE id = ?1",
            params![id, derived.title],
        )?;
        transaction.execute("DELETE FROM note_fts WHERE note_id = ?1", [&id])?;
        transaction.execute(
            "INSERT INTO note_fts (note_id, title, plain_text) VALUES (?1, ?2, ?3)",
            params![id, derived.title, derived.plain_text],
        )?;
    }
    Ok(())
}

fn migrate_bases(transaction: &Transaction<'_>) -> rusqlite_migration::HookResult {
    let mut statement = transaction.prepare("PRAGMA table_info(notes)")?;
    let columns = statement
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<Result<Vec<_>, _>>()?;
    if !columns.iter().any(|column| column == "frontmatter_json") {
        transaction.execute_batch("ALTER TABLE notes ADD COLUMN frontmatter_json TEXT CHECK(frontmatter_json IS NULL OR json_valid(frontmatter_json));")?;
    }
    if !columns.iter().any(|column| column == "frontmatter_format") {
        transaction.execute_batch("ALTER TABLE notes ADD COLUMN frontmatter_format TEXT;")?;
    }
    if !columns.iter().any(|column| column == "frontmatter_error") {
        transaction.execute_batch("ALTER TABLE notes ADD COLUMN frontmatter_error TEXT;")?;
    }
    transaction.execute_batch(
        "CREATE TABLE IF NOT EXISTS bases (
            id TEXT PRIMARY KEY NOT NULL,
            name TEXT NOT NULL COLLATE NOCASE UNIQUE CHECK(length(trim(name)) > 0),
            definition_json TEXT NOT NULL CHECK(json_valid(definition_json)),
            revision INTEGER NOT NULL CHECK(revision > 0)
        );
        CREATE TRIGGER IF NOT EXISTS bases_change_revision_after_insert AFTER INSERT ON bases BEGIN
            UPDATE library_metadata SET change_revision = change_revision + 1 WHERE singleton = 1;
        END;
        CREATE TRIGGER IF NOT EXISTS bases_change_revision_after_update AFTER UPDATE ON bases BEGIN
            UPDATE library_metadata SET change_revision = change_revision + 1 WHERE singleton = 1;
        END;
        CREATE TRIGGER IF NOT EXISTS bases_change_revision_after_delete AFTER DELETE ON bases BEGIN
            UPDATE library_metadata SET change_revision = change_revision + 1 WHERE singleton = 1;
        END;",
    )?;
    let mut notes = transaction.prepare("SELECT id, source FROM notes")?;
    let rows = notes.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;
    for row in rows {
        let (id, source) = row?;
        let projection = project_frontmatter(&source);
        transaction.execute(
            "UPDATE notes SET frontmatter_json = ?2, frontmatter_format = ?3, frontmatter_error = ?4 WHERE id = ?1",
            params![id, projection.json, projection.format, projection.error],
        )?;
    }
    Ok(())
}

fn migrate_category_appearance_columns(
    transaction: &Transaction<'_>,
) -> rusqlite_migration::HookResult {
    let mut statement = transaction.prepare("PRAGMA table_info(categories)")?;
    let columns = statement
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<Result<Vec<_>, _>>()?;
    if !columns.iter().any(|column| column == "icon") {
        transaction.execute_batch(
            "ALTER TABLE categories ADD COLUMN icon TEXT NOT NULL DEFAULT 'folder';",
        )?;
    }
    if !columns.iter().any(|column| column == "color") {
        transaction.execute_batch(
            "ALTER TABLE categories ADD COLUMN color TEXT NOT NULL DEFAULT 'auto';",
        )?;
    }
    Ok(())
}

fn migrate_note_favorite_columns(transaction: &Transaction<'_>) -> rusqlite_migration::HookResult {
    let mut statement = transaction.prepare("PRAGMA table_info(notes)")?;
    let columns = statement
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<Result<Vec<_>, _>>()?;
    if !columns.iter().any(|column| column == "is_favorite") {
        transaction.execute_batch(
            "ALTER TABLE notes ADD COLUMN is_favorite INTEGER NOT NULL DEFAULT 0 CHECK(is_favorite IN (0, 1));",
        )?;
    }
    if !columns.iter().any(|column| column == "favorited_at") {
        transaction.execute_batch("ALTER TABLE notes ADD COLUMN favorited_at INTEGER;")?;
    }
    transaction.execute_batch(
        "CREATE INDEX IF NOT EXISTS notes_favorite_order_idx ON notes(is_favorite, favorited_at DESC);",
    )?;
    Ok(())
}
