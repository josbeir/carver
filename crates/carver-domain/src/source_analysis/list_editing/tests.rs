use super::*;

#[test]
fn quote_prefix_should_preserve_nested_markers_beyond_the_requested_depth() {
    let line = " >\t>   > café";
    assert_eq!(quote_prefix_at_depth(line, 0), ("", line));
    assert_eq!(quote_prefix_at_depth(line, 1), (" >", "\t>   > café"));
    assert_eq!(quote_prefix_at_depth(line, 2), (" >\t> ", "  > café"));
    assert_eq!(quote_prefix(line), (" >\t>   > ", "café"));
    assert_eq!(quote_prefix_at_depth("plain", 2), ("", "plain"));
}

#[test]
fn successor_should_preserve_every_carve_list_dialect() -> Result<(), Box<dyn std::error::Error>> {
    for (source, expected) in [
        ("- text", "- "),
        ("* text", "* "),
        (". text", ". "),
        ("3. text", "4. "),
        ("9) text", "10) "),
        ("b. text", "c. "),
        ("H) text", "I) "),
        ("z. text", "a. "),
        ("iv. text", "v. "),
        ("IX) text", "X) "),
        ("- [x] text", "- [ ] "),
        ("* [?] text", "* [ ] "),
        (">   - [>] text", ">   - [ ] "),
    ] {
        let prefix = ListPrefix::parse(source, None).ok_or("list prefix")?;
        assert_eq!(prefix.successor()?, expected, "{source}");
    }
    Ok(())
}

#[test]
fn successor_should_follow_the_enclosing_roman_dialect() -> Result<(), Box<dyn std::error::Error>> {
    let prefix =
        ListPrefix::parse("v. item", Some(OrderedListType::LowerRoman)).ok_or("list prefix")?;
    assert_eq!(prefix.successor()?, "vi. ");
    Ok(())
}

#[test]
fn prefix_should_accept_empty_items_and_every_task_state() -> Result<(), Box<dyn std::error::Error>>
{
    for state in [' ', 'x', 'X', '-', '_', '>', '?'] {
        for separator in ["", " ", "  \t"] {
            let source = format!("- [{state}]{separator}");
            let prefix = ListPrefix::parse(&source, None).ok_or("list prefix")?;
            assert!(prefix.is_task());
            assert_eq!(prefix.content_start, source.len());
            assert_eq!(
                prefix.successor()?,
                format!(
                    "- [ ]{}",
                    if separator.is_empty() { " " } else { separator }
                )
            );
        }
    }
    assert!(ListPrefix::parse("- ", None).is_some());
    Ok(())
}

#[test]
fn successor_should_drop_attributes_without_changing_marker_spacing()
-> Result<(), Box<dyn std::error::Error>> {
    let prefix =
        ListPrefix::parse(" >  3.{#unique title=\"a } café\"}  text", None).ok_or("list prefix")?;
    assert_eq!(prefix.successor()?, " >  4.  ");
    assert_eq!(prefix.content_column, 5);
    Ok(())
}

#[test]
fn prefix_should_reject_nonmarkers_and_invalid_attributes() -> Result<(), Box<dyn std::error::Error>>
{
    for source in [
        "+ continuation",
        "---",
        "-\ttext",
        "\\- text",
        "-- text",
        "-{title=\"unterminated} text",
        "ab. text",
    ] {
        assert!(ListPrefix::parse(source, None).is_none(), "{source}");
    }
    let prefix = ListPrefix::parse("- [!] literal", None).ok_or("list prefix")?;
    assert!(!prefix.is_task());
    assert_eq!(&"- [!] literal"[prefix.content_start..], "[!] literal");
    Ok(())
}

#[test]
fn successor_should_report_decimal_overflow() -> Result<(), Box<dyn std::error::Error>> {
    let source = format!("{}. text", usize::MAX);
    assert!(matches!(
        ListPrefix::parse(&source, None)
            .ok_or("list prefix")?
            .successor(),
        Err(ListPrefixError::OrdinalOverflow)
    ));
    Ok(())
}

#[test]
fn successor_should_render_the_largest_decimal_without_overflowing()
-> Result<(), Box<dyn std::error::Error>> {
    let source = format!("{}) text", usize::MAX - 1);
    assert_eq!(
        ListPrefix::parse(&source, None)
            .ok_or("prefix")?
            .successor()?,
        format!("{}) ", usize::MAX)
    );
    Ok(())
}

#[test]
fn prefix_should_measure_separator_tabs_from_the_marker_column()
-> Result<(), Box<dyn std::error::Error>> {
    let prefix = ListPrefix::parse("  - \ttext", None).ok_or("prefix")?;
    assert_eq!(prefix.content_column, 8);
    assert_eq!(prefix.successor()?, "  - \t");
    Ok(())
}

#[test]
fn prefix_should_measure_task_indentation_without_checkbox_or_attributes()
-> Result<(), Box<dyn std::error::Error>> {
    let prefix = ListPrefix::parse("\t-{#id} [ ] text", None).ok_or("list prefix")?;
    assert_eq!(prefix.indent, 4);
    assert_eq!(prefix.content_column, 6);
    Ok(())
}
