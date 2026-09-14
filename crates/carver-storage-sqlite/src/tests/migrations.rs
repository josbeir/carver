use super::*;
use carver_library_port::PageRequest;

#[test]
fn migrations_should_be_valid() {
    migrations()
        .validate()
        .unwrap_or_else(|error| panic!("migration definition is invalid: {error}"));
}

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

    assert_eq!(schema_version(&database_path), 4);
    let connection = rusqlite::Connection::open(&database_path)
        .unwrap_or_else(|error| panic!("database open failed: {error}"));
    assert_eq!(
        migrations()
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
    assert_eq!(schema_version(&database_path), 4);
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
