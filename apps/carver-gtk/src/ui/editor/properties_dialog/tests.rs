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
        heading_title: None,
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

    // Title, then the note's authored fields in order, then an appended configured default.
    assert_eq!(keys, ["title", "author", "type"]);
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
        preserve_empty: false,
        derived: false,
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

#[test]
fn type_label_should_stay_plain_while_subtitles_escape_markup() {
    // The drop-down model renders plain text, so its label keeps the raw ampersand.
    assert_eq!(type_label(DocumentPropertyType::DateTime), "Date & time");
    // Row subtitles are Pango markup, so the ampersand must be escaped to render.
    assert_eq!(
        row_subtitle(DocumentPropertyType::DateTime, &FrontmatterValue::Null),
        "Date &amp; time"
    );
}

#[test]
fn type_for_value_should_recover_iso_dates() {
    assert_eq!(
        type_for_value(&FrontmatterValue::Text("2026-09-16".to_owned())),
        DocumentPropertyType::Date
    );
    assert_eq!(
        type_for_value(&FrontmatterValue::Text(
            "2026-09-26T14:22:49+02:00".to_owned()
        )),
        DocumentPropertyType::DateTime
    );
    assert_eq!(
        type_for_value(&FrontmatterValue::Text(
            "2026-09-26T14:22:49.240147874+02:00".to_owned()
        )),
        DocumentPropertyType::DateTime
    );
    assert_eq!(
        type_for_value(&FrontmatterValue::Text("hello world".to_owned())),
        DocumentPropertyType::Text
    );
    assert_eq!(
        type_for_value(&FrontmatterValue::Number(serde_json::Number::from(3))),
        DocumentPropertyType::Number
    );
}

fn draft_with(field_type: DocumentPropertyType, value: FrontmatterValue) -> PropertyDraft {
    let mut draft = PropertyDraft::blank();
    draft.choice = field_type;
    draft.value = value;
    draft
}

#[test]
fn type_for_value_should_cover_every_kind() {
    assert_eq!(
        type_for_value(&FrontmatterValue::Boolean(true)),
        DocumentPropertyType::Boolean
    );
    assert_eq!(
        type_for_value(&FrontmatterValue::List(Vec::new())),
        DocumentPropertyType::List
    );
    assert_eq!(
        type_for_value(&FrontmatterValue::Null),
        DocumentPropertyType::Text
    );
    assert_eq!(
        type_for_value(&FrontmatterValue::Object(Vec::new())),
        DocumentPropertyType::Text
    );
}

#[test]
fn type_index_should_round_trip_every_field_type() {
    for field_type in [
        DocumentPropertyType::Text,
        DocumentPropertyType::LongText,
        DocumentPropertyType::Number,
        DocumentPropertyType::Boolean,
        DocumentPropertyType::List,
        DocumentPropertyType::Date,
        DocumentPropertyType::DateTime,
    ] {
        assert_eq!(type_from_index(type_index(field_type)), field_type);
    }
    assert_eq!(type_from_index(u32::MAX), DocumentPropertyType::Text);
}

#[test]
fn configured_value_support_should_cover_every_field_type() {
    assert!(configured_value_supported(&draft_with(
        DocumentPropertyType::Number,
        FrontmatterValue::Number(serde_json::Number::from(1)),
    )));
    assert!(!configured_value_supported(&draft_with(
        DocumentPropertyType::Number,
        FrontmatterValue::Text("x".to_owned()),
    )));
    assert!(configured_value_supported(&draft_with(
        DocumentPropertyType::Boolean,
        FrontmatterValue::Boolean(true),
    )));
    assert!(!configured_value_supported(&draft_with(
        DocumentPropertyType::Boolean,
        FrontmatterValue::Text("x".to_owned()),
    )));
    assert!(configured_value_supported(&draft_with(
        DocumentPropertyType::Text,
        FrontmatterValue::Null,
    )));
    assert!(configured_value_supported(&draft_with(
        DocumentPropertyType::LongText,
        FrontmatterValue::Text("x".to_owned()),
    )));
    assert!(configured_value_supported(&draft_with(
        DocumentPropertyType::DateTime,
        FrontmatterValue::Text("2026-09-16T12:00:00Z".to_owned()),
    )));
    assert!(!configured_value_supported(&draft_with(
        DocumentPropertyType::DateTime,
        FrontmatterValue::Text("not a date".to_owned()),
    )));
    // A list without configured options stays free-form.
    assert!(configured_value_supported(&draft_with(
        DocumentPropertyType::List,
        FrontmatterValue::Text("x".to_owned()),
    )));
}

