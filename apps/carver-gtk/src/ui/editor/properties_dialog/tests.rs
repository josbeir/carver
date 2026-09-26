use super::*;

fn field(key: &str, value: &str) -> FrontmatterField {
    FrontmatterField::new(key, FrontmatterValue::Text(value.to_owned()))
}

fn request(
    document: Option<FrontmatterDocument>,
    defaults: Vec<DocumentProperty>,
    defaults_enabled: bool,
) -> EditorPropertiesRequest {
    EditorPropertiesRequest {
        session: crate::mvu::EditorSessionId(1),
        note_id: carver_sdk::NoteId::new(),
        document,
        raw: None,
        defaults,
        defaults_enabled,
    }
}

fn default_property(key: &str, value: &str) -> DocumentProperty {
    DocumentProperty {
        key: key.to_owned(),
        kind: PropertyKind::Text,
        multiline: false,
        value: serde_json::Value::String(value.to_owned()),
    }
}

#[test]
fn initial_drafts_should_list_only_note_properties() {
    let request = request(
        Some(FrontmatterDocument {
            format: FrontmatterFormat::Yaml,
            fields: vec![field("author", "Jane")],
            error: None,
        }),
        vec![default_property("type", "default")],
        true,
    );

    let drafts = initial_drafts(&request);
    let keys: Vec<&str> = drafts.iter().map(|draft| draft.key.as_str()).collect();

    assert_eq!(keys, ["title", "author"]);
    assert!(drafts[0].fixed_key);
    assert!(!drafts[1].fixed_key);
}

#[test]
fn initial_drafts_should_show_an_empty_title_without_frontmatter() {
    let request = request(None, vec![default_property("type", "default")], true);

    let drafts = initial_drafts(&request);

    assert_eq!(drafts.len(), 1);
    assert_eq!(drafts[0].key, "title");
    assert_eq!(drafts[0].value, FrontmatterValue::Text(String::new()));
}

#[test]
fn build_document_should_skip_an_empty_title() {
    let request = request(None, Vec::new(), false);
    let drafts = initial_drafts(&request);

    let document = build_document(FrontmatterFormat::Yaml, &drafts);

    assert!(document.fields.is_empty());
}
