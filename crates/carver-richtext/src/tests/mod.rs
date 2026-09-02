use super::*;
#[test]
fn parses_and_serializes_visible_bold_and_images() {
    let document = parse_carve("# Plan\n*bold* ![diagram](assets/diagram.png)")
        .unwrap_or_else(|error| panic!("parse failed: {error}"));
    assert!(serialize_carve(&document).contains("![diagram](assets/diagram.png)"));
}

#[test]
fn parses_headings_and_all_common_list_markers() {
    let document = parse_carve("# Title\n- bullet\n1. numbered\n- [x] done")
        .unwrap_or_else(|error| panic!("parse failed: {error}"));
    assert_eq!(document.blocks.len(), 4);
    assert_eq!(
        serialize_carve(&document),
        "# Title\n- bullet\n1. numbered\n- [x] done"
    );
}

#[test]
fn parses_common_inline_formatting() {
    let document = parse_carve("`code` ~strike~ [link](https://example.test)")
        .unwrap_or_else(|error| panic!("parse failed: {error}"));
    assert_eq!(
        serialize_carve(&document),
        "`code` ~strike~ [link](https://example.test)"
    );
}

#[test]
fn parses_the_documented_wysiwyg_subset_without_losing_source() {
    let source = "# Carve WYSIWYG Demo\n\nThis is a *WYSIWYG editor* that outputs /Carve markup/.\n\n## Inline marks\n\n- *Strong* → `*text*`\n- /Emphasis/ → `/text/`\n- _Underline_ → `_text_`\n- =Highlight= → `=text=`\n- ~Strike~ → `~text~`\n- {+Inserted+} → `{+text+}`\n- {-Deleted-} → `{-text-}`\n- [HTML]{abbr=\"HyperText Markup Language\"} → `[HTML]{abbr=\"...\"}`\n\n## Task list\n\n- [x] Task lists round-trip to `- [x]`\n- [ ] Toggle the checkbox and watch the source\n\n> Edit the content and watch the Carve source below.\n\n```php\necho \"Hello, Carve!\";\n```";
    let document = parse_carve(source).unwrap_or_else(|error| panic!("parse failed: {error}"));
    assert_eq!(serialize_carve(&document), source);
    assert!(matches!(
        document.blocks.last(),
        Some(RichBlock::CodeBlock { language: Some(language), .. }) if language == "php"
    ));
}

#[test]
fn preserves_empty_and_trailing_paragraph_breaks() {
    let source = "First paragraph\n\nSecond paragraph\n";
    let document = parse_carve(source).unwrap_or_else(|error| panic!("parse failed: {error}"));
    assert_eq!(serialize_carve(&document), source);
}

#[test]
fn protects_tables_from_lossy_rich_editing() {
    assert_eq!(
        parse_carve("| A | B |"),
        Err(RichDocumentError::Unsupported(UnsupportedFeature::Table))
    );
}

#[test]
fn canonical_ast_routes_advanced_carve_to_the_full_renderer() {
    let source = "# Heading\n\n|= Name |= Value |\n| One | Two |\n\n::: note\nKeep this\n:::\n";
    assert_eq!(editor_projection(source), EditorProjection::RenderedOnly);
    let html = render_html(source);
    assert!(html.contains("<table"));
    assert!(html.contains("Keep this"));
}

#[test]
fn canonical_ast_routes_footnotes_to_the_full_renderer() {
    let source = "Text[^1]\n\n[^1]: Footnote";
    assert_eq!(editor_projection(source), EditorProjection::RenderedOnly);
}

#[test]
fn canonical_ast_routes_remote_images_to_the_full_renderer() {
    let source = "![remote](https://example.test/image.png)";
    assert_eq!(editor_projection(source), EditorProjection::RenderedOnly);
}

#[test]
fn canonical_ast_keeps_the_everyday_editor_subset_native() {
    let source = "## Plan\n\n- [x] *Bold* /italic/ _under_ =highlight= {^sup^} {,sub,}\n\n```rust\nlet ok = true;\n```";
    assert!(native_document(&parse(source)));
    assert!(parse_carve(source).is_ok());
    let projection = editor_projection(source);
    assert!(
        matches!(projection, EditorProjection::Native(_)),
        "{projection:?}"
    );
}

#[test]
fn canonical_ast_keeps_standalone_managed_images_editable() {
    let source = "# Heading 2\n\n# h2 testttttttttt\n\nthis is a note blabla hello world\n\n![Pasted image](assets/11ce96fc0358a8c741e4086afec6f3ae0ffb5e43f35f4b22f270a437016bcf9b.png)\n\n*blabla*\n\n*glalala*";
    let projection = editor_projection(source);
    assert!(
        matches!(projection, EditorProjection::Native(_)),
        "{projection:?}"
    );
    assert_eq!(
        parse_carve(source).map(|document| serialize_carve(&document)),
        Ok(source.to_owned())
    );
}
