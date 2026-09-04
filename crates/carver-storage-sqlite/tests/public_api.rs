//! External-consumer contract tests for `carver-storage-sqlite`.

use carver_storage_sqlite::SqliteLibrary;
use time::OffsetDateTime;

#[test]
fn sqlite_library_should_persist_categories_through_its_public_api()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let library = SqliteLibrary::open(
        &directory.path().join("library.sqlite3"),
        &directory.path().join("assets"),
    )?;

    let category = library.create_category("Projects", OffsetDateTime::now_utc())?;
    let categories = library.list_categories()?;

    assert_eq!(categories.len(), 1);
    assert_eq!(categories[0].id, category.id);
    assert_eq!(categories[0].name, "Projects");
    assert_eq!(categories[0].position, 0);
    Ok(())
}
