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
fn saving_unchanged_source_preserves_the_note_timestamp_and_revision() {
    let (_directory, library) = library();
    let created_at = OffsetDateTime::UNIX_EPOCH;
    let category = library
        .create_category("Work", created_at)
        .unwrap_or_else(|error| panic!("category failed: {error}"));
    let note = library
        .create_note(category.id, created_at)
        .unwrap_or_else(|error| panic!("note failed: {error}"));
    let edited_at = created_at + time::Duration::days(1);
    let saved = library
        .save_note(note.id, note.revision, "# Keep", edited_at)
        .unwrap_or_else(|error| panic!("save failed: {error}"));

    let unchanged = library
        .save_note(
            saved.id,
            saved.revision,
            &saved.source,
            edited_at + time::Duration::days(1),
        )
        .unwrap_or_else(|error| panic!("unchanged save failed: {error}"));

    assert_eq!(unchanged.updated_at, saved.updated_at);
    assert_eq!(unchanged.revision, saved.revision);
}

#[test]
fn moving_a_note_preserves_content_and_its_recent_position() {
    let (_directory, library) = library();
    let created_at = OffsetDateTime::now_utc() - time::Duration::days(1);
    let source_category = library
        .create_category("Source", created_at)
        .unwrap_or_else(|error| panic!("source category failed: {error}"));
    let destination_category = library
        .create_category("Destination", created_at)
        .unwrap_or_else(|error| panic!("destination category failed: {error}"));
    let note = library
        .create_note(source_category.id, created_at)
        .unwrap_or_else(|error| panic!("note failed: {error}"));
    let saved = library
        .save_note(note.id, note.revision, "# Keep this content", created_at)
        .unwrap_or_else(|error| panic!("save failed: {error}"));
    let asset = library
        .store_asset(saved.id, "png", b"asset")
        .unwrap_or_else(|error| panic!("asset failed: {error}"));

    let moved = library
        .move_note(saved.id, destination_category.id, OffsetDateTime::now_utc())
        .unwrap_or_else(|error| panic!("move failed: {error}"));

    assert_eq!(moved.category_id, destination_category.id);
    assert_eq!(moved.source, saved.source);
    assert_eq!(moved.updated_at, saved.updated_at);
    assert_eq!(moved.revision.0, saved.revision.0 + 1);
    assert!(
        library
            .note_asset_bytes(moved.id, &asset)
            .unwrap_or_else(|error| panic!("asset lookup failed: {error}"))
            .is_some()
    );
    assert_eq!(
        library.note_count(source_category.id).unwrap_or(usize::MAX),
        0
    );
    assert_eq!(
        library
            .note_count(destination_category.id)
            .unwrap_or(usize::MAX),
        1
    );
}

#[test]
fn moving_to_a_trashed_category_is_rejected() {
    let (_directory, library) = library();
    let now = OffsetDateTime::now_utc();
    let source_category = library
        .create_category("Source", now)
        .unwrap_or_else(|error| panic!("source category failed: {error}"));
    let destination_category = library
        .create_category("Destination", now)
        .unwrap_or_else(|error| panic!("destination category failed: {error}"));
    let note = library
        .create_note(source_category.id, now)
        .unwrap_or_else(|error| panic!("note failed: {error}"));
    library
        .trash_category(destination_category.id, now)
        .unwrap_or_else(|error| panic!("destination trash failed: {error}"));

    let result = library.move_note(note.id, destination_category.id, now);

    assert!(matches!(result, Err(StorageError::MoveUnavailable)));
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

#[test]
fn create_category_rejects_blank_names() {
    let (_directory, library) = library();
    let result = library.create_category(" \t ", OffsetDateTime::now_utc());
    assert!(matches!(result, Err(StorageError::InvalidCategoryName)));
}

#[test]
fn store_asset_rejects_unsupported_extensions() {
    let (_directory, library) = library();
    let now = OffsetDateTime::now_utc();
    let category = library
        .create_category("Work", now)
        .unwrap_or_else(|error| panic!("category failed: {error}"));
    let note = library
        .create_note(category.id, now)
        .unwrap_or_else(|error| panic!("note failed: {error}"));
    let result = library.store_asset(note.id, "../sqlite", b"not an image");
    assert!(matches!(
        result,
        Err(StorageError::UnsupportedAssetExtension(_))
    ));
}
