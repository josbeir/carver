//! Display-backed Source mode formatting controls and split-preview coverage.
use super::*;

pub(super) fn exercise_source_formatting_controls(
    editor: &gtk::Widget,
    source: &gtk::TextView,
    buffer: &gtk::TextBuffer,
) -> TestResult {
    for name in [
        "format-bold-button",
        "format-italic-button",
        "format-strike-button",
        "format-underline-button",
        "format-highlight-button",
        "format-superscript-button",
        "format-subscript-button",
        "format-bullet-button",
        "format-ordered-button",
        "format-task-button",
        "format-code-button",
        "format-code-block-button",
    ] {
        buffer.set_text("format me");
        select_all(buffer);
        let button = widget_as::<gtk::ToggleButton>(editor, name).ok_or(name)?;
        button.grab_focus();
        button.emit_clicked();
        assert_source_focus_restored(source, name)?;
        if name == "format-bold-button"
            && buffer
                .selection_bounds()
                .is_none_or(|(start, end)| buffer.text(&start, &end, false) != "*format me*")
        {
            return Err("bold should preserve the transformed source selection".into());
        }
    }
    buffer.set_text("Level 1\nLevel 2\nLevel 3\nLevel 4");
    select_all(buffer);
    widget_as::<gtk::ToggleButton>(editor, "format-ordered-button")
        .ok_or("format-ordered-button")?
        .emit_clicked();
    if buffer.text(&buffer.start_iter(), &buffer.end_iter(), false)
        != ". Level 1\n. Level 2\n. Level 3\n. Level 4"
    {
        return Err("source ordered-list serialization".into());
    }
    let heading = widget_as::<gtk::MenuButton>(editor, "format-heading-button")
        .ok_or("format heading picker")?;
    let choices = heading
        .popover()
        .and_then(|popover| popover.child())
        .ok_or("formatting choices")?;
    let mut child = choices.first_child();
    while let Some(widget) = child {
        child = widget.next_sibling();
        buffer.set_text("format me");
        select_all(buffer);
        let choice = widget
            .downcast::<gtk::ToggleButton>()
            .map_err(|_| "formatting choice")?;
        choice.grab_focus();
        choice.emit_clicked();
        assert_source_focus_restored(source, "heading choice")?;
    }
    let table =
        widget_as::<gtk::MenuButton>(editor, "format-table-button").ok_or("source table picker")?;
    let table_content = table
        .popover()
        .and_then(|popover| popover.child())
        .and_downcast::<gtk::Box>()
        .ok_or("source table picker content")?;
    let grid = table_content
        .first_child()
        .and_then(|dimensions| dimensions.next_sibling())
        .and_downcast::<gtk::Grid>()
        .ok_or("source table size grid")?;
    buffer.set_text("");
    let cell = grid
        .first_child()
        .and_downcast::<gtk::Button>()
        .ok_or("source table first cell")?;
    cell.grab_focus();
    cell.emit_clicked();
    assert_source_focus_restored(source, "table picker")?;
    if !buffer
        .text(&buffer.start_iter(), &buffer.end_iter(), false)
        .contains("|= |")
    {
        return Err("source table picker insert".into());
    }
    let image =
        widget_as::<gtk::MenuButton>(editor, "format-image-button").ok_or("source image picker")?;
    let image_choices = image
        .popover()
        .and_then(|popover| popover.child())
        .ok_or("source image choices")?;
    let width = image_choices
        .first_child()
        .and_then(|insert| insert.next_sibling())
        .and_then(|separator| separator.next_sibling())
        .and_downcast::<gtk::ToggleButton>()
        .ok_or("source image width")?;
    buffer.set_text("![First](assets/first.png)");
    buffer.place_cursor(&buffer.iter_at_offset(4));
    width.grab_focus();
    width.emit_clicked();
    assert_source_focus_restored(source, "image width")?;
    Ok(())
}

