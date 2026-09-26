use super::*;

fn yaml_document(source: &str) -> FrontmatterDocument {
    match parse_frontmatter_document(source) {
        Some(document) => document,
        None => panic!("expected a frontmatter block"),
    }
}

fn rendered(document: &FrontmatterDocument) -> String {
    match render_frontmatter_document(document) {
        Ok(block) => block,
        Err(error) => panic!("expected a rendered block: {error}"),
    }
}

fn replaced(source: &str, desired: Option<&FrontmatterDocument>) -> String {
    match replace_frontmatter(source, desired) {
        Ok(updated) => updated,
        Err(error) => panic!("expected a replacement: {error}"),
    }
}

#[test]
fn parse_should_preserve_yaml_order_and_types() {
    let document = yaml_document(
        "---\ntitle: Hello\nauthor: Jane\ntags: [a, b]\ndraft: true\ncount: 3\n---\n\nBody",
    );
    assert_eq!(document.format, FrontmatterFormat::Yaml);
    assert!(document.error.is_none());
    let keys: Vec<&str> = document
        .fields
        .iter()
        .map(|field| field.key.as_str())
        .collect();
    assert_eq!(keys, ["title", "author", "tags", "draft", "count"]);
    assert_eq!(
        document.fields[1].value,
        FrontmatterValue::Text("Jane".to_owned())
    );
    assert_eq!(document.fields[3].value, FrontmatterValue::Boolean(true));
    assert!(matches!(
        document.fields[4].value,
        FrontmatterValue::Number(_)
    ));
    assert_eq!(document.fields[2].kind(), PropertyKind::List);
}

#[test]
fn parse_should_preserve_json_order() {
    let document =
        yaml_document("---json\n{\n  \"title\": \"Hello\",\n  \"author\": \"Jane\"\n}\n---\n");
    assert_eq!(document.format, FrontmatterFormat::Json);
    let keys: Vec<&str> = document
        .fields
        .iter()
        .map(|field| field.key.as_str())
        .collect();
    assert_eq!(keys, ["title", "author"]);
}

#[test]
fn parse_should_read_toml() {
    let document = yaml_document("---toml\nauthor = \"Jane\"\ncount = 3\n---\n");
    assert_eq!(document.format, FrontmatterFormat::Toml);
    let keys: Vec<&str> = document
        .fields
        .iter()
        .map(|field| field.key.as_str())
        .collect();
    assert_eq!(keys, ["author", "count"]);
}

#[test]
fn parse_should_report_malformed_and_non_object_roots() {
    let malformed = yaml_document("---yaml\nkey: [\n---\n");
    assert!(malformed.error.is_some());
    let scalar = yaml_document("---yaml\n- one\n- two\n---\n");
    assert!(scalar.error.is_some());
}

#[test]
fn render_should_round_trip_yaml_order() {
    let document = yaml_document("---\nb: two\na: one\n---\n");
    let block = rendered(&document);
    assert_eq!(block, "---\nb: two\na: one\n---");
    let reparsed = yaml_document(&format!("{block}\n"));
    assert_eq!(reparsed.fields, document.fields);
}

#[test]
fn scalar_edit_should_preserve_untouched_lines() {
    let source = "---\nauthor: Jane\ntags: [a, b]\n# keep me\nmonth: may\n---\n\nBody";
    let mut desired = yaml_document(source);
    desired.fields[0].value = FrontmatterValue::Text("John".to_owned());
    assert_eq!(
        replaced(source, Some(&desired)),
        "---\nauthor: \"John\"\ntags: [a, b]\n# keep me\nmonth: may\n---\n\nBody"
    );
}

#[test]
fn unchanged_document_should_not_be_reformatted() {
    let source = "---\nauthor:    Jane\ntags: [a, b]\n---\n\nBody";
    let desired = yaml_document(source);
    assert_eq!(replaced(source, Some(&desired)), source);
}

