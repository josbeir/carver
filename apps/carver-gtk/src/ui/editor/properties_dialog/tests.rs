use super::*;

fn field(key: &str, value: &str) -> FrontmatterField {
    FrontmatterField::new(key, FrontmatterValue::Text(value.to_owned()))
}

fn request(
    document: Option<FrontmatterDocument>,
    defaults: Vec<DocumentProperty>,
) -> EditorPropertiesRequest {
    EditorPropertiesRequest {
        session: crate::mvu::EditorSessionId(1),
        note_id: carver_sdk::NoteId::new(),
        document,
        raw: None,
        defaults,
        default_format: FrontmatterFormat::Yaml,
    }
}

fn default_property(key: &str, value: &str) -> DocumentProperty {
    DocumentProperty {
        key: key.to_owned(),
        field_type: DocumentPropertyType::Text,
        multiple: false,
        value: serde_json::Value::String(value.to_owned()),
    }
}

#[test]
fn initial_drafts_should_always_offer_configured_defaults() {
    let request = request(
        Some(FrontmatterDocument {
            format: FrontmatterFormat::Yaml,
            fields: vec![field("author", "Jane")],
            error: None,
        }),
        vec![default_property("type", "default")],
    );

    let drafts = initial_drafts(&request);
    let keys: Vec<&str> = drafts.iter().map(|draft| draft.key.as_str()).collect();

    // Title, then the configured default, then the note's own custom property.
    assert_eq!(keys, ["title", "type", "author"]);
    assert!(!drafts[0].editable_key);
    assert!(!drafts[0].removable);
    let Some(kind) = drafts.iter().find(|draft| draft.key == "type") else {
        panic!("expected a type row");
    };
    assert!(!kind.editable_key);
    assert!(!kind.editable_kind);
    assert!(!kind.removable);
    assert_eq!(kind.value, FrontmatterValue::Text("default".to_owned()));
    let Some(author) = drafts.iter().find(|draft| draft.key == "author") else {
        panic!("expected an author row");
    };
    assert!(author.editable_key);
    assert!(author.editable_kind);
    assert!(author.removable);
}

#[test]
fn initial_drafts_should_take_the_note_value_for_a_configured_default() {
    let request = request(
        Some(FrontmatterDocument {
            format: FrontmatterFormat::Yaml,
            fields: vec![field("author", "Jane")],
            error: None,
        }),
        vec![default_property("author", "Default")],
    );

    let drafts = initial_drafts(&request);
    let Some(author) = drafts.iter().find(|draft| draft.key == "author") else {
        panic!("expected an author row");
    };

    assert_eq!(author.value, FrontmatterValue::Text("Jane".to_owned()));
    assert!(!author.editable_kind);
    assert!(!author.removable);
}

#[test]
fn initial_drafts_should_show_an_empty_title_without_frontmatter() {
    let request = request(None, Vec::new());

    let drafts = initial_drafts(&request);

    assert_eq!(drafts.len(), 1);
    assert_eq!(drafts[0].key, "title");
    assert_eq!(drafts[0].value, FrontmatterValue::Text(String::new()));
}

#[test]
fn initial_drafts_should_carry_list_options_from_configuration() {
    let list_default = DocumentProperty {
        key: "status".to_owned(),
        field_type: DocumentPropertyType::List,
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
    );

    let drafts = initial_drafts(&request);
    let Some(status) = drafts.iter().find(|draft| draft.key == "status") else {
        panic!("expected a status row");
    };

    assert!(status.multiple);
    assert_eq!(status.options, ["active", "archived"]);
    assert!(!status.editable_kind);
    assert!(!status.removable);
}

#[test]
fn build_document_should_skip_an_empty_title() {
    let request = request(None, Vec::new());
    let drafts = initial_drafts(&request);

    let document = build_document(FrontmatterFormat::Yaml, &drafts);

    assert!(document.fields.is_empty());
}

#[test]
fn configured_value_support_should_require_matching_type_and_options() {
    // A text default with a numeric note value is not controllable.
    let text = PropertyDraft::default_row(
        "author".to_owned(),
        &default_property("author", "Jane"),
        FrontmatterValue::Number(serde_json::Number::from(3)),
    );
    assert!(!configured_value_supported(&text));

    let list_default = DocumentProperty {
        key: "status".to_owned(),
        field_type: DocumentPropertyType::List,
        multiple: false,
        value: serde_json::json!(["active", "archived"]),
    };
    let single = |value: FrontmatterValue| PropertyDraft {
        key: "status".to_owned(),
        choice: DocumentPropertyType::List,
        value,
        editable_key: false,
        editable_kind: false,
        removable: false,
        multiple: false,
        options: list_default.options(),
    };
    assert!(configured_value_supported(&single(FrontmatterValue::Text(
        "active".to_owned()
    ))));
    assert!(!configured_value_supported(&single(
        FrontmatterValue::Text("other".to_owned())
    )));
    assert!(!configured_value_supported(&single(
        FrontmatterValue::List(vec![FrontmatterValue::Text("active".to_owned())])
    )));
}

#[test]
fn date_field_support_should_require_an_iso_value() {
    let date_property = DocumentProperty {
        key: "due".to_owned(),
        field_type: DocumentPropertyType::Date,
        multiple: false,
        value: serde_json::Value::String(String::new()),
    };
    let date = PropertyDraft::default_row(
        "due".to_owned(),
        &date_property,
        FrontmatterValue::Text("2024-01-15".to_owned()),
    );
    assert!(configured_value_supported(&date));

    let invalid = PropertyDraft::default_row(
        "due".to_owned(),
        &date_property,
        FrontmatterValue::Text("not a date".to_owned()),
    );
    assert!(!configured_value_supported(&invalid));

    let date_time_property = DocumentProperty {
        key: "at".to_owned(),
        field_type: DocumentPropertyType::DateTime,
        multiple: false,
        value: serde_json::Value::String(String::new()),
    };
    let date_time = PropertyDraft::default_row(
        "at".to_owned(),
        &date_time_property,
        FrontmatterValue::Text("2024-01-15T10:30:00Z".to_owned()),
    );
    assert!(configured_value_supported(&date_time));
}
