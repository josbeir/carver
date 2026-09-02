use super::*;

/// Exercises source-editing commands with GTK initialized by the shared UI test.
pub(crate) fn graphical_commands_cover_editor_buffer_operations() {
    let buffer = gtk::TextBuffer::new(None);

    toggle_inline(&buffer, "*", "*");
    assert_eq!(buffer_text(&buffer), "**");

    buffer.set_text("selected");
    buffer.select_range(&buffer.start_iter(), &buffer.end_iter());
    toggle_inline(&buffer, "*", "*");
    assert_eq!(buffer_text(&buffer), "*selected*");
    buffer.select_range(&buffer.start_iter(), &buffer.end_iter());
    toggle_inline(&buffer, "*", "*");
    assert_eq!(buffer_text(&buffer), "selected");

    buffer.set_text("# one\nsecond");
    buffer.select_range(&buffer.start_iter(), &buffer.end_iter());
    set_heading(&buffer, 2);
    assert_eq!(buffer_text(&buffer), "## one\n## second");
    buffer.select_range(&buffer.start_iter(), &buffer.end_iter());
    set_heading(&buffer, 0);
    assert_eq!(buffer_text(&buffer), "one\nsecond");

    buffer.set_text("first\nsecond");
    buffer.select_range(&buffer.start_iter(), &buffer.end_iter());
    toggle_list(&buffer, "- ");
    assert_eq!(buffer_text(&buffer), "- first\n- second");
    buffer.select_range(&buffer.start_iter(), &buffer.end_iter());
    toggle_list(&buffer, "- ");
    assert_eq!(buffer_text(&buffer), "first\nsecond");

    buffer.select_range(&buffer.start_iter(), &buffer.end_iter());
    toggle_code_block(&buffer);
    assert_eq!(buffer_text(&buffer), "```\nfirst\nsecond\n```");
    buffer.select_range(&buffer.start_iter(), &buffer.end_iter());
    toggle_code_block(&buffer);
    assert_eq!(buffer_text(&buffer), "first\nsecond");

    buffer.set_text("Carver");
    buffer.select_range(&buffer.start_iter(), &buffer.end_iter());
    insert_link(&buffer, "Carver", "https://carver.invalid");
    assert_eq!(buffer_text(&buffer), "[Carver](https://carver.invalid)");
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
