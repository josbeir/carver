use super::*;

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

#[test]
fn store_asset_accepts_non_image_file_extensions() -> Result<(), StorageError> {
    let (_directory, library) = library();
    let now = OffsetDateTime::now_utc();
    let category = library.create_category("Work", now)?;
    let note = library.create_note(category.id, now)?;

    let path = library.store_asset(note.id, "pdf", b"document")?;

    assert!(
        std::path::Path::new(&path)
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("pdf"))
    );
    Ok(())
}

#[test]
fn store_asset_reuses_the_canonical_filename_for_identical_bytes() -> Result<(), StorageError> {
    let (_directory, library) = library();
    let now = OffsetDateTime::now_utc();
    let category = library.create_category("Work", now)?;
    let first = library.create_note(category.id, now)?;
    let second = library.create_note(category.id, now)?;

    let first_path = library.store_asset(first.id, "pdf", b"same bytes")?;
    let second_path = library.store_asset(second.id, "txt", b"same bytes")?;

    assert_eq!(first_path, second_path);
    assert_eq!(
        library.note_asset_bytes(second.id, &second_path)?,
        Some(b"same bytes".to_vec())
    );
    Ok(())
}

#[test]
fn existing_asset_should_not_bypass_extension_validation() -> Result<(), StorageError> {
    let (_directory, library) = library();
    let now = OffsetDateTime::now_utc();
    let category = library.create_category("Work", now)?;
    let first = library.create_note(category.id, now)?;
    let second = library.create_note(category.id, now)?;
    let path = library.store_asset(first.id, "png", b"same bytes")?;
    assert!(matches!(
        library.store_asset(second.id, "../sqlite", b"same bytes"),
        Err(StorageError::UnsupportedAssetExtension(_))
    ));
    assert_eq!(library.note_asset_size(second.id, &path)?, None);
    Ok(())
}

#[test]
fn asset_size_should_respect_note_ownership_without_reading_bytes() -> Result<(), StorageError> {
    let (_directory, library) = library();
    let now = OffsetDateTime::now_utc();
    let category = library.create_category("Work", now)?;
    let first = library.create_note(category.id, now)?;
    let second = library.create_note(category.id, now)?;
    let path = library.store_asset(first.id, "pdf", b"document")?;
    assert_eq!(library.note_asset_size(first.id, &path)?, Some(8));
    assert_eq!(library.note_asset_size(second.id, &path)?, None);
    assert_eq!(
        library.note_asset_size(first.id, "../database.sqlite")?,
        None
    );
    Ok(())
}
