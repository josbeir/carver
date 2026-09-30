use super::*;
use carver_config::{DocumentProperty, DocumentPropertyType};

fn config() -> DocumentPropertiesConfig {
    DocumentPropertiesConfig {
        enabled: true,
        entries: vec![
            DocumentProperty {
                key: "status".into(),
                field_type: DocumentPropertyType::Text,
                multiple: false,
                value: serde_json::json!("open"),
            },
            DocumentProperty {
                key: "author".into(),
                field_type: DocumentPropertyType::Text,
                multiple: false,
                value: serde_json::json!("Jos"),
            },
        ],
        ..DocumentPropertiesConfig::default()
    }
}
#[test]
fn preview_should_explain_template_overrides_and_inherited_defaults() {
    let preview = source_preview("---\nstatus: done\ntype: meeting\n---\n# Agenda", &config())
        .unwrap_or_else(|error| panic!("{error:?}"));
    assert!(preview.properties.iter().any(|p| p.key == "status"
        && p.value == "done"
        && p.origin == TemplatePropertyOrigin::Override("open".into())));
    assert!(preview.properties.iter().any(|p| p.key == "author"
        && p.value == "Jos"
        && p.origin == TemplatePropertyOrigin::Default));
    assert!(
        preview
            .properties
            .iter()
            .any(|p| p.key == "type" && p.origin == TemplatePropertyOrigin::Template)
    );
}
#[test]
fn preview_should_preserve_explicit_empty_overrides() {
    let preview = source_preview("---\nstatus: ''\n---", &config())
        .unwrap_or_else(|error| panic!("{error:?}"));
    let status = preview
        .properties
        .iter()
        .find(|p| p.key == "status")
        .unwrap_or_else(|| panic!("status property"));
    assert_eq!(status.value, "\"\"");
    assert_eq!(
        status.origin,
        TemplatePropertyOrigin::Override("open".into())
    );
}
#[test]
fn preview_should_omit_disabled_defaults() {
    let mut config = config();
    config.enabled = false;
    let preview = source_preview("---\nstatus: done\n---", &config)
        .unwrap_or_else(|error| panic!("{error:?}"));
    assert_eq!(preview.properties.len(), 1);
    assert_eq!(
        preview.properties[0].origin,
        TemplatePropertyOrigin::Template
    );
}
#[test]
fn preview_should_reject_invalid_and_managed_asset_sources() {
    assert!(source_preview("---\nstatus: [\n---", &config()).is_err());
    assert!(source_preview("![Photo](assets/photo.png)", &config()).is_err());
}
