use super::*;

#[test]
fn body_only_edits_should_not_invalidate_discovered_descriptors() {
    let (_directory, library) = library();
    let now = OffsetDateTime::UNIX_EPOCH;
    let category = library
        .create_category("Notes", now)
        .unwrap_or_else(|error| panic!("category failed: {error}"));
    let note = library
        .create_note_with_source(category.id, "---yaml\ntype: draft\n---\n# First", now)
        .unwrap_or_else(|error| panic!("note failed: {error}"));

    let discovered = library
        .property_descriptors()
        .unwrap_or_else(|error| panic!("descriptors failed: {error}"));
    let revision = library
        .frontmatter_revision()
        .unwrap_or_else(|error| panic!("revision failed: {error}"));
    assert!(
        discovered
            .iter()
            .any(|descriptor| descriptor.path.0 == "/type")
    );

    // A body-only edit keeps the projected frontmatter identical, so the cache must survive it.
    let saved = library
        .save_note(
            note.id,
            note.revision,
            "---yaml\ntype: draft\n---\n# First\n\nmore body",
            now + time::Duration::seconds(1),
        )
        .unwrap_or_else(|error| panic!("save failed: {error}"));
    assert_eq!(
        library
            .frontmatter_revision()
            .unwrap_or_else(|error| panic!("revision failed: {error}")),
        revision
    );
    assert_eq!(
        library
            .property_descriptors()
            .unwrap_or_else(|error| panic!("descriptors failed: {error}")),
        discovered
    );

    // A frontmatter change invalidates the cache and discovers the new path.
    library
        .save_note(
            saved.id,
            saved.revision,
            "---yaml\ntype: open\nscore: 3\n---\n# First\n\nmore body",
            now + time::Duration::seconds(2),
        )
        .unwrap_or_else(|error| panic!("save failed: {error}"));
    assert_ne!(
        library
            .frontmatter_revision()
            .unwrap_or_else(|error| panic!("revision failed: {error}")),
        revision
    );
    assert!(
        library
            .property_descriptors()
            .unwrap_or_else(|error| panic!("descriptors failed: {error}"))
            .iter()
            .any(|descriptor| descriptor.path.0 == "/score")
    );
}

#[test]
fn base_row_counts_should_refresh_after_note_and_trash_changes() {
    let (_directory, library) = library();
    let now = OffsetDateTime::UNIX_EPOCH;
    let category = library
        .create_category("Notes", now)
        .unwrap_or_else(|error| panic!("category failed: {error}"));
    library
        .create_base("All", &[BaseColumn::Name])
        .unwrap_or_else(|error| panic!("base failed: {error}"));

    let count = |library: &SqliteLibrary| {
        library
            .bases()
            .unwrap_or_else(|error| panic!("bases failed: {error}"))
            .first()
            .map_or(0, |base| base.row_count)
    };
    assert_eq!(count(&library), 0);

    let first = library
        .create_note_with_source(category.id, "# One", now)
        .unwrap_or_else(|error| panic!("note failed: {error}"));
    library
        .create_note_with_source(category.id, "# Two", now)
        .unwrap_or_else(|error| panic!("note failed: {error}"));
    assert_eq!(count(&library), 2);
    // A repeated read reuses the cached counts without going stale.
    assert_eq!(count(&library), 2);

    library
        .trash_note(first.id, now)
        .unwrap_or_else(|error| panic!("trash failed: {error}"));
    assert_eq!(count(&library), 1);
}
