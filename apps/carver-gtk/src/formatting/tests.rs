use super::*;

pub(crate) fn restored_link_selection_replaces_instead_of_duplicating_text() {
    let buffer = gtk::TextBuffer::new(None);
    buffer.set_text("Carver link");
    let start = buffer.start_iter();
    let end = buffer.end_iter();
    buffer.select_range(&start, &end);
    let selection = capture_selection(&buffer);
    buffer.place_cursor(&buffer.end_iter());

    restore_selection(&buffer, selection);
    insert_link(&buffer, "Carver link", "https://carver.invalid");

    assert_eq!(buffer_text(&buffer), "Carver link");
    assert_eq!(
        link_destination(&buffer.start_iter()).as_deref(),
        Some("https://carver.invalid")
    );
}

pub(crate) fn rich_code_block_command_serializes_as_fenced_carve() {
    let buffer = gtk::TextBuffer::new(None);
    install_tags(&buffer);
    buffer.set_text("let answer = 42;");
    let start = buffer.start_iter();
    let end = buffer.end_iter();
    buffer.select_range(&start, &end);

    toggle_selected_code_blocks(&buffer);

    assert_eq!(
        crate::editor::buffer_text(&buffer),
        "```\nlet answer = 42;\n```"
    );
}

pub(crate) fn code_block_tag_uses_compact_type_and_line_spacing()
-> Result<(), Box<dyn std::error::Error>> {
    let buffer = gtk::TextBuffer::new(None);
    buffer.set_text("first\nsecond");
    let start = buffer.start_iter();
    let end = buffer.end_iter();
    apply_code_block_tag(&buffer, &start, &end, None);

    let tag = buffer
        .tag_table()
        .lookup("rich-code-block-")
        .ok_or("code block tag should be installed")?;

    assert_eq!(tag.pixels_above_lines(), 0);
    assert_eq!(tag.pixels_below_lines(), 0);
    assert_eq!((tag.left_margin(), tag.right_margin()), (0, 0));
    assert!((tag.scale() - 0.9).abs() < f64::EPSILON);
    Ok(())
}

fn buffer_text(buffer: &gtk::TextBuffer) -> String {
    buffer
        .text(&buffer.start_iter(), &buffer.end_iter(), false)
        .to_string()
}
