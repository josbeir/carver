use super::*;
use crate::mvu::{SourceCommand, SourceEdit};
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

    apply_buffer_command(
        &buffer,
        SourceCommand::ToggleInline {
            opening: "*".into(),
            closing: "*".into(),
        },
    );

    assert_eq!(text(&buffer), "word**");
    assert_eq!(buffer.iter_at_mark(&buffer.get_insert()).offset(), 5);
}

fn inline_command_replaces_selected_text_and_keeps_it_selected() {
    let buffer = buffer_with("word");
    buffer.select_range(&buffer.start_iter(), &buffer.end_iter());

    apply_buffer_command(
        &buffer,
        SourceCommand::ToggleInline {
            opening: "*".into(),
            closing: "*".into(),
        },
    );

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

    apply_buffer_command(&buffer, SourceCommand::SetHeading(3));

    assert_eq!(text(&buffer), "### One\n### Two\n### Three");
}

fn heading_level_zero_removes_existing_markers() {
    let buffer = buffer_with("### One\nTwo");
    buffer.select_range(&buffer.start_iter(), &buffer.end_iter());

    apply_buffer_command(&buffer, SourceCommand::SetHeading(0));

    assert_eq!(text(&buffer), "One\nTwo");
}

fn list_command_adds_and_removes_supported_markers() {
    let buffer = buffer_with("- [ ] One\n2. Two\n* Three");
    buffer.select_range(&buffer.start_iter(), &buffer.end_iter());

    apply_buffer_command(&buffer, SourceCommand::ToggleList("- ".into()));

    assert_eq!(text(&buffer), "- One\n- Two\n- Three");
    buffer.select_range(&buffer.start_iter(), &buffer.end_iter());
    apply_buffer_command(&buffer, SourceCommand::ToggleList("- ".into()));
    assert_eq!(text(&buffer), "One\nTwo\nThree");
}

fn code_block_command_wraps_then_unwraps_the_selection() {
    let buffer = buffer_with("let value = 1;");
    buffer.select_range(&buffer.start_iter(), &buffer.end_iter());

    apply_buffer_command(&buffer, SourceCommand::ToggleCodeBlock);
    assert_eq!(text(&buffer), "```\nlet value = 1;\n```");

    buffer.select_range(&buffer.start_iter(), &buffer.end_iter());
    apply_buffer_command(&buffer, SourceCommand::ToggleCodeBlock);
    assert_eq!(text(&buffer), "let value = 1;");
}

fn code_block_command_without_selection_uses_inline_code() {
    let buffer = buffer_with("");

    apply_buffer_command(&buffer, SourceCommand::ToggleCodeBlock);

    assert_eq!(text(&buffer), "``");
}

fn link_command_replaces_selection_or_inserts_at_cursor() {
    let selected = buffer_with("Carver");
    selected.select_range(&selected.start_iter(), &selected.end_iter());
    apply_buffer_command(
        &selected,
        SourceCommand::InsertLink {
            text: "Carver".into(),
            destination: "https://example.com".into(),
        },
    );
    assert_eq!(text(&selected), "[Carver](https://example.com)");

    let inserted = buffer_with("Read ");
    let end = inserted.end_iter();
    inserted.place_cursor(&end);
    apply_buffer_command(
        &inserted,
        SourceCommand::InsertLink {
            text: "more".into(),
            destination: "https://example.com/more".into(),
        },
    );
    assert_eq!(text(&inserted), "Read [more](https://example.com/more)");
}

fn image_width_command_updates_the_direct_image_at_the_cursor() {
    let buffer = buffer_with("![Diagram](assets/diagram.png){width=\"25%\"}");
    let cursor = buffer.iter_at_offset(30);
    buffer.place_cursor(&cursor);
    apply_buffer_command(&buffer, SourceCommand::SetImageWidth(Some(50)));
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

fn apply_buffer_command(buffer: &gtk::TextBuffer, command: SourceCommand) {
    let edit = SourceEdit::apply(text(buffer), selection_from_buffer(buffer), command);
    replace_source_buffer(buffer, edit.source());
    let selection = edit.selection();
    let start = buffer.iter_at_offset(i32::try_from(selection.start).unwrap_or(i32::MAX));
    let end = buffer.iter_at_offset(i32::try_from(selection.end).unwrap_or(i32::MAX));
    buffer.select_range(&start, &end);
}
