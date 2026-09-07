use super::*;

#[test]
fn updating_note_timestamps_should_preserve_source_and_increment_the_revision() {
    let (_directory, library) = library();
    let initial_time = OffsetDateTime::UNIX_EPOCH;
    let category = library
        .create_category("Journal", initial_time)
        .unwrap_or_else(|error| panic!("category failed: {error}"));
    let note = library
        .create_note_with_source(category.id, "# Entry", initial_time)
        .unwrap_or_else(|error| panic!("note failed: {error}"));
    let created_at = initial_time - time::Duration::days(2);
    let updated_at = initial_time + time::Duration::days(3);

    let updated = library
        .update_note_timestamps(note.id, note.revision, created_at, updated_at)
        .unwrap_or_else(|error| panic!("timestamp update failed: {error}"));

    assert_eq!(
        (
            updated.source,
            updated.revision,
            updated.created_at,
            updated.updated_at
        ),
        (
            "# Entry".to_owned(),
            Revision(note.revision.0 + 1),
            created_at,
            updated_at
        )
    );
}

#[test]
fn favoriting_a_note_should_preserve_content_timestamp_and_increment_revision() {
    let (_directory, library) = library();
    let created_at = OffsetDateTime::UNIX_EPOCH;
    let category = library
        .create_category("Favorites", created_at)
        .unwrap_or_else(|error| panic!("category failed: {error}"));
    let note = library
        .create_note_with_source(category.id, "# Keep", created_at)
        .unwrap_or_else(|error| panic!("note failed: {error}"));
    let favorite = library
        .set_note_favorite(
            note.id,
            note.revision,
            true,
            created_at + time::Duration::days(1),
        )
        .unwrap_or_else(|error| panic!("favorite update failed: {error}"));

    assert_eq!(
        (favorite.is_favorite, favorite.revision, favorite.updated_at),
        (true, Revision(note.revision.0 + 1), note.updated_at)
    );
}

#[test]
fn favorite_notes_should_return_active_notes_by_most_recent_favorite() {
    let (_directory, library) = library();
    let now = OffsetDateTime::UNIX_EPOCH;
    let category = library
        .create_category("Favorites", now)
        .unwrap_or_else(|error| panic!("category failed: {error}"));
    let first = library
        .create_note_with_source(category.id, "# First", now)
        .unwrap_or_else(|error| panic!("first note failed: {error}"));
    let second = library
        .create_note_with_source(category.id, "# Second", now)
        .unwrap_or_else(|error| panic!("second note failed: {error}"));
    let _first = library
        .set_note_favorite(first.id, first.revision, true, now)
        .unwrap_or_else(|error| panic!("first favorite failed: {error}"));
    let _second = library
        .set_note_favorite(
            second.id,
            second.revision,
            true,
            now + time::Duration::seconds(1),
        )
        .unwrap_or_else(|error| panic!("second favorite failed: {error}"));
    let other_category = library
        .create_category("Other Favorites", now)
        .unwrap_or_else(|error| panic!("other category failed: {error}"));
    let other = library
        .create_note_with_source(other_category.id, "# Other", now)
        .unwrap_or_else(|error| panic!("other note failed: {error}"));
    let _other = library
        .set_note_favorite(
            other.id,
            other.revision,
            true,
            now + time::Duration::seconds(2),
        )
        .unwrap_or_else(|error| panic!("other favorite failed: {error}"));

    let favorites = library
        .favorite_notes(None, 20, 0)
        .unwrap_or_else(|error| panic!("favorite list failed: {error}"));

    assert_eq!(
        favorites.iter().map(|note| note.id).collect::<Vec<_>>(),
        vec![other.id, second.id, first.id]
    );
    let category_favorites = library
        .favorite_notes(Some(category.id), 20, 0)
        .unwrap_or_else(|error| panic!("category favorite list failed: {error}"));
    assert_eq!(
        category_favorites
            .iter()
            .map(|note| note.id)
            .collect::<Vec<_>>(),
        vec![second.id, first.id]
    );
}

#[test]
fn favoriting_with_a_stale_revision_should_fail() {
    let (_directory, library) = library();
    let now = OffsetDateTime::UNIX_EPOCH;
    let category = library
        .create_category("Favorites", now)
        .unwrap_or_else(|error| panic!("category failed: {error}"));
    let note = library
        .create_note(category.id, now)
        .unwrap_or_else(|error| panic!("note failed: {error}"));
    let _updated = library
        .set_note_favorite(note.id, note.revision, true, now)
        .unwrap_or_else(|error| panic!("first favorite failed: {error}"));

    let result = library.set_note_favorite(note.id, note.revision, false, now);

    assert!(matches!(result, Err(StorageError::Conflict)));
}

#[test]
fn updating_note_timestamps_should_reject_modification_before_creation() {
    let (_directory, library) = library();
    let timestamp = OffsetDateTime::UNIX_EPOCH;
    let category = library
        .create_category("Journal", timestamp)
        .unwrap_or_else(|error| panic!("category failed: {error}"));
    let note = library
        .create_note(category.id, timestamp)
        .unwrap_or_else(|error| panic!("note failed: {error}"));

    let result = library.update_note_timestamps(
        note.id,
        note.revision,
        timestamp,
        timestamp - time::Duration::seconds(1),
    );

    assert!(matches!(result, Err(StorageError::InvalidNoteTimestamps)));
}

#[test]
fn creating_a_note_in_a_trashed_category_should_return_category_unavailable() {
    let (_directory, library) = library();
    let now = OffsetDateTime::UNIX_EPOCH;
    let category = library
        .create_category("Archived", now)
        .unwrap_or_else(|error| panic!("category failed: {error}"));
    library
        .trash_category(category.id, now)
        .unwrap_or_else(|error| panic!("category trash failed: {error}"));

    let result = library.create_note(category.id, now);

    assert!(matches!(result, Err(StorageError::CategoryUnavailable)));
}

#[test]
fn importing_a_note_in_a_trashed_category_should_return_category_unavailable() {
    let (_directory, library) = library();
    let now = OffsetDateTime::UNIX_EPOCH;
    let category = library
        .create_category("Archived", now)
        .unwrap_or_else(|error| panic!("category failed: {error}"));
    library
        .trash_category(category.id, now)
        .unwrap_or_else(|error| panic!("category trash failed: {error}"));

    let result = library.create_note_with_source(category.id, "# Hidden", now);

    assert!(matches!(result, Err(StorageError::CategoryUnavailable)));
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
