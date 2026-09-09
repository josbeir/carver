//! Excerpts preserve grapheme boundaries without changing stored note content.

use carver_domain::Note;
use carver_storage_sqlite::SqliteLibrary;
use time::OffsetDateTime;

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn library_with_note(
    source: &str,
) -> Result<(tempfile::TempDir, SqliteLibrary, Note), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let library = SqliteLibrary::open(
        &directory.path().join("library.sqlite3"),
        &directory.path().join("assets"),
    )?;
    let now = OffsetDateTime::UNIX_EPOCH;
    let category = library.create_category("Unicode", now)?;
    let note = library.create_note_with_source(category.id, source, now)?;
    Ok((directory, library, note))
}

#[test]
fn recent_excerpt_should_keep_the_combining_accent_at_the_limit() -> TestResult {
    let expected = format!("{}e\u{301}", "a".repeat(179));
    let source = format!("{expected}remaining");
    let (_directory, library, note) = library_with_note(&source)?;

    let summary = library.recent_notes(None, 1, 0)?.pop().ok_or("summary")?;

    assert_eq!(summary.excerpt, expected);
    let persisted = library.note(note.id)?.ok_or("persisted note")?;
    assert_eq!(persisted.source, source);
    assert_eq!(persisted.revision, note.revision);
    assert_eq!(persisted.updated_at, note.updated_at);
    Ok(())
}

#[test]
fn trash_excerpt_should_keep_the_whole_emoji_at_the_limit() -> TestResult {
    let expected = format!("{}👩🏽‍💻", "a".repeat(179));
    let source = format!("{expected}remaining");
    let (_directory, library, note) = library_with_note(&source)?;
    library.trash_note(note.id, OffsetDateTime::UNIX_EPOCH)?;
    let before = library.note(note.id)?.ok_or("trashed note")?;

    let summary = library
        .trash_contents()?
        .notes
        .pop()
        .ok_or("trash summary")?;

    assert_eq!(summary.excerpt, expected);
    let persisted = library.note(note.id)?.ok_or("persisted note")?;
    assert_eq!(persisted.source, source);
    assert_eq!(persisted.revision, before.revision);
    assert_eq!(persisted.updated_at, before.updated_at);
    Ok(())
}
