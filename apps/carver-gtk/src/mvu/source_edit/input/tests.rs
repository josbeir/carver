use super::*;

fn edit(
    source: &str,
    selection: Range<usize>,
    input: SourceInput,
) -> Result<SourceEdit, Box<dyn std::error::Error>> {
    let analysis = SourceAnalysis::parse(source);
    match SourceEdit::plan_input(source, &analysis, selection, input)? {
        SourceInputOutcome::Edit(edit) => Ok(edit),
        outcome => Err(format!("expected edit, got {outcome:?} for {source:?}").into()),
    }
}

fn enter_end(source: &str) -> Result<SourceEdit, Box<dyn std::error::Error>> {
    let end = source.chars().count();
    edit(source, end..end, SourceInput::Enter)
}

#[test]
fn enter_should_continue_every_supported_list_style() -> Result<(), Box<dyn std::error::Error>> {
    for (source, prefix) in [
        ("- bullet", "- "),
        ("* bullet", "* "),
        (". ordered", ". "),
        ("10. numbered", "11. "),
        ("9) numbered", "10) "),
        ("b. alpha", "c. "),
        ("H) alpha", "I) "),
        ("z. alpha", "a. "),
        ("iv. roman", "v. "),
        ("IV) roman", "V) "),
        ("iv. first\nv. second", "vi. "),
        ("h. first\ni. second", "j. "),
        ("- [X] task", "- [ ] "),
        ("* [?] task", "* [ ] "),
        (">   - task", ">   - "),
    ] {
        let result = enter_end(source)?;
        let expected = format!("{source}\n{prefix}");
        assert_eq!(result.source(), expected);
        assert_eq!(
            result.selection(),
            expected.chars().count()..expected.chars().count()
        );
    }
    Ok(())
}

#[test]
fn enter_should_split_unicode_text_and_preserve_the_remaining_source()
-> Result<(), Box<dyn std::error::Error>> {
    let result = edit("- café☕ tail\n- last", 7..7, SourceInput::Enter)?;
    assert_eq!(result.source(), "- café☕\n-  tail\n- last");
    assert_eq!(result.selection(), 10..10);
    Ok(())
}

#[test]
fn enter_should_replace_a_selection_within_item_text() -> Result<(), Box<dyn std::error::Error>> {
    let result = edit("- first selected tail", 8..16, SourceInput::Enter)?;
    assert_eq!(result.source(), "- first \n-  tail");
    assert_eq!(result.selection(), 11..11);
    Ok(())
}

#[test]
fn enter_should_continue_item_paragraphs_at_the_item_indent()
-> Result<(), Box<dyn std::error::Error>> {
    let result = enter_end("- first\n  continuation")?;
    assert_eq!(result.source(), "- first\n  continuation\n- ");
    Ok(())
}

#[test]
fn enter_should_preserve_existing_attributes_without_copying_them()
-> Result<(), Box<dyn std::error::Error>> {
    let result = enter_end("3.{#unique title=\"café } text\"} item")?;
    assert_eq!(
        result.source(),
        "3.{#unique title=\"café } text\"} item\n4. "
    );
    Ok(())
}

#[test]
fn enter_should_exit_empty_top_level_items_without_inserting_another_newline()
-> Result<(), Box<dyn std::error::Error>> {
    for source in ["- ", "* [ ] ", ". ", "1. ", "iv.{#id} ", "  - "] {
        let result = enter_end(source)?;
        assert_eq!(result.source(), "", "{source}");
        assert_eq!(result.selection(), 0..0);
    }
    Ok(())
}

#[test]
fn enter_should_step_empty_nested_items_back_to_the_parent_list()
-> Result<(), Box<dyn std::error::Error>> {
    let result = enter_end("3. parent\n   - [ ] ")?;
    assert_eq!(result.source(), "3. parent\n4. ");
    assert_eq!(result.selection(), 13..13);
    Ok(())
}

#[test]
fn enter_should_step_back_one_level_in_deeply_nested_lists()
-> Result<(), Box<dyn std::error::Error>> {
    let result = enter_end("- parent\n  * child\n    - ")?;
    assert_eq!(result.source(), "- parent\n  * child\n  * ");
    Ok(())
}

