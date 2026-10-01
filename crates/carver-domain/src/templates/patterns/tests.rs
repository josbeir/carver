use super::*;
use time::macros::datetime;
fn context() -> TemplateContext {
    TemplateContext {
        now: datetime!(2026-10-01 14:30:12 +02:00),
        category: "Meetings".into(),
    }
}
#[test]
fn patterns_should_resolve_from_one_timestamp() -> Result<(), TemplateError> {
    assert_eq!(
        expand_template_source("{{date}} {{time}} {{datetime}} {{category}}", &context())?,
        "2026-10-01 14:30 2026-10-01T14:30:12+02:00 Meetings"
    );
    Ok(())
}
#[test]
fn custom_formats_should_use_standard_tokens() -> Result<(), TemplateError> {
    assert_eq!(
        expand_template_source("{{date:%d/%m/%Y}} {{time:%H:%M:%S}}", &context())?,
        "01/10/2026 14:30:12"
    );
    Ok(())
}
#[test]
fn escaped_patterns_should_remain_literal_without_recursion() -> Result<(), TemplateError> {
    let mut context = context();
    context.category = "{{date}}".into();
    assert_eq!(
        expand_template_source(r"\{{unknown}} {{category}}", &context)?,
        "{{unknown}} {{date}}"
    );
    Ok(())
}
#[test]
fn invalid_patterns_should_be_rejected() {
    for source in ["{{unknown}}", "{{date", "{{date:}}", "{{date:%Q}}"] {
        assert!(
            expand_template_source(source, &context()).is_err(),
            "{source}"
        );
    }
}
#[test]
fn property_patterns_should_preserve_types_and_escape_category_names() -> Result<(), TemplateError>
{
    let mut context = context();
    context.category = "Quotes: \"hello\"\nNext".into();
    for source in [
        "---\nlabel: '{{category}}'\nwhen: '{{date}}'\nflag: true\n---\n# {{date}}",
        "---json\n{\"label\":\"{{category}}\",\"when\":\"{{date}}\",\"flag\":true}\n---\n# {{date}}",
    ] {
        let expanded = expand_template_source(source, &context)?;
        let document = parse_frontmatter_document(&expanded)
            .ok_or_else(|| TemplateError::Frontmatter("missing".into()))?;
        assert!(document.error.is_none());
        assert_eq!(
            document.fields[0].value,
            FrontmatterValue::Text(context.category.clone())
        );
        assert_eq!(document.fields[2].value, FrontmatterValue::Boolean(true));
        assert!(expanded.ends_with("# 2026-10-01"));
    }
    Ok(())
}
#[test]
fn static_templates_should_preserve_exact_source() -> Result<(), TemplateError> {
    let source = "---\nstatus: done\n---\n\n\n# Heading\n";
    assert_eq!(expand_template_source(source, &context())?, source);
    Ok(())
}

#[test]
fn nested_string_patterns_should_resolve_without_changing_keys() -> Result<(), TemplateError> {
    let source =
        "---json\n{\"items\":[\"{{date}}\",{\"name\":\"{{category}}\"}],\"{{date}}\":42}\n---\n";
    let expanded = expand_template_source(source, &context())?;
    let document = parse_frontmatter_document(&expanded)
        .ok_or_else(|| TemplateError::Frontmatter("missing".into()))?;
    assert_eq!(document.fields[1].key, "{{date}}");
    assert_eq!(
        document.fields[0].value.to_json(),
        serde_json::json!(["2026-10-01", {"name":"Meetings"}])
    );
    Ok(())
}
#[test]
fn dynamic_media_paths_should_not_allow_managed_assets() {
    let mut sample = context();
    sample.category = "assets/photo.png".into();
    assert!(matches!(
        expand_template_source("![Image]({{category}})", &sample),
        Err(TemplateError::ManagedAssets)
    ));
}

#[test]
fn dynamic_templates_should_preserve_body_spacing_in_all_frontmatter_formats()
-> Result<(), TemplateError> {
    for header in [
        "---\nwhen: '{{date}}'\n---",
        "---json\n{\"when\":\"{{date}}\"}\n---",
        "---toml\nwhen = '{{date}}'\n---",
    ] {
        let source = format!("{header}\n\n\n\n# {{{{time}}}}\n\n\nTrailing\n");
        let expanded = expand_template_source(&source, &context())?;
        assert!(
            expanded.ends_with("\n\n\n\n# 14:30\n\n\nTrailing\n"),
            "{expanded:?}"
        );
    }
    Ok(())
}
#[test]
fn category_pattern_should_preserve_body_that_looks_like_frontmatter() -> Result<(), TemplateError>
{
    let sample = TemplateContext {
        category: "---\nlabel: text\n---\nBody".into(),
        ..context()
    };
    assert_eq!(
        expand_template_source("{{category}}", &sample)?,
        sample.category
    );
    Ok(())
}