#[test]
fn parse_number_should_accept_and_reject_inputs() {
    assert_eq!(parse_number("42"), Some(serde_json::Number::from(42)));
    assert_eq!(parse_number("  -7  "), Some(serde_json::Number::from(-7)));
    assert_eq!(parse_number("1.5"), serde_json::Number::from_f64(1.5));
    assert_eq!(parse_number("   "), None);
    assert_eq!(parse_number("abc"), None);
}

#[test]
fn simple_value_helpers_should_format_values() {
    assert_eq!(frontmatter_text(&FrontmatterValue::Boolean(true)), "true");
    assert_eq!(
        frontmatter_text(&FrontmatterValue::Number(serde_json::Number::from(3))),
        "3"
    );
    assert_eq!(frontmatter_text(&FrontmatterValue::Null), "");
    assert_eq!(
        frontmatter_text(&FrontmatterValue::Object(vec![(
            "k".to_owned(),
            FrontmatterValue::Text("v".to_owned())
        )])),
        "{\"k\":\"v\"}"
    );
    assert_eq!(
        frontmatter_list_text(&FrontmatterValue::List(vec![
            FrontmatterValue::Text("a".to_owned()),
            FrontmatterValue::Text("b".to_owned()),
        ])),
        "a, b"
    );
    assert_eq!(display_title(TITLE_KEY), gettext("Title"));
    assert_eq!(display_title("author"), "author");
    assert_eq!(
        list_value("a, b ,, c"),
        FrontmatterValue::List(vec![
            FrontmatterValue::Text("a".to_owned()),
            FrontmatterValue::Text("b".to_owned()),
            FrontmatterValue::Text("c".to_owned()),
        ])
    );

    let preview = preview_text(&FrontmatterValue::Text("x".repeat(80)));
    assert!(preview.ends_with('…'));
    assert_eq!(preview.chars().count(), 60);
}

#[test]
fn normalized_default_value_should_coerce_mismatches() {
    assert_eq!(
        normalized_default_value(DocumentPropertyType::Number, &serde_json::json!("x")),
        serde_json::json!(0)
    );
    assert_eq!(
        normalized_default_value(DocumentPropertyType::Boolean, &serde_json::json!("x")),
        serde_json::json!(false)
    );
    assert_eq!(
        normalized_default_value(DocumentPropertyType::List, &serde_json::json!("x")),
        serde_json::json!([])
    );
    assert_eq!(
        normalized_default_value(DocumentPropertyType::Text, &serde_json::json!(1)),
        serde_json::json!("")
    );
    assert_eq!(
        normalized_default_value(DocumentPropertyType::Number, &serde_json::json!(3)),
        serde_json::json!(3)
    );
    assert_eq!(
        normalized_default_value(DocumentPropertyType::Boolean, &serde_json::json!(true)),
        serde_json::json!(true)
    );
    assert_eq!(
        normalized_default_value(DocumentPropertyType::List, &serde_json::json!(["a"])),
        serde_json::json!(["a"])
    );
}

#[test]
fn normalized_default_properties_should_drop_blank_reserved_and_duplicate_keys() {
    let draft = |key: &str| {
        let mut draft = PropertyDraft::blank();
        draft.key = key.to_owned();
        draft.value = FrontmatterValue::Text("v".to_owned());
        draft
    };
    let normalized = normalized_default_properties(&[
        draft("title"),
        draft(""),
        draft("author"),
        draft("author"),
    ]);
    assert_eq!(normalized.len(), 1);
    assert_eq!(normalized[0].key, "author");
}