#[test]
fn add_and_remove_should_edit_lines() {
    let source = "---\nauthor: Jane\ntags: [a, b]\n---\nBody";
    let mut desired = yaml_document(source);
    desired.fields.retain(|field| field.key != "tags");
    desired.fields.push(FrontmatterField::new(
        "project",
        FrontmatterValue::Text("Carver".to_owned()),
    ));
    assert_eq!(
        replaced(source, Some(&desired)),
        "---\nauthor: Jane\nproject: \"Carver\"\n---\n\nBody"
    );
}

#[test]
fn reorder_should_full_render() {
    let source = "---\na: one\nb: two\n---\nBody";
    let desired = FrontmatterDocument {
        format: FrontmatterFormat::Yaml,
        fields: vec![
            FrontmatterField::new("b", FrontmatterValue::Text("two".to_owned())),
            FrontmatterField::new("a", FrontmatterValue::Text("one".to_owned())),
        ],
        error: None,
    };
    assert_eq!(
        replaced(source, Some(&desired)),
        "---\nb: two\na: one\n---\n\nBody"
    );
}

#[test]
fn format_switch_should_render_new_fences() {
    let source = "---\nauthor: Jane\n---\nBody";
    let mut desired = yaml_document(source);
    desired.format = FrontmatterFormat::Toml;
    assert_eq!(
        replaced(source, Some(&desired)),
        "---toml\nauthor = \"Jane\"\n---\n\nBody"
    );
}

#[test]
fn remove_should_strip_the_block() {
    assert_eq!(replaced("---\na: b\n---\nBody", None), "Body");
}

#[test]
fn insert_should_prepend_a_block() {
    let desired = FrontmatterDocument {
        format: FrontmatterFormat::Yaml,
        fields: vec![FrontmatterField::new(
            "author",
            FrontmatterValue::Text("Jane".to_owned()),
        )],
        error: None,
    };
    assert_eq!(
        replaced("Body", Some(&desired)),
        "---\nauthor: Jane\n---\n\nBody"
    );
}

#[test]
fn frontmatter_source_should_drop_reserved_and_blank_keys() {
    let fields = vec![
        FrontmatterField::new("title", FrontmatterValue::Text("Ignored".to_owned())),
        FrontmatterField::new("", FrontmatterValue::Text("Ignored".to_owned())),
        FrontmatterField::new("author", FrontmatterValue::Text("Jane".to_owned())),
    ];
    assert_eq!(frontmatter_source(&fields), "---\nauthor: Jane\n---\n");
    assert_eq!(frontmatter_source(&[]), "");
}

#[test]
fn frontmatter_source_should_quote_values_needing_it() {
    let fields = vec![FrontmatterField::new(
        "summary",
        FrontmatterValue::Text("a: b".to_owned()),
    )];
    let source = frontmatter_source(&fields);
    assert!(source.contains("summary:"));
    let document = yaml_document(&source);
    assert_eq!(
        document.fields[0].value,
        FrontmatterValue::Text("a: b".to_owned())
    );
}

#[test]
fn frontmatter_source_seed_should_drive_derived_title_from_heading() {
    let source = format!(
        "{}# Meeting",
        frontmatter_source(&[FrontmatterField::new(
            "author",
            FrontmatterValue::Text("Jane".to_owned()),
        )])
    );
    assert_eq!(crate::derive_content(&source).title, "Meeting");
}

#[test]
fn number_and_boolean_should_survive_yaml_round_trip() {
    let fields = vec![
        FrontmatterField::new(
            "count",
            FrontmatterValue::Number(serde_json::Number::from(3)),
        ),
        FrontmatterField::new("draft", FrontmatterValue::Boolean(false)),
    ];
    let source = frontmatter_source(&fields);
    let document = yaml_document(&source);
    assert_eq!(
        document.fields[0].value,
        FrontmatterValue::Number(serde_json::Number::from(3))
    );
    assert_eq!(document.fields[1].value, FrontmatterValue::Boolean(false));
}

