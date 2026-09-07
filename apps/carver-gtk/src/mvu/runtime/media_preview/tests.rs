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
fn preview_copies_should_reuse_one_copy_per_managed_asset() -> Result<(), Box<dyn std::error::Error>>
{
    let copies = std::cell::RefCell::new(std::collections::BTreeMap::new());
    let (directory, path) = prepare_copy(b"first", "assets/hash.pdf", "Brief")?;
    let retained = retain_preview_copy(&copies, "assets/hash.pdf".into(), directory, path.clone());

    assert_eq!(retained, path);
    assert_eq!(cached_preview_path(&copies, "assets/hash.pdf"), Some(path));
    assert_eq!(copies.borrow().len(), 1);
    Ok(())
}

#[test]
fn preview_filename_should_preserve_an_existing_extension() {
    assert_eq!(
        preview_filename("assets/hash.pdf", "Project brief.pdf"),
        "Project brief.pdf"
    );
    assert_eq!(preview_filename("assets/hash.png", "..."), "Attachment.png");
}

#[test]
fn preview_filename_should_fit_a_filesystem_component_with_multibyte_text() {
    let filename = preview_filename("assets/hash.pdf", &"測".repeat(100));
    assert!(filename.len() <= 255);
    assert!(
        std::path::Path::new(&filename)
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("pdf"))
    );
    assert!(std::str::from_utf8(filename.as_bytes()).is_ok());
}
