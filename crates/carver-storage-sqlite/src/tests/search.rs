use super::*;

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
    assert_eq!(results[0].note.category_name, "Work");
}

#[test]
fn creating_a_note_with_source_indexes_its_derived_content() {
    let (_directory, library) = library();
    let now = OffsetDateTime::UNIX_EPOCH;
    let category = library
        .create_category("Work", now)
        .unwrap_or_else(|error| panic!("category failed: {error}"));

    let note = library
        .create_note_with_source(category.id, "# Imported roadmap\n\nShip it", now)
        .unwrap_or_else(|error| panic!("import failed: {error}"));

    assert_eq!(note.revision, Revision(1));
    assert_eq!(note.title, "Imported roadmap");
    assert_eq!(
        library
            .search_notes("Ship", None, 20)
            .unwrap_or_else(|error| panic!("search failed: {error}"))
            .len(),
        1
    );
}

#[test]
fn recent_note_summaries_include_their_category_name() {
    let (_directory, library) = library();
    let now = OffsetDateTime::now_utc();
    let category = library
        .create_category("Personal", now)
        .unwrap_or_else(|error| panic!("category failed: {error}"));
    let _note = library
        .create_note(category.id, now)
        .unwrap_or_else(|error| panic!("note failed: {error}"));
    let summaries = library
        .recent_notes(None, 20, 0)
        .unwrap_or_else(|error| panic!("list failed: {error}"));
    assert_eq!(summaries[0].category_name, "Personal");
}
