use std::ops::Range;

use super::{SourceAnalysis, SourceNodeKind};

fn context(source: &str, selection: Range<usize>) -> Vec<SourceNodeKind> {
    SourceAnalysis::parse(source)
        .context_for(selection)
        .map(|context| context.path().to_vec())
        .unwrap_or_default()
}

#[test]
fn context_should_report_paragraph_and_bold_at_a_bold_cursor() {
    assert_eq!(
        context("*bold*", 2..2),
        vec![SourceNodeKind::Paragraph, SourceNodeKind::Bold]
    );
}

#[test]
fn breadcrumb_should_show_authored_paragraphs_without_implicit_list_item_paragraphs() {
    let paragraph = SourceAnalysis::parse("plain text");
    assert_eq!(
        paragraph
            .context_for(3..3)
            .map(|context| context.breadcrumb()),
        Some(String::from("p"))
    );

    let ordered_list = SourceAnalysis::parse("1. list item");
    assert_eq!(
        ordered_list
            .context_for(4..4)
            .map(|context| context.breadcrumb()),
        Some(String::from("ol › li"))
    );
}

#[test]
fn context_should_report_document_frontmatter_from_the_carve_ast() {
    let source = "---yaml\ntitle: Carve Feature Demo\n---\n\n# Note";
    let analysis = SourceAnalysis::parse(source);

    assert_eq!(
        analysis
            .context_for(9..9)
            .map(|context| context.path().to_vec()),
        Some(vec![SourceNodeKind::Frontmatter])
    );
    assert_eq!(
        analysis
            .context_for(9..9)
            .map(|context| context.breadcrumb()),
        Some(String::from("frontmatter"))
    );
    assert_ne!(
        SourceAnalysis::parse("# Note\n\n---\ntitle: not frontmatter\n---")
            .context_for(12..12)
            .map(|context| context.breadcrumb()),
        Some(String::from("frontmatter"))
    );
}

#[test]
fn context_should_report_heading_and_bold_for_nested_markup() {
    assert_eq!(
        context("# *bold*", 4..4),
        vec![SourceNodeKind::Heading(1), SourceNodeKind::Bold]
    );
}

#[test]
fn context_should_keep_only_shared_ancestors_for_mixed_selection() {
    assert_eq!(
        context("# *bold* /italic/", 2..17),
        vec![SourceNodeKind::Heading(1)]
    );
}

#[test]
fn context_should_use_unicode_code_point_offsets() {
    assert_eq!(
        context("Café *bold*", 7..7),
        vec![SourceNodeKind::Paragraph, SourceNodeKind::Bold]
    );
}

#[test]
fn context_should_include_image_width_and_table_ancestry() {
    let image = SourceAnalysis::parse("![Diagram](assets/diagram.png){width=\"50%\"}");
    assert_eq!(
        image
            .context_for(30..30)
            .map(|context| context.path().to_vec()),
        Some(vec![SourceNodeKind::Image { width: Some(50) }])
    );

    let table = SourceAnalysis::parse("|= Heading|\n| value|");
    assert_eq!(
        table
            .context_for(4..4)
            .map(|context| context.path().to_vec()),
        Some(vec![
            SourceNodeKind::Table,
            SourceNodeKind::TableRow,
            SourceNodeKind::TableHeader,
        ])
    );
}

#[test]
fn context_should_cover_quotes_lists_code_and_comments() -> Result<(), Box<dyn std::error::Error>> {
    let source = "> /italic/\n\n- [ ] task\n\n```\ncode\n```\n\n%% note";
    let analysis = SourceAnalysis::parse(source);

    for needle in ["italic", "task", "code", "note"] {
        let offset = source.find(needle).ok_or("fixture marker")?;
        assert!(analysis.context_for(offset..offset).is_some());
    }
    Ok(())
}
