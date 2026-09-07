use super::*;

#[test]
fn trash_and_restore_should_reject_stale_or_repeated_requests() {
    let (_directory, library) = library();
    let now = OffsetDateTime::now_utc();
    let category = library
        .create_category("Work", now)
        .unwrap_or_else(|error| panic!("category failed: {error}"));
    let note = library
        .create_note(category.id, now)
        .unwrap_or_else(|error| panic!("note failed: {error}"));

    library
        .trash_note(note.id, now)
        .unwrap_or_else(|error| panic!("note trash failed: {error}"));
    assert!(matches!(
        library.trash_note(note.id, now),
        Err(StorageError::MutationUnavailable)
    ));
    library
        .restore_note(note.id)
        .unwrap_or_else(|error| panic!("note restore failed: {error}"));
    assert!(matches!(
        library.restore_note(note.id),
        Err(StorageError::MutationUnavailable)
    ));
    library
        .trash_category(category.id, now)
        .unwrap_or_else(|error| panic!("category trash failed: {error}"));
    assert!(matches!(
        library.trash_category(category.id, now),
        Err(StorageError::MutationUnavailable)
    ));
    library
        .restore_category(category.id, now)
        .unwrap_or_else(|error| panic!("category restore failed: {error}"));
    assert!(matches!(
        library.restore_category(category.id, now),
        Err(StorageError::MutationUnavailable)
    ));
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

#[test]
fn trash_contents_groups_category_notes_and_lists_directly_trashed_notes() {
    let (_directory, library) = library();
    let now = OffsetDateTime::now_utc();
    let active_category = library
        .create_category("Active", now)
        .unwrap_or_else(|error| panic!("category failed: {error}"));
    let direct_note = library
        .create_note(active_category.id, now)
        .unwrap_or_else(|error| panic!("note failed: {error}"));
    library
        .trash_note(direct_note.id, now)
        .unwrap_or_else(|error| panic!("direct trash failed: {error}"));
    let deleted_category = library
        .create_category("Deleted", now)
        .unwrap_or_else(|error| panic!("category failed: {error}"));
    let grouped_note = library
        .create_note(deleted_category.id, now)
        .unwrap_or_else(|error| panic!("note failed: {error}"));
    library
        .trash_category(deleted_category.id, now)
        .unwrap_or_else(|error| panic!("category trash failed: {error}"));

    let contents = library
        .trash_contents()
        .unwrap_or_else(|error| panic!("trash listing failed: {error}"));

    assert_eq!(contents.categories.len(), 1);
    assert_eq!(contents.categories[0].category.id, deleted_category.id);
    assert_eq!(contents.categories[0].recoverable_note_count, 1);
    assert_eq!(contents.notes.len(), 1);
    assert_eq!(contents.notes[0].id, direct_note.id);
    assert_ne!(contents.notes[0].id, grouped_note.id);
}

#[test]
fn empty_trash_removes_search_entries_and_orphaned_assets() {
    let (directory, library) = library();
    let now = OffsetDateTime::now_utc();
    let category = library
        .create_category("Work", now)
        .unwrap_or_else(|error| panic!("category failed: {error}"));
    let note = library
        .create_note(category.id, now)
        .unwrap_or_else(|error| panic!("note failed: {error}"));
    let saved = library
        .save_note(note.id, note.revision, "# Remove me", now)
        .unwrap_or_else(|error| panic!("save failed: {error}"));
    library
        .store_asset(saved.id, "png", b"test image")
        .unwrap_or_else(|error| panic!("asset failed: {error}"));
    library
        .trash_note(saved.id, now)
        .unwrap_or_else(|error| panic!("trash failed: {error}"));

    let result = library
        .empty_trash()
        .unwrap_or_else(|error| panic!("empty failed: {error}"));

    assert_eq!(result.notes_deleted, 1);
    assert_eq!(result.assets_deleted, 1);
    assert!(
        library
            .note(saved.id)
            .unwrap_or_else(|error| panic!("lookup failed: {error}"))
            .is_none()
    );
    assert!(
        library
            .search_notes("Remove", None, 10)
            .unwrap_or_else(|error| panic!("search failed: {error}"))
            .is_empty()
    );
    assert!(
        fs::read_dir(directory.path().join("assets"))
            .unwrap_or_else(|error| panic!("asset directory failed: {error}"))
            .next()
            .is_none()
    );
}
