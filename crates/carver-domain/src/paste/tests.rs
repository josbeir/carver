use super::{PasteIntent, PastedFormat, detect_pasted_format, import_pasted_text};

#[test]
fn plain_prose_should_stay_plain_and_verbatim() {
    let text = "Just an ordinary sentence about Carve and Markdown.";
    let pasted = import_pasted_text(text, PasteIntent::Auto);

    assert_eq!(pasted.format, PastedFormat::Plain);
    assert_eq!(pasted.source, text);
}

#[test]
fn empty_or_whitespace_should_be_plain() {
    assert_eq!(detect_pasted_format(""), PastedFormat::Plain);
    assert_eq!(detect_pasted_format("   \n\t"), PastedFormat::Plain);
}

#[test]
fn oversized_text_should_not_be_parsed_for_detection() {
    let text = format!("**{}**", "x".repeat(300 * 1024));
    assert_eq!(detect_pasted_format(&text), PastedFormat::Plain);
}

#[test]
fn ambiguous_single_delimiter_emphasis_should_prefer_carve() {
    assert_eq!(detect_pasted_format("*bold*"), PastedFormat::Carve);
    assert_eq!(detect_pasted_format("_underline_"), PastedFormat::Carve);
}

#[test]
fn carve_inline_and_block_markup_should_be_detected() {
    for source in [
        "/italic/",
        "# Heading",
        "- item one\n- item two",
        "> quote",
        "`code`",
        "A [link](https://example.com)",
        "---\ntitle: Note\n---\n\nBody",
    ] {
        assert_eq!(
            detect_pasted_format(source),
            PastedFormat::Carve,
            "expected Carve for {source:?}"
        );
    }
}

#[test]
fn markdown_only_delimiters_should_be_detected_as_markdown() {
    for source in [
        "**bold**",
        "__bold__",
        "~~strike~~",
        "Title\n=====",
        "| Name | State |\n| --- | --- |\n| Carver | Ready |",
    ] {
        assert_eq!(
            detect_pasted_format(source),
            PastedFormat::Markdown,
            "expected Markdown for {source:?}"
        );
    }
}

#[test]
fn markdown_import_should_convert_through_carve_migration() {
    let pasted = import_pasted_text("**bold** and ~~old~~", PasteIntent::Auto);

    assert_eq!(pasted.format, PastedFormat::Markdown);
    assert!(
        pasted.source.contains("*bold*"),
        "expected migrated Carve, got {:?}",
        pasted.source
    );
}

#[test]
fn carve_intent_should_preserve_the_text_verbatim() {
    let text = "**not migrated**";
    let pasted = import_pasted_text(text, PasteIntent::Carve);

    assert_eq!(pasted.format, PastedFormat::Carve);
    assert_eq!(pasted.source, text);
}

#[test]
fn markdown_intent_should_force_migration_for_ambiguous_text() {
    let pasted = import_pasted_text("*em*", PasteIntent::Markdown);

    assert_eq!(pasted.format, PastedFormat::Markdown);
    assert!(
        pasted.source.contains("/em/"),
        "expected migrated Carve italic, got {:?}",
        pasted.source
    );
}

#[test]
fn carve_conversion_should_preserve_plain_multi_paragraph_text() {
    let text = "First paragraph.\n\nSecond paragraph.";
    let pasted = import_pasted_text(text, PasteIntent::Auto);

    assert_eq!(pasted.format, PastedFormat::Carve);
    assert_eq!(pasted.source, text);
}
