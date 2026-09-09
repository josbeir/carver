use super::*;

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

#[test]
fn inline_command_should_wrap_and_unwrap_a_selection() {
    assert_eq!(inline_replacement("word", "*", "*"), "*word*");
    assert_eq!(inline_replacement("*word*", "*", "*"), "word");
}

#[test]
fn list_marker_should_be_added_and_removed() {
    assert_eq!(list_replacement("one", "- ", false), "- one");
    assert_eq!(list_replacement("one", "- ", true), "one");
}

#[test]
fn heading_command_should_replace_an_existing_level() {
    assert_eq!(heading_replacement("# Title", "## "), "## Title");
    assert_eq!(heading_replacement("## Title", ""), "Title");
}

#[test]
fn list_command_should_replace_existing_markers() {
    assert_eq!(strip_list_marker("- one"), "one");
    assert_eq!(list_replacement("one", ". ", false), ". one");
}

#[test]
fn ordered_list_command_should_use_automatic_bare_dot_markers() {
    let mut edit = SourceEdit::new("Level 1\nLevel 2\nLevel 3\nLevel 4".to_owned(), 0..31);

    edit.toggle_ordered_list();

    assert_eq!(edit.source(), ". Level 1\n. Level 2\n. Level 3\n. Level 4");
}

#[test]
fn ordered_list_command_should_remove_any_existing_ordered_markers() {
    let mut edit = SourceEdit::new("4. Level 1\n8. Level 2".to_owned(), 0..21);

    edit.toggle_ordered_list();

    assert_eq!(edit.source(), "Level 1\nLevel 2");
}

#[test]
fn ordered_list_command_should_remove_bare_dot_markers() {
    let mut edit = SourceEdit::new(". Level 1\n. Level 2".to_owned(), 0..19);

    edit.toggle_ordered_list();

    assert_eq!(edit.source(), "Level 1\nLevel 2");
}

#[test]
fn image_width_should_replace_only_the_width_attribute() {
    assert_eq!(
        image_with_width(
            "![Diagram](assets/diagram.png){width=\"25%\" title=\"Project overview\"}",
            Some(50),
        ),
        "![Diagram](assets/diagram.png){title=\"Project overview\" width=\"50%\"}"
    );
    assert_eq!(
        image_with_width("![Diagram](assets/diagram.png){width=\"25%\"}", None),
        "![Diagram](assets/diagram.png)"
    );
}

#[test]
fn image_span_should_accept_a_cursor_in_presentation_attributes() {
    let source = "Before ![Diagram](assets/diagram.png){width=\"50%\"} after";
    let cursor = source.find("50%").unwrap_or_default();
    let span = image_span_at(source, cursor).unwrap_or_default();
    assert_eq!(
        &source[span.0..span.1],
        "![Diagram](assets/diagram.png){width=\"50%\"}"
    );
}

#[test]
fn pure_inline_edit_should_preserve_character_based_unicode_selection() {
    let mut edit = SourceEdit::new("Café".to_owned(), 3..4);

    edit.toggle_inline("*", "*");

    assert_eq!(edit.source(), "Caf*é*");
    assert_eq!(edit.selection(), 3..6);
}

#[test]
fn pure_line_edit_should_transform_only_selected_lines() {
    let mut edit = SourceEdit::new("# One\nTwo\nThree".to_owned(), 0..9);

    edit.set_heading(2);

    assert_eq!(edit.source(), "## One\n## Two\nThree");
    assert_eq!(edit.selection(), 0..13);
}

#[test]
fn hard_break_command_should_insert_a_backslash_before_the_newline() {
    let mut edit = SourceEdit::new("First line".to_owned(), 5..5);

    edit.insert_hard_break();

    assert_eq!(edit.source(), "First\\\n line");
}

#[test]
fn pure_list_and_code_edits_should_round_trip_canonical_source() {
    let mut edit = SourceEdit::new("One\nTwo".to_owned(), 0..7);
    edit.toggle_list("- ");
    assert_eq!(edit.source(), "- One\n- Two");

    edit.toggle_code_block();
    assert_eq!(edit.source(), "```\n- One\n- Two\n```");
    edit.toggle_code_block();
    assert_eq!(edit.source(), "- One\n- Two");
}

#[test]
fn pure_insert_and_image_width_edits_should_update_selection() {
    let mut link = SourceEdit::new("Read ".to_owned(), 5..5);
    link.insert_link("more", "https://example.com");
    assert_eq!(link.source(), "Read [more](https://example.com)");
    assert_eq!(link.selection(), 32..32);

    let mut image = SourceEdit::new(
        "![Diagram](assets/diagram.png){width=\"25%\"}".to_owned(),
        30..30,
    );
    assert!(image.set_image_width(Some(50)));
    assert_eq!(
        image.source(),
        "![Diagram](assets/diagram.png){width=\"50%\"}"
    );
}
