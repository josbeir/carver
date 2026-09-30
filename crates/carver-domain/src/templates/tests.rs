use super::*;
#[test]
fn merge_should_preserve_body_and_override_defaults() {
    let source = "---\nstatus: planned\n---\n\n\n# Heading\n\n";
    let fields = vec![
        FrontmatterField::new("status", FrontmatterValue::Text("draft".into())),
        FrontmatterField::new("author", FrontmatterValue::Text("Jos".into())),
    ];
    let merged = merge_template_source(source, &fields, FrontmatterFormat::Json)
        .unwrap_or_else(|error| panic!("{error:?}"));
    assert!(merged.ends_with("---\n\n\n# Heading\n\n"));
    assert_eq!(
        parse_frontmatter_document(&merged)
            .unwrap_or_else(|| panic!("frontmatter missing"))
            .fields[0]
            .value,
        FrontmatterValue::Text("planned".into())
    );
}
#[test]
fn merge_should_preserve_source_when_defaults_are_disabled() {
    let source = "---\nstatus: false\n---\n\n# Draft\n";
    assert_eq!(
        merge_template_source(source, &[], FrontmatterFormat::Yaml)
            .unwrap_or_else(|error| panic!("{error:?}")),
        source
    );
}
#[test]
fn template_should_reject_duplicate_properties() {
    assert!(validate_template_source("---json\n{\"x\":1,\"x\":2}\n---\n").is_err());
}
#[test]
fn template_should_reject_managed_assets() {
    assert!(matches!(
        validate_template_source("![Image](assets/photo.png)"),
        Err(TemplateError::ManagedAssets)
    ));
}

#[test]
fn merge_should_keep_template_format_and_body_for_all_frontmatter_formats() {
    for (source, format) in [
        (
            "---\nstatus: planned\n---\n\n\n::: raw\n<custom>\n:::\n",
            FrontmatterFormat::Yaml,
        ),
        (
            "---json\n{\"status\":\"planned\"}\n---\n\n\n::: raw\n<custom>\n:::\n",
            FrontmatterFormat::Json,
        ),
        (
            "---toml\nstatus = \"planned\"\n---\n\n\n::: raw\n<custom>\n:::\n",
            FrontmatterFormat::Toml,
        ),
    ] {
        let fields = vec![FrontmatterField::new(
            "author",
            FrontmatterValue::Text("Jos".into()),
        )];
        let merged = merge_template_source(source, &fields, FrontmatterFormat::Yaml)
            .unwrap_or_else(|e| panic!("{e}"));
        assert!(merged.ends_with("---\n\n\n::: raw\n<custom>\n:::\n"));
        assert_eq!(
            parse_frontmatter_document(&merged).map(|d| d.format),
            Some(format)
        );
    }
}
#[test]
fn merge_should_keep_explicit_empty_values_instead_of_defaults() {
    let source =
        "---json\n{\"text\":\"\",\"list\":[],\"flag\":false,\"number\":0,\"empty\":null}\n---\n\n";
    let fields = ["text", "list", "flag", "number", "empty"]
        .map(|key| FrontmatterField::new(key, FrontmatterValue::Text("default".into())));
    assert_eq!(
        merge_template_source(source, &fields, FrontmatterFormat::Yaml)
            .unwrap_or_else(|e| panic!("{e}")),
        source
    );
}
#[test]
fn merge_should_preserve_leading_body_blank_lines_when_adding_frontmatter() {
    let fields = [FrontmatterField::new(
        "author",
        FrontmatterValue::Text("Jos".into()),
    )];
    let merged = merge_template_source("\n\n# Heading\n", &fields, FrontmatterFormat::Json)
        .unwrap_or_else(|e| panic!("{e}"));
    assert!(merged.starts_with("---json\n"));
    assert!(merged.ends_with("---\n\n\n\n# Heading\n"));
}
#[test]
fn templates_should_allow_external_and_note_links_but_reject_attachments() {
    assert!(
        validate_template_source("[Website](https://example.com)\n\n[Note](carver://note/example)")
            .is_ok()
    );
    assert!(matches!(
        validate_template_source("[File](assets/file.pdf)"),
        Err(TemplateError::ManagedAssets)
    ));
    assert!(validate_template_source("---json\n{\"nested\": {\"x\":1,\"x\":2}}\n---").is_err());
}
