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