#[test]
fn parse_number_should_accept_an_unsigned_integer() {
    assert_eq!(
        parse_number("18446744073709551615"),
        Some(serde_json::Number::from(u64::MAX))
    );
}

#[test]
fn build_document_should_keep_authored_empty_values_but_drop_unfilled_ones() {
    let authored_empty = authored_draft(PropertyDraft::custom(
        "note".to_owned(),
        FrontmatterValue::Text(String::new()),
    ));
    let authored_null = authored_draft(PropertyDraft::custom(
        "flag".to_owned(),
        FrontmatterValue::Null,
    ));
    let mut unfilled = PropertyDraft::blank();
    unfilled.key = "unfilled".to_owned();
    let mut filled = PropertyDraft::blank();
    filled.key = "filled".to_owned();
    filled.value = FrontmatterValue::Text("x".to_owned());

    let document = build_document(
        FrontmatterFormat::Yaml,
        &[authored_empty, authored_null, unfilled, filled],
    );
    let keys: Vec<&str> = document
        .fields
        .iter()
        .map(|field| field.key.as_str())
        .collect();
    assert_eq!(keys, ["note", "flag", "filled"]);
}

#[test]
fn is_editable_value_should_route_complex_lists_to_raw() {
    assert!(is_editable_value(&FrontmatterValue::Text("x".to_owned())));
    assert!(is_editable_value(&FrontmatterValue::List(vec![
        FrontmatterValue::Text("a".to_owned())
    ])));
    assert!(!is_editable_value(&FrontmatterValue::Object(Vec::new())));
    assert!(!is_editable_value(&FrontmatterValue::List(vec![
        FrontmatterValue::Boolean(true)
    ])));
    assert!(!is_editable_value(&FrontmatterValue::List(vec![
        FrontmatterValue::Text("a,b".to_owned())
    ])));
    assert!(!is_editable_value(&FrontmatterValue::List(vec![
        FrontmatterValue::Object(Vec::new())
    ])));
}

#[test]
fn initial_drafts_should_prefill_the_title_from_a_heading() {
    let mut request = request(None, Vec::new());
    request.heading_title = Some("Meeting".to_owned());

    let drafts = initial_drafts(&request);
    assert_eq!(drafts[0].key, "title");
    assert_eq!(
        drafts[0].value,
        FrontmatterValue::Text("Meeting".to_owned())
    );
    assert!(drafts[0].derived);
}

#[test]
fn initial_drafts_should_not_prefill_over_an_authored_title() {
    let mut request = request(
        Some(FrontmatterDocument {
            format: FrontmatterFormat::Yaml,
            fields: vec![field("title", "Authored")],
            error: None,
        }),
        Vec::new(),
    );
    request.heading_title = Some("Meeting".to_owned());

    let drafts = initial_drafts(&request);
    let Some(title) = drafts.iter().find(|draft| draft.key == "title") else {
        panic!("expected a title row");
    };
    assert_eq!(title.value, FrontmatterValue::Text("Authored".to_owned()));
    assert!(!title.derived);
}

#[test]
fn build_document_should_skip_a_derived_title_until_edited() {
    let mut title = PropertyDraft::title(FrontmatterValue::Text("Meeting".to_owned()));
    title.derived = true;
    assert!(
        build_document(FrontmatterFormat::Yaml, &[title.clone()])
            .fields
            .is_empty()
    );

    title.derived = false;
    let document = build_document(FrontmatterFormat::Yaml, &[title]);
    assert_eq!(document.fields.len(), 1);
    assert_eq!(document.fields[0].key, "title");
}

#[test]
fn initial_drafts_should_put_an_authored_title_first() {
    let request = request(
        Some(FrontmatterDocument {
            format: FrontmatterFormat::Yaml,
            fields: vec![field("author", "Jane"), field("title", "Meeting")],
            error: None,
        }),
        Vec::new(),
    );

    let drafts = initial_drafts(&request);
    assert_eq!(drafts[0].key, "title");
    assert_eq!(
        drafts[0].value,
        FrontmatterValue::Text("Meeting".to_owned())
    );
    assert_eq!(drafts[1].key, "author");
}
