use super::*;

#[test]
fn category_creation_should_increment_the_semantic_library_revision() {
    let (_directory, library) = library();
    let now = OffsetDateTime::UNIX_EPOCH;

    let initial_revision = library
        .change_revision()
        .unwrap_or_else(|error| panic!("initial revision failed: {error}"));
    let _category = library
        .create_category("Work", now)
        .unwrap_or_else(|error| panic!("category failed: {error}"));
    let changed_revision = library
        .change_revision()
        .unwrap_or_else(|error| panic!("changed revision failed: {error}"));

    assert_eq!(changed_revision, LibraryRevision(initial_revision.0 + 1));
}

#[test]
fn category_appearance_should_round_trip_through_storage() {
    let (_directory, library) = library();
    let created_at = OffsetDateTime::UNIX_EPOCH;
    let appearance = CategoryAppearance {
        icon: CategoryIcon::Calendar,
        color: CategoryColor::Rose,
    };
    let category = library
        .create_category_with_appearance("Personal", appearance, created_at)
        .unwrap_or_else(|error| panic!("category failed: {error}"));
    let updated_at = created_at + time::Duration::days(1);
    let updated_appearance = CategoryAppearance {
        icon: CategoryIcon::People,
        color: CategoryColor::Teal,
    };

    let updated = library
        .update_category(category.id, "Projects", updated_appearance, updated_at)
        .unwrap_or_else(|error| panic!("category update failed: {error}"));

    assert_eq!(updated.name, "Projects");
    assert_eq!(updated.appearance, updated_appearance);
    assert_eq!(updated.updated_at, updated_at);
}

#[test]
fn category_summaries_count_only_active_notes_in_active_categories() {
    let (_directory, library) = library();
    let now = OffsetDateTime::now_utc();
    let work = library
        .create_category("Work", now)
        .unwrap_or_else(|error| panic!("work category failed: {error}"));
    let archived = library
        .create_category("Archived", now)
        .unwrap_or_else(|error| panic!("archived category failed: {error}"));
    let active_note = library
        .create_note(work.id, now)
        .unwrap_or_else(|error| panic!("active note failed: {error}"));
    let trashed_note = library
        .create_note(work.id, now)
        .unwrap_or_else(|error| panic!("trashed note failed: {error}"));
    let _archived_note = library
        .create_note(archived.id, now)
        .unwrap_or_else(|error| panic!("archived note failed: {error}"));
    library
        .trash_note(trashed_note.id, now)
        .unwrap_or_else(|error| panic!("note trash failed: {error}"));
    library
        .trash_category(archived.id, now)
        .unwrap_or_else(|error| panic!("category trash failed: {error}"));

    let summaries = library
        .list_category_summaries()
        .unwrap_or_else(|error| panic!("category summaries failed: {error}"));

    assert_eq!(summaries.len(), 1);
    assert_eq!(summaries[0].category.id, work.id);
    assert_eq!(summaries[0].note_count, 1);
    assert_eq!(summaries[0].category.id, active_note.category_id);
}