#[test]
fn raw_replacement_should_keep_the_body_and_switch_format() {
    assert_eq!(
        replace_frontmatter_raw(
            "---\na: b\n---\nBody",
            FrontmatterFormat::Json,
            "{\"x\": 1}"
        ),
        "---json\n{\"x\": 1}\n---\n\nBody"
    );
    assert_eq!(
        replace_frontmatter_raw("Body", FrontmatterFormat::Yaml, "a: b"),
        "---\na: b\n---\n\nBody"
    );
    assert_eq!(
        replace_frontmatter_raw("\n\nBody", FrontmatterFormat::Yaml, "a: b"),
        "---\na: b\n---\n\nBody"
    );
    assert_eq!(
        replace_frontmatter_raw("---\na: b\n---\nBody", FrontmatterFormat::Yaml, "  "),
        "Body"
    );
}

#[test]
fn empty_fields_should_remove_or_leave_the_block_empty() {
    let empty = FrontmatterDocument {
        format: FrontmatterFormat::Yaml,
        fields: Vec::new(),
        error: None,
    };
    assert_eq!(replaced("---\na: b\n---\nBody", Some(&empty)), "Body");
    assert_eq!(replaced("Body", Some(&empty)), "Body");
}

#[test]
fn scalar_edit_should_preserve_adjacent_comments() {
    let source = "---\na: one\n# important\nb: two\n---\nBody";
    let mut desired = yaml_document(source);
    desired.fields[0].value = FrontmatterValue::Text("changed".to_owned());
    assert_eq!(
        replaced(source, Some(&desired)),
        "---\na: \"changed\"\n# important\nb: two\n---\n\nBody"
    );
}

#[test]
fn format_tokens_should_match_their_fences() {
    assert_eq!(FrontmatterFormat::Yaml.as_str(), "yaml");
    assert_eq!(FrontmatterFormat::Json.as_str(), "json");
    assert_eq!(FrontmatterFormat::Toml.as_str(), "toml");
    assert_eq!(FrontmatterFormat::Yaml.opener(), "---");
    assert_eq!(FrontmatterFormat::Json.opener(), "---json");
    assert_eq!(FrontmatterFormat::Toml.opener(), "---toml");
    assert_eq!(
        FrontmatterFormat::from_token("yml"),
        Some(FrontmatterFormat::Yaml)
    );
    assert_eq!(FrontmatterFormat::from_token("unknown"), None);
}

#[test]
fn value_should_map_kinds_and_round_trip_json() {
    assert_eq!(
        FrontmatterValue::Text(String::new()).kind(),
        PropertyKind::Text
    );
    assert_eq!(
        FrontmatterValue::Number(serde_json::Number::from(1)).kind(),
        PropertyKind::Number
    );
    assert_eq!(
        FrontmatterValue::Boolean(false).kind(),
        PropertyKind::Boolean
    );
    assert_eq!(
        FrontmatterValue::List(Vec::new()).kind(),
        PropertyKind::List
    );
    assert_eq!(
        FrontmatterValue::Object(Vec::new()).kind(),
        PropertyKind::Mixed
    );
    assert_eq!(FrontmatterValue::Null.kind(), PropertyKind::Null);

    for value in [
        FrontmatterValue::Text("x".to_owned()),
        FrontmatterValue::Number(serde_json::Number::from(3)),
        FrontmatterValue::Boolean(true),
        FrontmatterValue::List(vec![
            FrontmatterValue::Text("a".to_owned()),
            FrontmatterValue::Null,
        ]),
        FrontmatterValue::Object(vec![("k".to_owned(), FrontmatterValue::Boolean(false))]),
        FrontmatterValue::Null,
    ] {
        assert_eq!(FrontmatterValue::from_json(&value.to_json()), value);
    }
}

