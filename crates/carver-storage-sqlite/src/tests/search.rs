use super::*;
use carver_library_port::PageRequest;

fn page(limit: usize) -> PageRequest {
    PageRequest { limit, offset: 0 }
}

#[test]
fn recent_notes_should_page_in_stable_order() {
    let (_directory, library) = library();
    let now = OffsetDateTime::UNIX_EPOCH;
    let category = library
        .create_category("Work", now)
        .unwrap_or_else(|error| panic!("category failed: {error}"));
    for title in ["First", "Second", "Third"] {
        library
            .create_note_with_source(category.id, &format!("# {title}"), now)
            .unwrap_or_else(|error| panic!("note failed: {error}"));
    }
    let first = library
        .recent_notes(
            None,
            PageRequest {
                limit: 2,
                offset: 0,
            },
        )
        .unwrap_or_else(|error| panic!("first page failed: {error}"));
    let second = library
        .recent_notes(
            None,
            PageRequest {
                limit: 2,
                offset: 2,
            },
        )
        .unwrap_or_else(|error| panic!("second page failed: {error}"));

    assert_eq!(first.items.len(), 2);
    assert!(first.has_more);
    assert_eq!(second.items.len(), 1);
    assert!(!second.has_more);
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
        .search_notes("Carve", None, page(20))
        .unwrap_or_else(|error| panic!("search failed: {error}"))
        .items;
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
            .search_notes("Ship", None, page(20))
            .unwrap_or_else(|error| panic!("search failed: {error}"))
            .items
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
        .recent_notes(None, page(20))
        .unwrap_or_else(|error| panic!("list failed: {error}"))
        .items;
    assert_eq!(summaries[0].category_name, "Personal");
}

#[test]
fn fts_search_should_match_category_names() {
    let (_directory, library) = library();
    let now = OffsetDateTime::UNIX_EPOCH;
    let category = library
        .create_category("Astronomy", now)
        .unwrap_or_else(|error| panic!("category failed: {error}"));
    let note = library
        .create_note_with_source(category.id, "# Field notes\n\nUnrelated body", now)
        .unwrap_or_else(|error| panic!("note failed: {error}"));

    let results = library
        .search_notes("Astronomy", None, page(20))
        .unwrap_or_else(|error| panic!("search failed: {error}"))
        .items;

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].note.id, note.id);
}

#[test]
fn renaming_a_category_should_reindex_its_notes_for_search() {
    let (_directory, library) = library();
    let now = OffsetDateTime::UNIX_EPOCH;
    let category = library
        .create_category("Astronomy", now)
        .unwrap_or_else(|error| panic!("category failed: {error}"));
    let _note = library
        .create_note_with_source(category.id, "# Field notes\n\nUnrelated body", now)
        .unwrap_or_else(|error| panic!("note failed: {error}"));

    library
        .rename_category(category.id, "Geology", now)
        .unwrap_or_else(|error| panic!("rename failed: {error}"));

    let stale = library
        .search_notes("Astronomy", None, page(20))
        .unwrap_or_else(|error| panic!("stale search failed: {error}"))
        .items;
    let renamed = library
        .search_notes("Geology", None, page(20))
        .unwrap_or_else(|error| panic!("renamed search failed: {error}"))
        .items;

    assert_eq!(stale, [] as [carver_domain::SearchHit; 0]);
    assert_eq!(renamed.len(), 1);
}

#[test]
fn updating_a_category_name_should_reindex_its_notes_for_search() {
    let (_directory, library) = library();
    let now = OffsetDateTime::UNIX_EPOCH;
    let category = library
        .create_category("Astronomy", now)
        .unwrap_or_else(|error| panic!("category failed: {error}"));
    let _note = library
        .create_note_with_source(category.id, "# Field notes\n\nUnrelated body", now)
        .unwrap_or_else(|error| panic!("note failed: {error}"));

    library
        .update_category(category.id, "Geology", CategoryAppearance::default(), now)
        .unwrap_or_else(|error| panic!("update failed: {error}"));

    let stale = library
        .search_notes("Astronomy", None, page(20))
        .unwrap_or_else(|error| panic!("stale search failed: {error}"))
        .items;
    let renamed = library
        .search_notes("Geology", None, page(20))
        .unwrap_or_else(|error| panic!("renamed search failed: {error}"))
        .items;

    assert_eq!(stale, [] as [carver_domain::SearchHit; 0]);
    assert_eq!(renamed.len(), 1);
}

#[test]
fn updating_category_appearance_should_keep_its_notes_searchable() {
    let (_directory, library) = library();
    let now = OffsetDateTime::UNIX_EPOCH;
    let category = library
        .create_category("Astronomy", now)
        .unwrap_or_else(|error| panic!("category failed: {error}"));
    let note = library
        .create_note_with_source(category.id, "# Field notes\n\nUnrelated body", now)
        .unwrap_or_else(|error| panic!("note failed: {error}"));

    library
        .update_category(
            category.id,
            "Astronomy",
            CategoryAppearance {
                icon: CategoryIcon::Star,
                color: CategoryColor::Blue,
            },
            now,
        )
        .unwrap_or_else(|error| panic!("update failed: {error}"));

    let results = library
        .search_notes("Astronomy", None, page(20))
        .unwrap_or_else(|error| panic!("search failed: {error}"))
        .items;

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].note.id, note.id);
}

#[test]
fn moving_a_note_should_reindex_its_category_for_search() {
    let (_directory, library) = library();
    let now = OffsetDateTime::UNIX_EPOCH;
    let source = library
        .create_category("Astronomy", now)
        .unwrap_or_else(|error| panic!("source category failed: {error}"));
    let destination = library
        .create_category("Geology", now)
        .unwrap_or_else(|error| panic!("destination category failed: {error}"));
    let note = library
        .create_note_with_source(source.id, "# Field notes\n\nUnrelated body", now)
        .unwrap_or_else(|error| panic!("note failed: {error}"));

    library
        .move_note(note.id, destination.id, now)
        .unwrap_or_else(|error| panic!("move failed: {error}"));

    let stale = library
        .search_notes("Astronomy", None, page(20))
        .unwrap_or_else(|error| panic!("stale search failed: {error}"))
        .items;
    let moved = library
        .search_notes("Geology", None, page(20))
        .unwrap_or_else(|error| panic!("moved search failed: {error}"))
        .items;

    assert_eq!(stale, [] as [carver_domain::SearchHit; 0]);
    assert_eq!(moved.len(), 1);
    assert_eq!(moved[0].note.id, note.id);
}

#[test]
fn fts_search_should_rank_content_matches_above_category_matches() {
    let (_directory, library) = library();
    let now = OffsetDateTime::UNIX_EPOCH;
    let category = library
        .create_category("Roadmap", now)
        .unwrap_or_else(|error| panic!("category failed: {error}"));
    let content_match = library
        .create_note_with_source(category.id, "# Alpha\n\nThe roadmap in detail", now)
        .unwrap_or_else(|error| panic!("note failed: {error}"));
    library
        .create_note_with_source(category.id, "# Beta\n\nUnrelated body", now)
        .unwrap_or_else(|error| panic!("note failed: {error}"));

    let results = library
        .search_notes("roadmap", None, page(20))
        .unwrap_or_else(|error| panic!("search failed: {error}"))
        .items;

    assert_eq!(results.len(), 2);
    assert_eq!(results[0].note.id, content_match.id);
}
