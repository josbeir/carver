use super::*;

#[test]
fn image_rewriting_should_handle_quoting_case_and_encoded_paths() -> Result<(), RewritingError> {
    let html = "<IMG alt='a > b' SRC = 'assets&#47;photo.png'><img\nsrc=assets/second.png>";
    let mut sources = Vec::new();
    rewrite_managed_images(html, |_, source| {
        sources.push(source.to_owned());
        Ok(())
    })?;
    assert_eq!(sources, ["assets/photo.png", "assets/second.png"]);
    Ok(())
}

#[test]
fn image_rewriting_should_leave_non_images_comments_and_external_sources_unchanged()
-> Result<(), RewritingError> {
    let html = r#"<!-- <img src="assets/comment.png"> --><script>const x = '<img src="assets/script.png">';</script><p data-src="assets/text.png">src="assets/text.png"</p><img src="https://example.test/image.png"><img data-src="assets/lazy.png">"#;
    let output = rewrite_managed_images(html, |image, _| image.set_attribute("src", "changed"))?;
    assert_eq!(output, html);
    Ok(())
}
