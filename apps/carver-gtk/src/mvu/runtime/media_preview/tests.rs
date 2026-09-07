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
fn preview_filename_should_preserve_an_existing_extension() {
    assert_eq!(
        preview_filename("assets/hash.pdf", "Project brief.pdf"),
        "Project brief.pdf"
    );
    assert_eq!(preview_filename("assets/hash.png", "..."), "Attachment.png");
}
