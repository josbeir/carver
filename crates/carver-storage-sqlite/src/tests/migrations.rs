use super::*;
use crate::migrations as schema_migrations;
use carver_library_port::PageRequest;

#[test]
fn reopening_a_versioned_library_should_not_have_pending_migrations() {
    let directory =
        tempfile::tempdir().unwrap_or_else(|error| panic!("temporary directory failed: {error}"));
    let database_path = directory.path().join("library.sqlite3");
    let assets_dir = directory.path().join("assets");
    let library = SqliteLibrary::open(&database_path, &assets_dir)
        .unwrap_or_else(|error| panic!("library open failed: {error}"));
    let initial_revision = library
        .change_revision()
        .unwrap_or_else(|error| panic!("initial revision failed: {error}"));
    drop(library);

    assert_eq!(schema_version(&database_path), 5);
    let connection = rusqlite::Connection::open(&database_path)
        .unwrap_or_else(|error| panic!("database open failed: {error}"));
    assert_eq!(
        schema_migrations::definitions()
            .pending_migrations(&connection)
            .unwrap_or_else(|error| panic!("pending migration check failed: {error}")),
        0
    );
    drop(connection);

    let reopened = SqliteLibrary::open(&database_path, &assets_dir)
        .unwrap_or_else(|error| panic!("library reopen failed: {error}"));
    assert_eq!(
        reopened
            .change_revision()
            .unwrap_or_else(|error| panic!("reopened revision failed: {error}")),
        initial_revision
    );
}

#[test]
fn opening_an_unversioned_current_library_should_adopt_the_schema() {
    let directory =
        tempfile::tempdir().unwrap_or_else(|error| panic!("temporary directory failed: {error}"));
    let database_path = directory.path().join("library.sqlite3");
    let assets_dir = directory.path().join("assets");
    let library = SqliteLibrary::open(&database_path, &assets_dir)
        .unwrap_or_else(|error| panic!("library open failed: {error}"));
    drop(library);

    let connection = rusqlite::Connection::open(&database_path)
        .unwrap_or_else(|error| panic!("database open failed: {error}"));
    connection
        .pragma_update(None, "user_version", 0_i32)
        .unwrap_or_else(|error| panic!("schema version reset failed: {error}"));
    drop(connection);

    let adopted = SqliteLibrary::open(&database_path, &assets_dir)
        .unwrap_or_else(|error| panic!("unversioned library adoption failed: {error}"));
    assert_eq!(
        adopted
            .change_revision()
            .unwrap_or_else(|error| panic!("adopted revision failed: {error}")),
        LibraryRevision(0)
    );
    drop(adopted);
    assert_eq!(schema_version(&database_path), 5);
}

#[test]
fn derived_title_migration_should_reindex_existing_frontmatter_titles() {
    let directory =
        tempfile::tempdir().unwrap_or_else(|error| panic!("temporary directory failed: {error}"));
    let database_path = directory.path().join("library.sqlite3");
    let assets_dir = directory.path().join("assets");
    let library = SqliteLibrary::open(&database_path, &assets_dir)
        .unwrap_or_else(|error| panic!("library open failed: {error}"));
    let category = library
        .create_category("Projects", OffsetDateTime::UNIX_EPOCH)
        .unwrap_or_else(|error| panic!("category creation failed: {error}"));
    let note = library
        .create_note_with_source(
            category.id,
            "---\ntitle: Frontmatter title\n---\n\n# Heading title\n\nBody text",
            OffsetDateTime::UNIX_EPOCH,
        )
        .unwrap_or_else(|error| panic!("note creation failed: {error}"));
    drop(library);

    let connection = rusqlite::Connection::open(&database_path)
        .unwrap_or_else(|error| panic!("database open failed: {error}"));
    connection
        .execute(
            "UPDATE notes SET title = ?2 WHERE id = ?1",
            rusqlite::params![note.id.to_string(), "Heading title"],
        )
        .unwrap_or_else(|error| panic!("legacy title setup failed: {error}"));
    connection
        .execute(
            "DELETE FROM note_fts WHERE note_id = ?1",
            [note.id.to_string()],
        )
        .unwrap_or_else(|error| panic!("legacy search setup cleanup failed: {error}"));
    connection
        .execute(
            "INSERT INTO note_fts (note_id, title, plain_text) VALUES (?1, ?2, ?3)",
            rusqlite::params![note.id.to_string(), "Heading title", "Body text"],
        )
        .unwrap_or_else(|error| panic!("legacy search setup failed: {error}"));
    connection
        .pragma_update(None, "user_version", 3_i32)
        .unwrap_or_else(|error| panic!("schema version reset failed: {error}"));
    drop(connection);

    let migrated = SqliteLibrary::open(&database_path, &assets_dir)
        .unwrap_or_else(|error| panic!("library migration failed: {error}"));

    assert_eq!(
        migrated
            .note(note.id)
            .unwrap_or_else(|error| panic!("note query failed: {error}"))
            .map(|note| (note.title, note.revision, note.updated_at)),
        Some((
            "Frontmatter title".to_owned(),
            note.revision,
            note.updated_at,
        ))
    );
    assert_eq!(
        migrated
            .search_notes(
                "Frontmatter",
                None,
                PageRequest {
                    limit: 1,
                    offset: 0,
                },
            )
            .unwrap_or_else(|error| panic!("search query failed: {error}"))
            .items
            .first()
            .map(|hit| hit.note.id),
        Some(note.id)
    );
}