pub(super) fn assert_source_focus_restored(source: &gtk::TextView, action: &str) -> TestResult {
    if run_main_context_until(|| widget_is_window_focus(source.upcast_ref())) {
        Ok(())
    } else {
        Err(format!("{action} should restore source focus").into())
    }
}

pub(super) fn assert_shared_toolbar_controls(root: &gtk::Widget) -> TestResult {
    for name in [
        "format-bold-button",
        "format-italic-button",
        "format-strike-button",
        "format-underline-button",
        "format-highlight-button",
        "format-superscript-button",
        "format-subscript-button",
        "format-code-button",
        "format-code-block-button",
        "format-bullet-button",
        "format-ordered-button",
        "format-task-button",
        "format-link-button",
    ] {
        widget_as::<gtk::ToggleButton>(root, name).ok_or(name)?;
    }
    for name in [
        "format-heading-button",
        "format-table-button",
        "format-image-button",
    ] {
        widget_as::<gtk::MenuButton>(root, name).ok_or(name)?;
    }
    Ok(())
}

pub(super) fn assert_split_preview_tracks_source_scroll(
    editor: &gtk::Widget,
    source: &gtk::TextView,
    source_mode: &gtk::ToggleButton,
) -> TestResult {
    source_mode.set_active(true);
    source.buffer().set_text(
        &(0..180)
            .map(|index| format!("Paragraph {index}"))
            .collect::<Vec<_>>()
            .join("\n\n"),
    );
    let split_toggle =
        widget_as::<gtk::ToggleButton>(editor, "source-split-toggle").ok_or("split toggle")?;
    split_toggle.set_active(true);
    let source_scroll = source
        .parent()
        .and_downcast::<gtk::ScrolledWindow>()
        .ok_or("source scroll")?;
    let adjustment = source_scroll.vadjustment();
    assert!(run_main_context_until(
        || adjustment.upper() > adjustment.page_size()
    ));
    adjustment.set_value((adjustment.upper() - adjustment.page_size()) * 0.5);
    let scroll_before_format = adjustment.value();
    let paragraph = source.buffer().text(
        &source.buffer().start_iter(),
        &source.buffer().end_iter(),
        false,
    );
    let cursor = paragraph.find("Paragraph 90").ok_or("formatting cursor")?;
    let cursor = i32::try_from(cursor).map_err(|_| "formatting cursor offset")?;
    source
        .buffer()
        .place_cursor(&source.buffer().iter_at_offset(cursor));
    let bold =
        widget_as::<gtk::ToggleButton>(editor, "format-bold-button").ok_or("source bold button")?;
    bold.emit_clicked();
    assert!(run_main_context_until(|| {
        (adjustment.value() - scroll_before_format).abs() < 1.0
            && source
                .buffer()
                .text(
                    &source.buffer().start_iter(),
                    &source.buffer().end_iter(),
                    false,
                )
                .contains("**Paragraph 90")
    }));
    split_toggle.set_active(false);
    Ok(())
}

