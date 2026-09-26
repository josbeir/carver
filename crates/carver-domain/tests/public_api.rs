//! External-consumer contract tests for `carver-domain`.

use carver_domain::derive_content;

#[test]
fn derive_content_should_expose_a_heading_as_the_public_title() {
    let content = derive_content("# Project plan\n\nPrepare the release.");

    assert_eq!(content.title, "Project plan");
}

#[test]
fn derive_content_should_prefer_a_frontmatter_title() {
    let content = derive_content("---\ntitle: Release plan\n---\n\n# Project plan");

    assert_eq!(content.title, "Release plan");
}

#[test]
fn derive_content_should_expose_the_heading_independently_of_the_title() {
    assert_eq!(
        derive_content("# Project plan\n\nPrepare the release.")
            .heading_title
            .as_deref(),
        Some("Project plan")
    );
    assert_eq!(derive_content("Body without a heading").heading_title, None);
    // The heading component stays available even when frontmatter owns the effective title.
    assert_eq!(
        derive_content("---\ntitle: Release plan\n---\n\n# Project plan")
            .heading_title
            .as_deref(),
        Some("Project plan")
    );
}