#[test]
fn enter_should_continue_and_exit_quotes_one_level_at_a_time()
-> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(enter_end("> > text")?.source(), "> > text\n> > ");
    assert_eq!(enter_end("> > ")?.source(), "> ");
    assert_eq!(enter_end("> ")?.source(), "");
    assert_eq!(enter_end("> - ")?.source(), "> ");
    Ok(())
}

#[test]
fn input_should_defer_to_native_editing_in_protected_or_nonlist_contexts()
-> Result<(), Box<dyn std::error::Error>> {
    for source in [
        "plain",
        "# Heading",
        "---",
        "\\- literal",
        "+ continuation",
        "```\n- code",
        "```\n- code\n```",
        "---\ntitle: test\n- value\n---",
        "%% - comment",
    ] {
        let end = source.chars().count();
        let analysis = SourceAnalysis::parse(source);
        for input in [
            SourceInput::Enter,
            SourceInput::IndentList,
            SourceInput::OutdentList,
        ] {
            assert_eq!(
                SourceEdit::plan_input(source, &analysis, end..end, input)?,
                SourceInputOutcome::Native,
                "{source:?} {input:?}"
            );
        }
    }
    Ok(())
}

#[test]
fn input_should_defer_inside_raw_blocks_tables_and_fenced_comments()
-> Result<(), Box<dyn std::error::Error>> {
    for source in [
        "```=html\n- raw\n```",
        "|= Header|\n| - value|",
        "%%%\n- comment\n%%%",
    ] {
        let cursor = source.find("- ").ok_or("fixture")? + 2;
        let analysis = SourceAnalysis::parse(source);
        assert!(analysis.protects_editing(cursor..cursor), "{source}");
        for input in [
            SourceInput::Enter,
            SourceInput::IndentList,
            SourceInput::OutdentList,
        ] {
            assert_eq!(
                SourceEdit::plan_input(source, &analysis, cursor..cursor, input)?,
                SourceInputOutcome::Native
            );
        }
    }
    Ok(())
}

#[test]
fn enter_should_defer_to_native_editing_inside_a_marker_or_across_items() {
    for (source, selection) in [
        ("- text", 1..1),
        ("- first\n- second", 3..13),
        ("3.{#id} text", 5..5),
    ] {
        let analysis = SourceAnalysis::parse(source);
        assert!(!SourceInput::Enter.accepts(source, &analysis, selection));
    }
}

#[test]
fn input_should_defer_when_an_ordinal_cannot_be_incremented() {
    let source = format!("# Heading\n\n{}. item", usize::MAX);
    let end = source.chars().count();
    assert!(!SourceInput::Enter.accepts(&source, &SourceAnalysis::parse(&source), end..end));
}

#[test]
fn tab_should_move_an_item_with_its_children_and_continuation_content()
-> Result<(), Box<dyn std::error::Error>> {
    let source = "- previous\n- parent\n  continuation\n  * child\n- last";
    let result = edit(source, 19..19, SourceInput::IndentList)?;
    assert_eq!(
        result.source(),
        "- previous\n  - parent\n    continuation\n    * child\n- last"
    );
    assert_eq!(result.selection(), 21..21);
    Ok(())
}

#[test]
fn tab_should_align_under_a_wide_ordered_marker_without_counting_attributes()
-> Result<(), Box<dyn std::error::Error>> {
    let source = "10.{#id} previous\n11. next";
    let end = source.chars().count();
    let result = edit(source, end..end, SourceInput::IndentList)?;
    assert_eq!(result.source(), "10.{#id} previous\n    11. next");
    Ok(())
}

#[test]
fn tab_should_move_selected_sibling_subtrees_without_touching_the_next_item()
-> Result<(), Box<dyn std::error::Error>> {
    let source = "- previous\n- one\n  * child\n- two\n- last";
    let start = source.find("- one").ok_or("marker")?;
    let end = source.find("- last").ok_or("marker")?;
    let result = edit(source, start..end, SourceInput::IndentList)?;
    assert_eq!(
        result.source(),
        "- previous\n  - one\n    * child\n  - two\n- last"
    );
    assert_eq!(result.selection(), start + 2..end + 6);
    Ok(())
}

