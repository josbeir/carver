//! Storage-facing regression tests for frontend fixtures.

use super::support::{TestResult, test_state};

#[test]
fn library_fixture_should_create_open_and_save_notes() -> TestResult {
    let (_temporary_directory, client) = test_state()?;
    let category = client.create_category("Notes")?;
    let created = client.create_note(category.id)?;
    let saved = client.save_note(created.id, created.revision, "# Regression note\nNo borrow")?;
    assert_eq!(saved.revision.0, 2);
    let reopened = client.note(created.id)?.ok_or("saved note")?;
    assert_eq!(reopened.source, "# Regression note\nNo borrow");
    assert_eq!(reopened.updated_at, saved.updated_at);
    Ok(())
}

#[test]
fn library_fixture_should_rename_categories_and_restore_notes() -> TestResult {
    let (_temporary_directory, client) = test_state()?;
    let category = client.create_category("Notes")?;
    let renamed = client.rename_category(category.id, "Work")?;
    assert_eq!(renamed.name, "Work");
    let created = client.create_note(category.id)?;
    client.trash_note(created.id)?;
    assert!(client.recent_notes(None, 10, 0)?.is_empty());
    client.restore_note(created.id)?;
    assert_eq!(client.recent_notes(None, 10, 0)?.len(), 1);
    Ok(())
}

#[test]
fn library_fixture_should_store_managed_images() -> TestResult {
    let (temporary_directory, client) = test_state()?;
    let category = client.create_category("Notes")?;
    let note = client.create_note(category.id)?;
    let image_path = client.store_asset(note.id, "png", b"test-png-bytes")?;
    assert!(image_path.starts_with("assets/"));
    assert!(
        temporary_directory
            .path()
            .join("data")
            .join(image_path)
            .is_file()
    );
    Ok(())
}
