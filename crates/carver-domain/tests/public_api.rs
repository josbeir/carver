//! External-consumer contract tests for `carver-domain`.

use carver_domain::derive_content;

#[test]
fn derive_content_should_expose_a_heading_as_the_public_title() {
    let content = derive_content("# Project plan\n\nPrepare the release.");

    assert_eq!(content.title, "Project plan");
}
