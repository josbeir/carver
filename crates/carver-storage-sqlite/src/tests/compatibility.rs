use super::*;

#[test]
fn opening_a_legacy_library_should_assign_the_default_category_appearance() {
    let directory =
        tempfile::tempdir().unwrap_or_else(|error| panic!("temporary directory failed: {error}"));
    let database_path = directory.path().join("library.sqlite3");
    let category_id = CategoryId::new();
    let connection = rusqlite::Connection::open(&database_path)
        .unwrap_or_else(|error| panic!("legacy database open failed: {error}"));
    connection
        .execute_batch(
            "CREATE TABLE categories (
                id TEXT PRIMARY KEY NOT NULL, name TEXT NOT NULL CHECK(length(trim(name)) > 0),
                position INTEGER NOT NULL, created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL, trashed_at INTEGER
            );",
        )
        .unwrap_or_else(|error| panic!("legacy schema creation failed: {error}"));
    connection
        .execute(
            "INSERT INTO categories (id, name, position, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![category_id.to_string(), "Legacy", 0_i64, 0_i64, 0_i64],
        )
        .unwrap_or_else(|error| panic!("legacy category creation failed: {error}"));
    drop(connection);

    let library = SqliteLibrary::open(&database_path, &directory.path().join("assets"))
        .unwrap_or_else(|error| panic!("legacy library migration failed: {error}"));
    let categories = library
        .list_categories()
        .unwrap_or_else(|error| panic!("categories failed: {error}"));

    assert_eq!(categories[0].appearance, CategoryAppearance::default());
    drop(library);
    assert_eq!(schema_version(&database_path), 2);
}
