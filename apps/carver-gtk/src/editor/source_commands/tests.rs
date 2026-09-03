use super::*;

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

fn image_commands_insert_standalone_markup_and_update_width() {
    let buffer = buffer_with("![Diagram](assets/diagram.png){width=\"25%\"}");
    let cursor = buffer.iter_at_offset(30);
    buffer.place_cursor(&cursor);
    assert!(set_image_width(&buffer, Some(50)));
    assert_eq!(
        text(&buffer),
        "![Diagram](assets/diagram.png){width=\"50%\"}"
    );

    let inserted = buffer_with("");
    insert_image(&inserted, "Diagram", "assets/diagram.png");
    assert_eq!(text(&inserted), "\n![Diagram](assets/diagram.png)\n");
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
    image_commands_insert_standalone_markup_and_update_width();
}