pub(super) fn formatting_controls_should_edit_carve(
    fixture: &WindowFixture,
    note: &carver_sdk::NoteSummary,
) -> TestResult {
    let client = &fixture.client;
    let root = fixture.root()?;
    let source = fixture.source()?;
    let source_buffer = fixture.source_buffer()?;
    source.buffer().set_text("# Source\n\nA paragraph");
    assert!(run_main_context_until(|| client
        .note(note.id)
        .ok()
        .flatten()
        .is_some_and(|saved| saved.source == "# Source\n\nA paragraph")));
    exercise_source_formatting_controls(&root, &source, &source.buffer())?;
    source.buffer().set_text("*fully bold*");
    source_buffer.ensure_highlight(&source.buffer().start_iter(), &source.buffer().end_iter());
    assert!(
        source_buffer.iter_has_context_class(&source.buffer().iter_at_offset(2), "carve-emphasis")
    );
    select_all(&source.buffer());
    assert!(run_main_context_until(|| {
        widget_as::<gtk::ToggleButton>(&root, "format-bold-button")
            .is_some_and(|button| button.is_active())
    }));
    source.buffer().set_text("=highlight= and {,subscript,}");
    source_buffer.ensure_highlight(&source.buffer().start_iter(), &source.buffer().end_iter());
    assert!(
        source_buffer.iter_has_context_class(&source.buffer().iter_at_offset(2), "carve-emphasis")
    );
    assert!(
        source_buffer.iter_has_context_class(&source.buffer().iter_at_offset(19), "carve-emphasis")
    );
    source.buffer().set_text("*bold* plain");
    select_all(&source.buffer());
    assert!(run_main_context_until(|| {
        widget_as::<gtk::ToggleButton>(&root, "format-bold-button")
            .is_some_and(|button| !button.is_active())
    }));
    source.buffer().set_text("# *bold*");
    source
        .buffer()
        .place_cursor(&source.buffer().iter_at_offset(4));
    let source_path = widget_as::<gtk::Label>(&root, "source-ast-path").ok_or("source AST path")?;
    assert!(run_main_context_until(|| {
        source_path.is_visible()
            && source_path.text() == "h1 › bold"
            && widget_as::<gtk::ToggleButton>(&root, "format-bold-button")
                .is_some_and(|button| button.is_active())
    }));
    Ok(())
}

pub(super) fn highlighting_should_mark_carve_constructs(fixture: &WindowFixture) -> TestResult {
    let source = fixture.source()?;
    let source_buffer = fixture.source_buffer()?;
    let source_path = fixture.source_path()?;
    source_buffer.ensure_highlight(&source.buffer().start_iter(), &source.buffer().end_iter());
    assert!(
        source_buffer.iter_has_context_class(&source.buffer().iter_at_offset(1), "carve-heading")
    );
    source
        .buffer()
        .set_text("---\ntitle: Carve Feature Demo\ndate: 2026-06-02\n---\n\n# Note");
    source
        .buffer()
        .place_cursor(&source.buffer().iter_at_offset(8));
    assert!(run_main_context_until(|| {
        source_path.is_visible() && source_path.text() == "frontmatter"
    }));
    source_buffer.ensure_highlight(&source.buffer().start_iter(), &source.buffer().end_iter());
    assert!(
        source_buffer
            .iter_has_context_class(&source.buffer().iter_at_offset(0), "carve-frontmatter")
    );
    assert!(
        source_buffer
            .iter_has_context_class(&source.buffer().iter_at_offset(4), "carve-frontmatter-key")
    );
    source.buffer().set_text("Before\n\n---\n\n# After");
    let divider_offset = 8;
    let heading_offset = 13;
    source_buffer.ensure_highlight(&source.buffer().start_iter(), &source.buffer().end_iter());
    assert!(source_buffer.iter_has_context_class(
        &source.buffer().iter_at_offset(divider_offset),
        "carve-thematic-break"
    ));
    assert!(source_buffer.iter_has_context_class(
        &source.buffer().iter_at_offset(heading_offset),
        "carve-heading"
    ));
    source
        .buffer()
        .set_text("# One\n## Two\n### Three\n#### Four\n##### Five\n###### Six");
    source_buffer.ensure_highlight(&source.buffer().start_iter(), &source.buffer().end_iter());
    for (offset, class) in [
        (0, "carve-heading-1"),
        (6, "carve-heading-2"),
        (13, "carve-heading-3"),
        (23, "carve-heading-4"),
        (33, "carve-heading-5"),
        (44, "carve-heading-6"),
    ] {
        let iter = source.buffer().iter_at_offset(offset);
        assert!(source_buffer.iter_has_context_class(&iter, "carve-heading"));
        assert!(source_buffer.iter_has_context_class(&iter, class));
    }
    Ok(())
}

