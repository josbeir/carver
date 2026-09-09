use super::*;
fn library() -> (tempfile::TempDir, SqliteLibrary) {
    let directory =
        tempfile::tempdir().unwrap_or_else(|error| panic!("temporary directory failed: {error}"));
    let library = SqliteLibrary::open(
        &directory.path().join("library.sqlite3"),
        &directory.path().join("assets"),
    )
    .unwrap_or_else(|error| panic!("library open failed: {error}"));
    (directory, library)
}

fn schema_version(database_path: &std::path::Path) -> i32 {
    let connection = rusqlite::Connection::open(database_path)
        .unwrap_or_else(|error| panic!("database open failed: {error}"));
    connection
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .unwrap_or_else(|error| panic!("schema version read failed: {error}"))
}

mod assets;
mod bases;
mod categories;
mod compatibility;
mod excerpts;
mod migrations;
mod notes;
mod search;
mod trash;
mod validation;
