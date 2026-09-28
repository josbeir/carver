use super::*;
use carver_domain::note_link_destination;

fn linked_source(target: NoteId) -> String {
    format!(
        "# Source\n\nSee [Target]({}).\n",
        note_link_destination(target)
    )
}

#[test]
fn creating_notes_should_index_links_in_both_directions() {
    let (_directory, library) = library();
    let now = OffsetDateTime::UNIX_EPOCH;
    let category = library
        .create_category("Links", now)
        .unwrap_or_else(|error| panic!("category failed: {error}"));
    let target = library
        .create_note_with_source(category.id, "# Target", now)
        .unwrap_or_else(|error| panic!("target failed: {error}"));
    let source = library
        .create_note_with_source(category.id, &linked_source(target.id), now)
        .unwrap_or_else(|error| panic!("source failed: {error}"));

    let source_links = library
        .note_links(source.id)
        .unwrap_or_else(|error| panic!("source links failed: {error}"));
    assert_eq!(
        source_links
            .outgoing
            .iter()
            .map(|note| note.id)
            .collect::<Vec<_>>(),
        vec![target.id]
    );
    assert!(source_links.backlinks.is_empty());

    let target_links = library
        .note_links(target.id)
        .unwrap_or_else(|error| panic!("target links failed: {error}"));
    assert!(target_links.outgoing.is_empty());
    assert_eq!(
        target_links
            .backlinks
            .iter()
            .map(|note| note.id)
            .collect::<Vec<_>>(),
        vec![source.id]
    );
}

#[test]
fn saving_a_note_should_replace_its_indexed_links() {
    let (_directory, library) = library();
    let now = OffsetDateTime::UNIX_EPOCH;
    let category = library
        .create_category("Links", now)
        .unwrap_or_else(|error| panic!("category failed: {error}"));
    let first = library
        .create_note_with_source(category.id, "# First", now)
        .unwrap_or_else(|error| panic!("first failed: {error}"));
    let second = library
        .create_note_with_source(category.id, "# Second", now)
        .unwrap_or_else(|error| panic!("second failed: {error}"));
    let source = library
        .create_note_with_source(category.id, &linked_source(first.id), now)
        .unwrap_or_else(|error| panic!("source failed: {error}"));

    let saved = library
        .save_note(source.id, source.revision, &linked_source(second.id), now)
        .unwrap_or_else(|error| panic!("save failed: {error}"));

    let links = library
        .note_links(saved.id)
        .unwrap_or_else(|error| panic!("links failed: {error}"));
    assert_eq!(
        links
            .outgoing
            .iter()
            .map(|note| note.id)
            .collect::<Vec<_>>(),
        vec![second.id]
    );
    let first_links = library
        .note_links(first.id)
        .unwrap_or_else(|error| panic!("first links failed: {error}"));
    assert!(first_links.backlinks.is_empty());
}

#[test]
fn removing_all_links_on_save_should_clear_the_index() {
    let (_directory, library) = library();
    let now = OffsetDateTime::UNIX_EPOCH;
    let category = library
        .create_category("Links", now)
        .unwrap_or_else(|error| panic!("category failed: {error}"));
    let target = library
        .create_note_with_source(category.id, "# Target", now)
        .unwrap_or_else(|error| panic!("target failed: {error}"));
    let source = library
        .create_note_with_source(category.id, &linked_source(target.id), now)
        .unwrap_or_else(|error| panic!("source failed: {error}"));

    let saved = library
        .save_note(source.id, source.revision, "# Source\n\nNo links.\n", now)
        .unwrap_or_else(|error| panic!("save failed: {error}"));
    let links = library
        .note_links(saved.id)
        .unwrap_or_else(|error| panic!("links failed: {error}"));
    assert!(links.outgoing.is_empty());
}

#[test]
fn trashing_a_target_should_hide_it_until_restored() {
    let (_directory, library) = library();
    let now = OffsetDateTime::UNIX_EPOCH;
    let category = library
        .create_category("Links", now)
        .unwrap_or_else(|error| panic!("category failed: {error}"));
    let target = library
        .create_note_with_source(category.id, "# Target", now)
        .unwrap_or_else(|error| panic!("target failed: {error}"));
    let source = library
        .create_note_with_source(category.id, &linked_source(target.id), now)
        .unwrap_or_else(|error| panic!("source failed: {error}"));

    library
        .trash_note(target.id, now)
        .unwrap_or_else(|error| panic!("trash failed: {error}"));
    let links = library
        .note_links(source.id)
        .unwrap_or_else(|error| panic!("links failed: {error}"));
    assert!(links.outgoing.is_empty());

    library
        .restore_note(target.id)
        .unwrap_or_else(|error| panic!("restore failed: {error}"));
    let links = library
        .note_links(source.id)
        .unwrap_or_else(|error| panic!("links failed: {error}"));
    assert_eq!(
        links
            .outgoing
            .iter()
            .map(|note| note.id)
            .collect::<Vec<_>>(),
        vec![target.id]
    );
}

#[test]
fn hard_deleting_a_note_should_cascade_its_links() {
    let (_directory, library) = library();
    let now = OffsetDateTime::UNIX_EPOCH;
    let category = library
        .create_category("Links", now)
        .unwrap_or_else(|error| panic!("category failed: {error}"));
    let target = library
        .create_note_with_source(category.id, "# Target", now)
        .unwrap_or_else(|error| panic!("target failed: {error}"));
    let source = library
        .create_note_with_source(category.id, &linked_source(target.id), now)
        .unwrap_or_else(|error| panic!("source failed: {error}"));

    library
        .trash_note(source.id, now)
        .unwrap_or_else(|error| panic!("trash failed: {error}"));
    library
        .empty_trash()
        .unwrap_or_else(|error| panic!("empty trash failed: {error}"));

    let links = library
        .note_links(target.id)
        .unwrap_or_else(|error| panic!("links failed: {error}"));
    assert!(links.backlinks.is_empty());
}

#[test]
fn reference_links_should_be_indexed() {
    let (_directory, library) = library();
    let now = OffsetDateTime::UNIX_EPOCH;
    let category = library
        .create_category("Links", now)
        .unwrap_or_else(|error| panic!("category failed: {error}"));
    let target = library
        .create_note_with_source(category.id, "# Target", now)
        .unwrap_or_else(|error| panic!("target failed: {error}"));
    let source = format!(
        "# Source\n\nSee [Target][ref].\n\n[ref]: {}\n",
        note_link_destination(target.id)
    );
    let source = library
        .create_note_with_source(category.id, &source, now)
        .unwrap_or_else(|error| panic!("source failed: {error}"));

    let links = library
        .note_links(source.id)
        .unwrap_or_else(|error| panic!("links failed: {error}"));
    assert_eq!(
        links
            .outgoing
            .iter()
            .map(|note| note.id)
            .collect::<Vec<_>>(),
        vec![target.id]
    );
}