#[test]
fn shift_tab_should_lift_an_item_and_keep_later_siblings_nested()
-> Result<(), Box<dyn std::error::Error>> {
    let source = "- parent\n  * one\n    - child\n  * two\n- last";
    let cursor = source.find("one").ok_or("marker")? + 3;
    let result = edit(source, cursor..cursor, SourceInput::OutdentList)?;
    assert_eq!(
        result.source(),
        "- parent\n* one\n  - child\n  * two\n- last"
    );
    assert_eq!(result.selection(), cursor - 2..cursor - 2);
    Ok(())
}

#[test]
fn nesting_should_preserve_quoted_list_prefixes() -> Result<(), Box<dyn std::error::Error>> {
    let source = "> - first\n> - second\n>   * child";
    let cursor = source.find("second").ok_or("marker")?;
    let nested = edit(source, cursor..cursor, SourceInput::IndentList)?;
    assert_eq!(nested.source(), "> - first\n>   - second\n>     * child");
    let lifted = edit(
        nested.source(),
        nested.selection(),
        SourceInput::OutdentList,
    )?;
    assert_eq!(lifted.source(), source);
    Ok(())
}

#[test]
fn nesting_should_consume_invalid_moves_without_an_edit() -> Result<(), Box<dyn std::error::Error>>
{
    let source = "- first\n- second";
    let analysis = SourceAnalysis::parse(source);
    assert_eq!(
        SourceEdit::plan_input(source, &analysis, 4..4, SourceInput::IndentList)?,
        SourceInputOutcome::Noop
    );
    assert_eq!(
        SourceEdit::plan_input(source, &analysis, 14..14, SourceInput::OutdentList)?,
        SourceInputOutcome::Noop
    );
    Ok(())
}

#[test]
fn hard_break_should_keep_the_cursor_inside_quoted_task_content() {
    let source = "> - [x] café";
    let end = source.chars().count();
    let result = SourceEdit::apply(
        source.into(),
        end..end,
        super::super::super::SourceCommand::InsertHardBreak,
    );
    assert_eq!(result.source(), "> - [x] café\\\n>   ");
    assert_eq!(result.selection(), 18..18);
}

#[test]
fn tab_should_nest_a_new_empty_item_and_shift_tab_should_lift_it()
-> Result<(), Box<dyn std::error::Error>> {
    let nested = edit("- first\n- ", 10..10, SourceInput::IndentList)?;
    assert_eq!(nested.source(), "- first\n  - ");
    let lifted = edit(
        nested.source(),
        nested.selection(),
        SourceInput::OutdentList,
    )?;
    assert_eq!(lifted.source(), "- first\n- ");
    Ok(())
}

#[test]
fn nesting_should_defer_for_a_selection_crossing_into_ordinary_text() {
    let source = "- one\n- two\n\nplain";
    let analysis = SourceAnalysis::parse(source);
    assert!(!SourceInput::IndentList.accepts(source, &analysis, 6..source.len()));
}

#[test]
fn nesting_should_preserve_tabs_when_their_columns_still_fit()
-> Result<(), Box<dyn std::error::Error>> {
    let source = "- parent\n    - first\n    - second\n\t  * child";
    let cursor = source.find("second").ok_or("marker")?;
    let nested = edit(source, cursor..cursor, SourceInput::IndentList)?;
    assert_eq!(
        nested.source(),
        "- parent\n    - first\n      - second\n\t    * child"
    );
    Ok(())
}

#[test]
fn enter_should_use_a_nested_lists_own_dialect_when_its_marker_is_empty()
-> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(
        enter_end("iv. parent\n    2. child")?.source(),
        "iv. parent\n    2. child\n    3. "
    );
    assert_eq!(enter_end("iv. parent\n    - ")?.source(), "iv. parent\nv. ");
    Ok(())
}
