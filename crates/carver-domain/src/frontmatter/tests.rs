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
        "---\nauthor: Jane\nproject: \"Carver\"\n---\nBody"
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
        "---\nb: two\na: one\n---\nBody"
    );
}

#[test]
fn format_switch_should_render_new_fences() {
    let source = "---\nauthor: Jane\n---\nBody";
    let mut desired = yaml_document(source);
    desired.format = FrontmatterFormat::Toml;
    assert_eq!(
        replaced(source, Some(&desired)),
        "---toml\nauthor = \"Jane\"\n---\nBody"
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
        "---\nauthor: Jane\n---\nBody"
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
        "---json\n{\"x\": 1}\n---\nBody"
    );
    assert_eq!(
        replace_frontmatter_raw("Body", FrontmatterFormat::Yaml, "a: b"),
        "---\na: b\n---\nBody"
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
        "---\na: \"changed\"\n# important\nb: two\n---\nBody"
    );
}
