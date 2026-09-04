use super::*;
use carver_domain::source_analysis::SourceAnalysis;

fn buffer_with(text: &str) -> gtk::TextBuffer {
    let buffer = gtk::TextBuffer::new(None);
    buffer.set_text(text);
    buffer
}

fn text(buffer: &gtk::TextBuffer) -> String {
    buffer
        .text(&buffer.start_iter(), &buffer.end_iter(), false)
        .to_string()
}

#[test]
fn inline_command_wraps_and_unwraps_a_selection() {
    assert_eq!(inline_replacement("word", "*", "*"), "*word*");
    assert_eq!(inline_replacement("*word*", "*", "*"), "word");
}

#[test]
fn line_command_changes_every_selected_line() {
    assert_eq!(list_replacement("one", "- ", false), "- one");
    assert_eq!(list_replacement("one", "- ", true), "one");
}

#[test]
fn heading_command_replaces_an_existing_level() {
    assert_eq!(heading_replacement("# Title", "## "), "## Title");
    assert_eq!(heading_replacement("## Title", ""), "Title");
}

#[test]
fn list_command_switches_all_selected_lines_without_nested_prefixes() {
    assert_eq!(strip_list_marker("- one"), "one");
    assert_eq!(list_replacement("one", "1. ", false), "1. one");
}

#[test]
fn ordered_list_command_should_number_selected_lines_consecutively() {
    let mut edit = SourceEdit::new("Level 1\nLevel 2\nLevel 3\nLevel 4", 0..31);

    edit.toggle_ordered_list();

    assert_eq!(
        edit.source(),
        "1. Level 1\n2. Level 2\n3. Level 3\n4. Level 4"
    );
}

#[test]
fn ordered_list_command_should_remove_any_existing_ordered_markers() {
    let mut edit = SourceEdit::new("4. Level 1\n8. Level 2", 0..21);

    edit.toggle_ordered_list();

    assert_eq!(edit.source(), "Level 1\nLevel 2");
}

