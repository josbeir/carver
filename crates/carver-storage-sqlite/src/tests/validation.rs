use super::*;

#[test]
fn create_category_rejects_blank_names() {
    let (_directory, library) = library();
    let result = library.create_category(" \t ", OffsetDateTime::now_utc());
    assert!(matches!(result, Err(StorageError::InvalidCategoryName)));
}
