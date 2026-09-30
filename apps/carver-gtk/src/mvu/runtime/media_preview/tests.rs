use super::*;

#[test]
fn preview_copy_should_keep_original_bytes_and_use_a_safe_filename()
-> Result<(), Box<dyn std::error::Error>> {
    let (directory, path) = prepare_copy(b"document bytes", "assets/hash.pdf", "../../Brief")?;
    assert_eq!(path.parent(), Some(directory.path()));
    assert_eq!(
        path.extension().and_then(|value| value.to_str()),
        Some("pdf")
    );
    assert_eq!(std::fs::read(&path)?, b"document bytes");
    assert_eq!(
        std::fs::metadata(&path)?.permissions().mode() & 0o777,
        0o400
    );
    drop(directory);
    assert!(!path.exists());
    Ok(())
}

#[test]
fn preview_copies_should_reuse_one_copy_per_note_asset() -> Result<(), Box<dyn std::error::Error>> {
    let copies = std::cell::RefCell::new(std::collections::BTreeMap::new());
    let first_note = carver_sdk::NoteId::new();
    let second_note = carver_sdk::NoteId::new();
    let (directory, path) = prepare_copy(b"first", "assets/hash.pdf", "Brief")?;
    let retained = retain_preview_copy(
        &copies,
        first_note,
        "assets/hash.pdf".into(),
        directory,
        path.clone(),
    );

    assert_eq!(retained, path);
    assert_eq!(
        cached_preview_path(&copies, first_note, "assets/hash.pdf"),
        Some(path)
    );
    assert_eq!(
        cached_preview_path(&copies, second_note, "assets/hash.pdf"),
        None
    );
    assert_eq!(copies.borrow().len(), 1);
    Ok(())
}