#[test]
fn image_width_replaces_only_the_width_attribute() {
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
fn image_span_accepts_a_cursor_in_presentation_attributes() {
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
    let mut edit = SourceEdit::new("Café", 3..4);

    edit.toggle_inline("*", "*");

    assert_eq!(edit.source(), "Caf*é*");
    assert_eq!(edit.selection(), 3..6);
}

#[test]
fn pure_line_edit_should_transform_only_selected_lines() {
    let mut edit = SourceEdit::new("# One\nTwo\nThree", 0..9);

    edit.set_heading(2);

    assert_eq!(edit.source(), "## One\n## Two\nThree");
    assert_eq!(edit.selection(), 0..13);
}

#[test]
fn hard_break_command_should_insert_a_backslash_before_the_newline() {
    let mut edit = SourceEdit::new("First line", 5..5);

    edit.insert_hard_break();

    assert_eq!(edit.source(), "First\\\n line");
}

#[test]
fn pure_list_and_code_edits_should_round_trip_canonical_source() {
    let mut edit = SourceEdit::new("One\nTwo", 0..7);
    edit.toggle_list("- ");
    assert_eq!(edit.source(), "- One\n- Two");

    edit.toggle_code_block();
    assert_eq!(edit.source(), "```\n- One\n- Two\n```");
    edit.toggle_code_block();
    assert_eq!(edit.source(), "- One\n- Two");
}

#[test]
fn pure_insert_and_image_width_edits_should_update_selection() {
    let mut link = SourceEdit::new("Read ", 5..5);
    link.insert_link("more", "https://example.com");
    assert_eq!(link.source(), "Read [more](https://example.com)");
    assert_eq!(link.selection(), 32..32);

    let mut image = SourceEdit::new("![Diagram](assets/diagram.png){width=\"25%\"}", 30..30);
    assert!(image.set_image_width(Some(50)));
    assert_eq!(
        image.source(),
        "![Diagram](assets/diagram.png){width=\"50%\"}"
    );
}

#[test]
fn toolbar_state_should_mark_only_unambiguous_source_formatting_as_active() {
    let active = source_toolbar_state("*bold*", 0..6);
    assert!(active.is_active(ToolbarCommand::Bold));

    let mixed = source_toolbar_state("*bold* plain", 0..12);
    assert!(!mixed.is_active(ToolbarCommand::Bold));
}

#[test]
fn toolbar_state_should_detect_block_and_image_context() {
    let heading = source_toolbar_state("## Heading", 0..10);
    assert_eq!(heading.heading(), 2);

    let image = source_toolbar_state("![Diagram](assets/diagram.png){width=\"50%\"}", 30..30);
    assert_eq!(image.image_width(), Some(50));
}

fn source_toolbar_state(source: &str, selection: Range<usize>) -> ToolbarState {
    let analysis = SourceAnalysis::parse(source);
    toolbar_state_from_context(analysis.context_for(selection))
}

fn inline_command_inserts_an_empty_pair_at_the_cursor() {
    let buffer = buffer_with("word");
    let end = buffer.end_iter();
    buffer.place_cursor(&end);

    toggle_inline(&buffer, "*", "*");

    assert_eq!(text(&buffer), "word**");
    assert_eq!(buffer.iter_at_mark(&buffer.get_insert()).offset(), 5);
}

fn inline_command_replaces_selected_text_and_keeps_it_selected() {
    let buffer = buffer_with("word");
    buffer.select_range(&buffer.start_iter(), &buffer.end_iter());

    toggle_inline(&buffer, "*", "*");

    assert_eq!(text(&buffer), "*word*");
    assert_eq!(
        buffer
            .selection_bounds()
            .map(|(start, end)| buffer.text(&start, &end, false).to_string()),
        Some("*word*".to_owned())
    );
}

fn heading_command_updates_every_selected_line() {
    let buffer = buffer_with("# One\n## Two\nThree");
    buffer.select_range(&buffer.start_iter(), &buffer.end_iter());

    set_heading(&buffer, 3);

    assert_eq!(text(&buffer), "### One\n### Two\n### Three");
}

fn heading_level_zero_removes_existing_markers() {
    let buffer = buffer_with("### One\nTwo");
    buffer.select_range(&buffer.start_iter(), &buffer.end_iter());

    set_heading(&buffer, 0);

    assert_eq!(text(&buffer), "One\nTwo");
}

fn list_command_adds_and_removes_supported_markers() {
    let buffer = buffer_with("- [ ] One\n2. Two\n* Three");
    buffer.select_range(&buffer.start_iter(), &buffer.end_iter());

    toggle_list(&buffer, "- ");

    assert_eq!(text(&buffer), "- One\n- Two\n- Three");
    buffer.select_range(&buffer.start_iter(), &buffer.end_iter());
    toggle_list(&buffer, "- ");
    assert_eq!(text(&buffer), "One\nTwo\nThree");
}

fn code_block_command_wraps_then_unwraps_the_selection() {
    let buffer = buffer_with("let value = 1;");
    buffer.select_range(&buffer.start_iter(), &buffer.end_iter());

    toggle_code_block(&buffer);
    assert_eq!(text(&buffer), "```\nlet value = 1;\n```");

    buffer.select_range(&buffer.start_iter(), &buffer.end_iter());
    toggle_code_block(&buffer);
    assert_eq!(text(&buffer), "let value = 1;");
}

fn code_block_command_without_selection_uses_inline_code() {
    let buffer = buffer_with("");

    toggle_code_block(&buffer);

    assert_eq!(text(&buffer), "``");
}

fn link_command_replaces_selection_or_inserts_at_cursor() {
    let selected = buffer_with("Carver");
    selected.select_range(&selected.start_iter(), &selected.end_iter());
    insert_link(&selected, "Carver", "https://example.com");
    assert_eq!(text(&selected), "[Carver](https://example.com)");

    let inserted = buffer_with("Read ");
    let end = inserted.end_iter();
    inserted.place_cursor(&end);
    insert_link(&inserted, "more", "https://example.com/more");
    assert_eq!(text(&inserted), "Read [more](https://example.com/more)");
}

fn image_width_command_updates_the_direct_image_at_the_cursor() {
    let buffer = buffer_with("![Diagram](assets/diagram.png){width=\"25%\"}");
    let cursor = buffer.iter_at_offset(30);
    buffer.place_cursor(&cursor);
    assert!(set_image_width(&buffer, Some(50)));
    assert_eq!(
        text(&buffer),
        "![Diagram](assets/diagram.png){width=\"50%\"}"
    );
}

pub(crate) fn gtk_source_commands_cover_selection_and_block_operations() {
    inline_command_inserts_an_empty_pair_at_the_cursor();
    inline_command_replaces_selected_text_and_keeps_it_selected();
    heading_command_updates_every_selected_line();
    heading_level_zero_removes_existing_markers();
    list_command_adds_and_removes_supported_markers();
    code_block_command_wraps_then_unwraps_the_selection();
    code_block_command_without_selection_uses_inline_code();
    link_command_replaces_selection_or_inserts_at_cursor();
    image_width_command_updates_the_direct_image_at_the_cursor();
}