#[test]
fn value_should_round_trip_through_serde() {
    let half = serde_json::Number::from_f64(1.5).unwrap_or_else(|| serde_json::Number::from(0));
    let value = FrontmatterValue::Object(vec![
        ("text".to_owned(), FrontmatterValue::Text("hi".to_owned())),
        ("float".to_owned(), FrontmatterValue::Number(half.clone())),
        (
            "list".to_owned(),
            FrontmatterValue::List(vec![
                FrontmatterValue::Null,
                FrontmatterValue::Boolean(true),
            ]),
        ),
    ]);
    let json = serde_json::to_string(&value).unwrap_or_default();
    let parsed: FrontmatterValue = serde_json::from_str(&json).unwrap_or(FrontmatterValue::Null);
    assert_eq!(parsed, value);

    let owned: FrontmatterValue =
        serde_json::from_value(serde_json::json!({"s": "x", "i": 2, "b": false, "z": null}))
            .unwrap_or(FrontmatterValue::Null);
    assert_eq!(owned.kind(), PropertyKind::Mixed);

    let float: FrontmatterValue = serde_json::from_str("1.5").unwrap_or(FrontmatterValue::Null);
    assert_eq!(float, FrontmatterValue::Number(half));
}

#[test]
fn key_and_inline_helpers_should_cover_edges() {
    assert!(needs_quoted_key("", FrontmatterFormat::Yaml));
    assert!(needs_quoted_key("a b", FrontmatterFormat::Toml));
    assert!(needs_quoted_key("a.b", FrontmatterFormat::Toml));
    assert!(!needs_quoted_key("a-b_1.x", FrontmatterFormat::Yaml));
    assert!(!needs_quoted_key("a-b_1", FrontmatterFormat::Toml));

    assert_eq!(unquote_key("\"quoted\""), "quoted");
    assert_eq!(unquote_key("'single'"), "single");
    assert_eq!(unquote_key("plain"), "plain");

    assert_eq!(top_level_key("# comment", FrontmatterFormat::Yaml), None);
    assert_eq!(top_level_key("   ", FrontmatterFormat::Yaml), None);
    assert_eq!(top_level_key(": value", FrontmatterFormat::Yaml), None);
    assert_eq!(
        top_level_key("key: value", FrontmatterFormat::Yaml),
        Some("key".to_owned())
    );
    assert_eq!(
        top_level_key("key = 1", FrontmatterFormat::Toml),
        Some("key".to_owned())
    );

    assert_eq!(
        escape_double_quoted("a\"b\\c\nd\re\tf"),
        "a\\\"b\\\\c\\nd\\re\\tf"
    );

    assert_eq!(
        render_inline_value(
            FrontmatterFormat::Yaml,
            &FrontmatterValue::Number(serde_json::Number::from(3))
        ),
        Some("3".to_owned())
    );
    assert_eq!(
        render_inline_value(FrontmatterFormat::Yaml, &FrontmatterValue::Boolean(true)),
        Some("true".to_owned())
    );
    assert_eq!(
        render_inline_value(FrontmatterFormat::Yaml, &FrontmatterValue::Null),
        Some("null".to_owned())
    );
    assert_eq!(
        render_inline_value(FrontmatterFormat::Toml, &FrontmatterValue::Null),
        None
    );
    assert_eq!(
        render_inline_value(
            FrontmatterFormat::Yaml,
            &FrontmatterValue::List(vec![FrontmatterValue::Text("a".to_owned())])
        ),
        Some("[\"a\"]".to_owned())
    );
    assert_eq!(
        render_inline_value(
            FrontmatterFormat::Toml,
            &FrontmatterValue::Object(vec![("k".to_owned(), FrontmatterValue::Boolean(false))])
        ),
        Some("{ k = false }".to_owned())
    );

    assert_eq!(
        render_field_line(
            FrontmatterFormat::Yaml,
            &FrontmatterField::new("a b", FrontmatterValue::Text("x".to_owned()))
        ),
        Some("\"a b\": \"x\"".to_owned())
    );
    assert_eq!(
        render_field_line(
            FrontmatterFormat::Toml,
            &FrontmatterField::new("k", FrontmatterValue::Number(serde_json::Number::from(1)))
        ),
        Some("k = 1".to_owned())
    );
    assert_eq!(
        render_field_line(
            FrontmatterFormat::Toml,
            &FrontmatterField::new("k", FrontmatterValue::Null)
        ),
        None
    );
}

