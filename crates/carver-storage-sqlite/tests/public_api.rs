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
    assert_eq!(library.list_category_summaries()?[0].note_count, 0);
    Ok(())
}

#[test]
fn asset_storage_should_ignore_a_preexisting_legacy_temporary_path()
-> Result<(), Box<dyn std::error::Error>> {
    use sha2::{Digest, Sha256};
    let directory = tempfile::tempdir()?;
    let assets = directory.path().join("assets");
    let library = SqliteLibrary::open(&directory.path().join("library.sqlite3"), &assets)?;
    let now = OffsetDateTime::now_utc();
    let category = library.create_category("Work", now)?;
    let note = library.create_note(category.id, now)?;
    let bytes = b"asset bytes";
    let legacy = assets.join(format!("{:x}.partial", Sha256::digest(bytes)));
    std::fs::create_dir(&legacy)?;

    let path = library.store_asset(note.id, "png", bytes)?;

    assert_eq!(
        library.note_asset_bytes(note.id, &path)?,
        Some(bytes.to_vec())
    );
    assert!(legacy.is_dir());
    Ok(())
}

#[test]
fn concurrent_asset_stores_should_share_complete_bytes_across_library_connections()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let database = directory.path().join("library.sqlite3");
    let assets = directory.path().join("assets");
    let library = SqliteLibrary::open(&database, &assets)?;
    let now = OffsetDateTime::now_utc();
    let category = library.create_category("Work", now)?;
    let mut writers = Vec::new();
    for _ in 0..8 {
        let note = library.create_note(category.id, now)?;
        writers.push((SqliteLibrary::open(&database, &assets)?, note.id));
    }
    let barrier = std::sync::Barrier::new(writers.len());
    let bytes = vec![42; 64 * 1024];
    std::thread::scope(|scope| -> Result<(), Box<dyn std::error::Error>> {
        let handles: Vec<_> = writers
            .into_iter()
            .map(|(library, note_id)| {
                let barrier = &barrier;
                let bytes = &bytes;
                scope.spawn(move || {
                    barrier.wait();
                    let path = library.store_asset(note_id, "png", bytes)?;
                    assert_eq!(
                        library.note_asset_bytes(note_id, &path)?,
                        Some(bytes.clone())
                    );
                    Ok::<_, carver_storage_sqlite::StorageError>(())
                })
            })
            .collect();
        for handle in handles {
            handle.join().map_err(|_| "asset writer panicked")??;
        }
        Ok(())
    })?;
    assert_eq!(std::fs::read_dir(&assets)?.count(), 1);
    Ok(())
}
