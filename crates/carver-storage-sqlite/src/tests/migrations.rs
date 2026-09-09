use super::*;

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

    assert_eq!(schema_version(&database_path), 3);
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
    assert_eq!(schema_version(&database_path), 3);
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