#[test]
fn asset_ownership_migration_should_reshape_legacy_asset_tables() {
    let directory =
        tempfile::tempdir().unwrap_or_else(|error| panic!("temporary directory failed: {error}"));
    let database_path = directory.path().join("library.sqlite3");
    let assets_dir = directory.path().join("assets");
    let library = SqliteLibrary::open(&database_path, &assets_dir)
        .unwrap_or_else(|error| panic!("library open failed: {error}"));
    let now = OffsetDateTime::now_utc();
    let category = library
        .create_category("Work", now)
        .unwrap_or_else(|error| panic!("category creation failed: {error}"));
    let note = library
        .create_note(category.id, now)
        .unwrap_or_else(|error| panic!("note creation failed: {error}"));
    drop(library);

    // Recreate the pre-version-five, globally shared asset tables.
    let connection = rusqlite::Connection::open(&database_path)
        .unwrap_or_else(|error| panic!("database open failed: {error}"));
    connection
        .execute_batch(
            "DROP TABLE IF EXISTS assets;
             DROP TABLE IF EXISTS note_assets;
             CREATE TABLE assets (
                 hash TEXT PRIMARY KEY NOT NULL, filename TEXT NOT NULL UNIQUE,
                 byte_size INTEGER NOT NULL
             );
             CREATE TABLE note_assets (
                 note_id TEXT NOT NULL REFERENCES notes(id) ON DELETE CASCADE,
                 asset_hash TEXT NOT NULL REFERENCES assets(hash) ON DELETE CASCADE,
                 PRIMARY KEY(note_id, asset_hash)
             );",
        )
        .unwrap_or_else(|error| panic!("legacy asset schema creation failed: {error}"));
    connection
        .execute(
            "INSERT INTO assets (hash, filename, byte_size) VALUES ('legacyhash', 'legacy.png', 4)",
            [],
        )
        .unwrap_or_else(|error| panic!("legacy asset insert failed: {error}"));
    connection
        .execute(
            "INSERT INTO note_assets (note_id, asset_hash) VALUES (?1, 'legacyhash')",
            [note.id.to_string()],
        )
        .unwrap_or_else(|error| panic!("legacy ownership insert failed: {error}"));
    connection
        .pragma_update(None, "user_version", 4_i32)
        .unwrap_or_else(|error| panic!("schema version reset failed: {error}"));
    drop(connection);

    let migrated = SqliteLibrary::open(&database_path, &assets_dir)
        .unwrap_or_else(|error| panic!("library migration failed: {error}"));

    assert_eq!(schema_version(&database_path), 5);
    assert_eq!(
        migrated
            .note_asset_bytes(note.id, "assets/legacy.png")
            .unwrap_or_else(|error| panic!("legacy asset lookup failed: {error}")),
        None
    );
    let path = migrated
        .store_asset(note.id, "png", b"fresh")
        .unwrap_or_else(|error| panic!("asset store failed: {error}"));
    // The note id stays out of the document-visible path.
    assert!(path.starts_with("assets/"));
    assert!(!path.contains(&note.id.to_string()));
    assert_eq!(
        migrated
            .note_asset_bytes(note.id, &path)
            .unwrap_or_else(|error| panic!("fresh asset lookup failed: {error}")),
        Some(b"fresh".to_vec())
    );
}

#[test]
fn change_notification_files_should_include_sqlite_wal_sidecars() {
    assert_eq!(
        change_notification_files(std::path::Path::new("/library/library.sqlite3")),
        Some([
            std::path::PathBuf::from("/library/library.sqlite3"),
            std::path::PathBuf::from("/library/library.sqlite3-wal"),
            std::path::PathBuf::from("/library/library.sqlite3-shm"),
        ])
    );
}
