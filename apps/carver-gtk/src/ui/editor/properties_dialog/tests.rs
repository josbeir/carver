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
        multiple: false,
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
    // The title row is fixed and cannot be removed; the note field is a custom row.
    assert!(!drafts[0].editable_key);
    assert!(!drafts[0].editable_kind);
    assert!(!drafts[0].removable);
    assert!(drafts[1].editable_key);
    assert!(drafts[1].editable_kind);
    assert!(drafts[1].removable);
}

#[test]
fn initial_drafts_should_fix_the_type_of_configured_defaults() {
    let request = request(
        Some(FrontmatterDocument {
            format: FrontmatterFormat::Yaml,
            fields: vec![field("author", "Jane")],
            error: None,
        }),
        vec![default_property("author", "Default")],
        true,
    );

    let drafts = initial_drafts(&request);
    let Some(author) = drafts.iter().find(|draft| draft.key == "author") else {
        panic!("expected an author row");
    };

    assert!(!author.editable_key);
    assert!(!author.editable_kind);
    assert!(author.removable);
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
fn initial_drafts_should_carry_list_options_from_configuration() {
    let list_default = DocumentProperty {
        key: "status".to_owned(),
        kind: PropertyKind::List,
        multiline: false,
        multiple: true,
        value: serde_json::json!(["active", "archived"]),
    };
    let request = request(
        Some(FrontmatterDocument {
            format: FrontmatterFormat::Yaml,
            fields: vec![FrontmatterField::new(
                "status",
                FrontmatterValue::List(vec![FrontmatterValue::Text("active".to_owned())]),
            )],
            error: None,
        }),
        vec![list_default],
        true,
    );

    let drafts = initial_drafts(&request);
    let Some(status) = drafts.iter().find(|draft| draft.key == "status") else {
        panic!("expected a status row");
    };

    assert!(status.multiple);
    assert_eq!(status.options, ["active", "archived"]);
    assert!(!status.editable_kind);
}

#[test]
fn build_document_should_skip_an_empty_title() {
    let request = request(None, Vec::new(), false);
    let drafts = initial_drafts(&request);

    let document = build_document(FrontmatterFormat::Yaml, &drafts);

    assert!(document.fields.is_empty());
}
