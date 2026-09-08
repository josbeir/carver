use std::ops::Range;

use super::{MediaKind, SourceAnalysis, SourceNodeKind};

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
fn context_should_report_definition_list_terms_and_descriptions() {
    let source = ":: Carve\n: A post-Markdown lightweight markup language.\n\n:: Djot\n: The markup language Carve evolves from.";
    let analysis = SourceAnalysis::parse(source);

    assert_eq!(
        analysis
            .context_for(4..4)
            .map(|context| context.breadcrumb()),
        Some(String::from("dl › dt"))
    );
    assert_eq!(
        analysis
            .context_for(14..14)
            .map(|context| context.breadcrumb()),
        Some(String::from("dl › dd"))
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
fn media_should_use_positioned_carve_ast_nodes() {
    let source = "![Diagram](assets/diagram.png) [Brief](assets/brief.pdf) `![code](assets/no.png)` [web](https://example.test)";
    let media = SourceAnalysis::parse(source).media().to_vec();

    assert_eq!(media.len(), 2);
    assert_eq!(media[0].kind, MediaKind::Image);
    assert_eq!(media[0].path, "assets/diagram.png");
    assert_eq!(media[0].label, "Diagram");
    assert_eq!(media[1].kind, MediaKind::Attachment);
    assert_eq!(media[1].path, "assets/brief.pdf");
    assert_eq!(media[1].label, "Brief");
    assert_eq!(&source[media[1].range.clone()], "[Brief](assets/brief.pdf)");
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

#[test]
fn headings_should_follow_ast_order_and_ignore_code() {
    let source = "# Same\n\n### Same\n\n> ## Nested\n\n```\n# Not a heading\n```";
    let analysis = SourceAnalysis::parse(source);
    assert_eq!(
        analysis
            .headings()
            .iter()
            .map(|h| (h.level, h.label.as_str()))
            .collect::<Vec<_>>(),
        vec![(1, "Same"), (3, "Same"), (2, "Nested")]
    );
}

#[test]
fn heading_labels_should_flatten_inline_markup_and_preserve_unicode_positions() {
    let source = "🦀\n\n## *Bold* [link](https://example.test) `code` é";
    let analysis = SourceAnalysis::parse(source);
    let heading = &analysis.headings()[0];
    assert_eq!(heading.label, "Bold link code é");
    assert_eq!(heading.range.start, 3);
    assert_eq!(
        source
            .chars()
            .skip(heading.text_start)
            .take(6)
            .collect::<String>(),
        "*Bold*"
    );
}

#[test]
fn empty_heading_should_have_a_navigable_fallback_label() {
    let mut analysis = SourceAnalysis::default();
    analysis.visit_block(
        &carve::BlockNode::Heading(carve::Heading {
            level: 1,
            children: Vec::new(),
            attrs: None,
            pos: Some(carve::Pos {
                start_offset: 0,
                end_offset: 2,
                ..Default::default()
            }),
        }),
        &mut Vec::new(),
    );
    assert_eq!(analysis.headings()[0].label, "Untitled heading");
}

#[test]
fn nested_heading_should_focus_after_its_authored_prefix() {
    let source = "> ## Nested";
    let analysis = SourceAnalysis::parse(source);
    assert_eq!(
        source
            .chars()
            .skip(analysis.headings()[0].text_start)
            .collect::<String>(),
        "Nested"
    );
}

#[test]
fn labels_should_keep_autolink_text_and_smart_punctuation() {
    let analysis = SourceAnalysis::parse("# Go <https://example.test> -- now");
    assert!(
        analysis.headings()[0]
            .label
            .contains("https://example.test")
    );
    assert!(analysis.headings()[0].label.contains("now"));
    assert!(!analysis.headings()[0].label.contains("  now"));
}