#[test]
fn toml_rendering_should_drop_null_fields() {
    let document = FrontmatterDocument {
        format: FrontmatterFormat::Toml,
        fields: vec![
            FrontmatterField::new("keep", FrontmatterValue::Text("x".to_owned())),
            FrontmatterField::new("drop", FrontmatterValue::Null),
        ],
        error: None,
    };
    let block = rendered(&document);
    assert!(block.contains("keep"));
    assert!(!block.contains("drop"));
}

#[test]
fn insert_without_body_should_end_at_the_block() {
    let desired = FrontmatterDocument {
        format: FrontmatterFormat::Yaml,
        fields: vec![FrontmatterField::new(
            "author",
            FrontmatterValue::Text("Jane".to_owned()),
        )],
        error: None,
    };
    assert_eq!(replaced("", Some(&desired)), "---\nauthor: Jane\n---\n");
    assert_eq!(replaced("\n\n", Some(&desired)), "---\nauthor: Jane\n---\n");
}

#[test]
fn scan_should_accept_a_block_without_a_trailing_newline() {
    assert_eq!(replaced("---\na: b\n---", None), "");
    assert_eq!(
        replace_frontmatter_raw("---\na: b\n---", FrontmatterFormat::Yaml, "c: d"),
        "---\nc: d\n---\n"
    );
}

#[test]
fn json_edits_should_re_render_the_block() {
    let source = "---json\n{\n  \"a\": 1,\n  \"b\": 2\n}\n---\nBody";
    let mut desired = yaml_document(source);
    desired.fields[0].value = FrontmatterValue::Number(serde_json::Number::from(5));
    let updated = replaced(source, Some(&desired));
    assert!(updated.starts_with("---json\n"));
    assert!(updated.contains("\"a\": 5"));
    assert!(updated.ends_with("---\n\nBody"));
}

#[test]
fn toml_scalar_edit_should_preserve_untouched_lines() {
    let source = "---toml\na = \"one\"\nb = 2\n---\nBody";
    let mut desired = yaml_document(source);
    desired.fields[0].value = FrontmatterValue::Text("two".to_owned());
    assert_eq!(
        replaced(source, Some(&desired)),
        "---toml\na = \"two\"\nb = 2\n---\n\nBody"
    );
}

#[test]
fn edits_should_keep_leading_comments_and_append_typed_fields() {
    let source = "---\n# lead\na: one\n\nb: two\n---\nBody";
    let mut desired = yaml_document(source);
    desired.fields[1].value = FrontmatterValue::Text("changed".to_owned());
    desired.fields.push(FrontmatterField::new(
        "count",
        FrontmatterValue::Number(serde_json::Number::from(3)),
    ));
    desired.fields.push(FrontmatterField::new(
        "draft",
        FrontmatterValue::Boolean(true),
    ));
    desired.fields.push(FrontmatterField::new(
        "tags",
        FrontmatterValue::List(vec![FrontmatterValue::Text("x".to_owned())]),
    ));
    desired.fields.push(FrontmatterField::new(
        "meta",
        FrontmatterValue::Object(vec![(
            "k".to_owned(),
            FrontmatterValue::Text("v".to_owned()),
        )]),
    ));
    let updated = replaced(source, Some(&desired));
    assert!(updated.contains("# lead"));
    assert!(updated.contains("b: \"changed\""));
    assert!(updated.contains("count: 3"));
    assert!(updated.contains("draft: true"));
    assert!(updated.contains("tags: [\"x\"]"));
    assert!(updated.contains("meta: {k: \"v\"}"));
}

