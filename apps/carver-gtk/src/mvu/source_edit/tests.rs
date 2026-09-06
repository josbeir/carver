use super::{SourceCommand, SourceEdit};

#[test]
fn toggle_list_should_replace_every_supported_list_marker() {
    let source = "* item\n- [x] done";
    let edit = SourceEdit::apply(
        source.to_owned(),
        0..source.chars().count(),
        SourceCommand::ToggleList(String::from("- [ ] ")),
    );

    assert_eq!(edit.source(), "- [ ] item\n- [ ] done");
}

#[test]
fn image_width_should_use_quoted_canonical_attribute_and_preserve_other_attributes() {
    let source = "![First](assets/first.png){width=25% title=\"A first image\"}";
    let edit = SourceEdit::apply(
        source.to_owned(),
        2..2,
        SourceCommand::SetImageWidth(Some(50)),
    );

    assert_eq!(
        edit.source(),
        "![First](assets/first.png){title=\"A first image\" width=\"50%\"}"
    );
}

#[test]
fn heading_changes_should_preserve_literal_hashes_and_leading_whitespace() {
    let edit = SourceEdit::apply(
        String::from("#tag\n  ordinary\n## Heading"),
        0..26,
        SourceCommand::SetHeading(0),
    );

    assert_eq!(edit.source(), "#tag\n  ordinary\nHeading");
}
