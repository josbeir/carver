use super::*;

#[test]
fn safe_media_filename_should_preserve_an_existing_extension() {
    assert_eq!(
        safe_media_filename("assets/hash.pdf", "Project brief.pdf"),
        "Project brief.pdf"
    );
    assert_eq!(
        safe_media_filename("assets/hash.png", "..."),
        "Attachment.png"
    );
}

#[test]
fn safe_media_filename_should_fit_a_filesystem_component_with_multibyte_text() {
    let filename = safe_media_filename("assets/hash.pdf", &"測".repeat(100));
    assert!(filename.len() <= 255);
    assert!(
        std::path::Path::new(&filename)
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("pdf"))
    );
    assert!(std::str::from_utf8(filename.as_bytes()).is_ok());
}
