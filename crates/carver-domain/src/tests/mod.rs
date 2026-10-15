use super::*;

#[test]
fn importing_carve_preserves_the_source() {
    let source = "# Keep\n\nA /Carve/ document.\n";

    assert_eq!(import_document(source, DocumentImportFormat::Carve), source);
}

#[test]
fn importing_markdown_uses_the_native_carve_migration() {
    let source = "---\ntitle: Imported\n---\n\n# Heading\n\n- [x] Done\n";

    let converted = import_document(source, DocumentImportFormat::Markdown);

    assert!(converted.contains("# Heading"));
    assert!(converted.contains("- [x] Done"));
}

#[test]
fn assessing_markdown_should_expose_verified_strong_fidelity() {
    let result = assess_import("**strong**", DocumentImportFormat::Markdown);
    assert_eq!(result.report.schema_version, 2);
    assert_eq!(result.report.source_format, "markdown");
    let diagnostic = result
        .report
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code == "markdown-strong")
        .unwrap_or_else(|| panic!("Markdown import should assess strong formatting"));
    assert_eq!(diagnostic.fidelity, "preserved");
    assert_eq!(diagnostic.confidence, "exact");
    assert_eq!(diagnostic.path.as_deref(), Some("line:1"));
}

#[test]
fn derives_title_from_first_heading() {
    let content = derive_content("# A /useful/ note\n\nBody text");
    assert_eq!(content.title, "A useful note");
    assert!(content.plain_text.contains("Body text"));
}

#[test]
fn derives_title_from_yaml_frontmatter_before_a_heading() {
    assert_eq!(
        derive_content("---\ntitle: From metadata\n---\n\n# From heading").title,
        "From metadata"
    );
}

#[test]
fn derives_title_from_json_frontmatter_before_a_heading() {
    assert_eq!(
        derive_content("---json\n{\"title\":\"From metadata\"}\n---\n\n# From heading").title,
        "From metadata"
    );
}

#[test]
fn derives_title_from_toml_frontmatter_before_a_heading() {
    assert_eq!(
        derive_content("---toml\ntitle = \"From metadata\"\n---\n\n# From heading").title,
        "From metadata"
    );
}

#[test]
fn derives_title_from_first_text_when_heading_is_missing() {
    assert_eq!(
        derive_content("\n\nHello world\n\nMore").title,
        "Hello world"
    );
}

#[test]
fn gives_empty_notes_a_safe_title() {
    assert_eq!(derive_content(" ").title, "Untitled Note");
}

#[test]
fn automatic_category_color_should_be_stable_for_one_category() {
    let category_id = CategoryId::new();

    assert_eq!(
        CategoryColor::Auto.resolved_for(category_id),
        CategoryColor::Auto.resolved_for(category_id)
    );
}
