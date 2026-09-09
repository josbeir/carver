#[test]
fn html_title_should_encode_closing_tags_and_literal_entities() {
    let document = super::html_document("<p>Body</p>", "</title><script>alert(1)</script> &lt; &");
    assert!(document.contains(
        "<title>&lt;/title&gt;&lt;script&gt;alert(1)&lt;/script&gt; &amp;lt; &amp;</title>"
    ));
    assert!(!document.contains("<script>"));
}