pub(super) fn highlighting_should_mark_carve_blocks(fixture: &WindowFixture) -> TestResult {
    let source = fixture.source()?;
    let source_buffer = fixture.source_buffer()?;
    let source_path = fixture.source_path()?;
    source
        .buffer()
        .set_text(":: Carve\n: A post-Markdown lightweight markup language.");
    source
        .buffer()
        .place_cursor(&source.buffer().iter_at_offset(13));
    assert!(run_main_context_until(|| {
        source_path.is_visible() && source_path.text() == "dl › dd"
    }));
    source_buffer.ensure_highlight(&source.buffer().start_iter(), &source.buffer().end_iter());
    assert!(
        source_buffer
            .iter_has_context_class(&source.buffer().iter_at_offset(1), "carve-definition-term")
    );
    assert!(source_buffer.iter_has_context_class(
        &source.buffer().iter_at_offset(9),
        "carve-definition-marker"
    ));
    source
        .buffer()
        .set_text("|= Fruit |=> Price |=~ Stock |\n| Apple | 1.20 | In |");
    source_buffer.ensure_highlight(&source.buffer().start_iter(), &source.buffer().end_iter());
    assert!(
        source_buffer
            .iter_has_context_class(&source.buffer().iter_at_offset(10), "carve-table-marker")
    );
    assert!(
        source_buffer
            .iter_has_context_class(&source.buffer().iter_at_offset(11), "carve-table-marker")
    );
    assert!(
        !source_buffer.iter_has_context_class(&source.buffer().iter_at_offset(4), "carve-emphasis")
    );
    assert!(
        source_buffer
            .iter_has_context_class(&source.buffer().iter_at_offset(39), "carve-table-marker")
    );
    source.buffer().set_text("1. list item");
    source
        .buffer()
        .place_cursor(&source.buffer().iter_at_offset(4));
    assert!(run_main_context_until(|| {
        source_path.is_visible() && source_path.text() == "ol › li"
    }));
    source.buffer().set_text("plain text");
    source
        .buffer()
        .place_cursor(&source.buffer().iter_at_offset(3));
    assert!(run_main_context_until(|| {
        source_path.is_visible() && source_path.text() == "p"
    }));
    Ok(())
}

pub(super) fn rendered_preview_and_split_should_track_source(
    fixture: &WindowFixture,
) -> TestResult {
    let root = fixture.root()?;
    let source = fixture.source()?;
    let source_mode = fixture.source_mode()?;
    let source_path = fixture.source_path()?;
    let toolbar = fixture.toolbar()?;
    let find_bar = fixture.find_bar()?;
    source
        .buffer()
        .set_text("![First](assets/first.png){width=\"50%\"}");
    let rendered_mode =
        widget_as::<gtk::ToggleButton>(&root, "editor-mode-rendered").ok_or("rendered mode")?;
    rendered_mode.set_active(true);
    assert!(run_main_context_until(|| !toolbar.is_sensitive()
        && !source_path.is_visible()
        && !find_bar.is_search_mode()));
    let rendered_preview =
        widget_as::<webkit6::WebView>(&root, "editor-rendered-preview").ok_or("preview")?;
    assert_web_script_should_be_true(
        &rendered_preview,
        "getComputedStyle(document.body).fontFamily.includes('DejaVu Serif')",
    );
    assert_web_script_should_be_true(
        &rendered_preview,
        "(() => { const image = document.querySelector('body > img'); const body = document.body; const style = getComputedStyle(body); const contentWidth = body.clientWidth - parseFloat(style.paddingInlineStart) - parseFloat(style.paddingInlineEnd); return image && Math.abs(image.getBoundingClientRect().width - contentWidth / 2) < 1; })()",
    );
    source_mode.set_active(true);
    assert!(run_main_context_until(|| toolbar.is_sensitive()));
    assert_split_preview_tracks_source_scroll(&root, &source, &source_mode)?;
    Ok(())
}