#[test]
fn edits_should_append_to_a_comment_only_block() {
    let source = "---\n# only comment\n---\nBody";
    let desired = FrontmatterDocument {
        format: FrontmatterFormat::Yaml,
        fields: vec![FrontmatterField::new(
            "a",
            FrontmatterValue::Text("b".to_owned()),
        )],
        error: None,
    };
    assert_eq!(
        replaced(source, Some(&desired)),
        "---\n# only comment\na: \"b\"\n---\n\nBody"
    );
}

#[test]
fn unsupported_format_should_report_an_error() {
    let Some(document) = parse_frontmatter_document("---xml\n<a/>\n---\n") else {
        panic!("expected a frontmatter block");
    };
    assert!(document.error.is_some());
}

#[test]
fn replace_should_handle_an_empty_trailing_body() {
    let desired = FrontmatterDocument {
        format: FrontmatterFormat::Yaml,
        fields: vec![FrontmatterField::new(
            "a",
            FrontmatterValue::Text("c".to_owned()),
        )],
        error: None,
    };
    assert_eq!(
        replaced("---\na: b\n---", Some(&desired)),
        "---\na: \"c\"\n---\n"
    );
}

#[test]
fn inline_object_keys_should_be_quoted_when_needed() {
    assert_eq!(
        render_inline_value(
            FrontmatterFormat::Yaml,
            &FrontmatterValue::Object(vec![(
                "a b".to_owned(),
                FrontmatterValue::Text("v".to_owned())
            )])
        ),
        Some("{\"a b\": \"v\"}".to_owned())
    );
}

#[test]
fn removing_a_field_should_keep_trailing_comments() {
    let source = "---\na: one\n# explanation for b\nb: two\n---\nBody";
    let mut desired = yaml_document(source);
    desired.fields.retain(|field| field.key != "a");
    assert_eq!(
        replaced(source, Some(&desired)),
        "---\n# explanation for b\nb: two\n---\n\nBody"
    );
}

#[test]
fn raw_replacement_should_preserve_the_authored_fence() {
    // Unchanged content is a no-op and an explicit `---yaml` fence is not normalized.
    assert_eq!(
        replace_frontmatter_raw("---yaml\na: b\n---\nBody", FrontmatterFormat::Yaml, "a: b"),
        "---yaml\na: b\n---\nBody"
    );
    assert_eq!(
        replace_frontmatter_raw("---yaml\na: b\n---\nBody", FrontmatterFormat::Yaml, "a: c"),
        "---yaml\na: c\n---\n\nBody"
    );
}

#[test]
fn title_should_lead_the_block_even_when_authored_last() {
    let source = "---\nauthor: Jane\n# keep\nstatus: draft\ntitle: Meeting\n---\nBody";
    let mut desired = yaml_document(source);
    if let Some(index) = desired.fields.iter().position(|field| field.key == "title") {
        let title = desired.fields.remove(index);
        desired.fields.insert(0, title);
    }
    assert_eq!(
        replaced(source, Some(&desired)),
        "---\ntitle: Meeting\nauthor: Jane\n# keep\nstatus: draft\n---\n\nBody"
    );
}

#[test]
fn a_new_title_should_lead_the_block() {
    let source = "---\nauthor: Jane\n---\nBody";
    let mut desired = yaml_document(source);
    desired.fields.insert(
        0,
        FrontmatterField::new("title", FrontmatterValue::Text("Meeting".to_owned())),
    );
    assert_eq!(
        replaced(source, Some(&desired)),
        "---\ntitle: \"Meeting\"\nauthor: Jane\n---\n\nBody"
    );
}
