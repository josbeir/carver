use super::*;

#[test]
fn from_value_should_infer_every_scalar_and_date_shape() {
    assert_eq!(
        PropertyType::from_value(&FrontmatterValue::Text("hello".to_owned())),
        PropertyType::Text
    );
    assert_eq!(
        PropertyType::from_value(&FrontmatterValue::Text("2026-09-27".to_owned())),
        PropertyType::Date
    );
    assert_eq!(
        PropertyType::from_value(&FrontmatterValue::Text("2026-09-27T10:30:00Z".to_owned())),
        PropertyType::DateTime
    );
    assert_eq!(
        PropertyType::from_value(&FrontmatterValue::Number(serde_json::Number::from(3))),
        PropertyType::Number
    );
    assert_eq!(
        PropertyType::from_value(&FrontmatterValue::Boolean(true)),
        PropertyType::Boolean
    );
    assert_eq!(
        PropertyType::from_value(&FrontmatterValue::List(vec![FrontmatterValue::Text(
            "rust".to_owned()
        )])),
        PropertyType::List
    );
    assert_eq!(
        PropertyType::from_value(&FrontmatterValue::Null),
        PropertyType::Text
    );
    assert_eq!(
        PropertyType::from_value(&FrontmatterValue::Object(Vec::new())),
        PropertyType::Text
    );
}

#[test]
fn from_value_should_not_treat_a_bare_time_as_a_date() {
    assert_eq!(
        PropertyType::from_value(&FrontmatterValue::Text("10:30:00".to_owned())),
        PropertyType::Text
    );
}

#[test]
fn kind_should_report_the_stored_json_kind() {
    assert_eq!(PropertyType::Text.kind(), PropertyKind::Text);
    assert_eq!(PropertyType::LongText.kind(), PropertyKind::Text);
    assert_eq!(PropertyType::Date.kind(), PropertyKind::Text);
    assert_eq!(PropertyType::DateTime.kind(), PropertyKind::Text);
    assert_eq!(PropertyType::Number.kind(), PropertyKind::Number);
    assert_eq!(PropertyType::Boolean.kind(), PropertyKind::Boolean);
    assert_eq!(PropertyType::List.kind(), PropertyKind::List);
}

#[test]
fn accepts_value_should_track_the_declared_type() {
    assert!(
        PropertyType::Number.accepts_value(&FrontmatterValue::Number(serde_json::Number::from(1)))
    );
    assert!(!PropertyType::Number.accepts_value(&FrontmatterValue::Text("1".to_owned())));
    assert!(PropertyType::Boolean.accepts_value(&FrontmatterValue::Boolean(false)));
    assert!(PropertyType::Text.accepts_value(&FrontmatterValue::Text(String::new())));
    assert!(PropertyType::Date.accepts_value(&FrontmatterValue::Text("2026-09-27".to_owned())));
    assert!(
        !PropertyType::Date
            .accepts_value(&FrontmatterValue::Text("2026-09-27T10:30:00Z".to_owned()))
    );
    assert!(
        PropertyType::DateTime
            .accepts_value(&FrontmatterValue::Text("2026-09-27T10:30:00Z".to_owned()))
    );
    // Every scalar type tolerates an explicit empty so it survives an unchanged save.
    for field_type in [
        PropertyType::Text,
        PropertyType::LongText,
        PropertyType::Number,
        PropertyType::Boolean,
        PropertyType::Date,
        PropertyType::DateTime,
    ] {
        assert!(field_type.accepts_value(&FrontmatterValue::Null));
    }
}

#[test]
fn accepts_json_should_validate_option_lists_and_nulls() {
    assert!(PropertyType::List.accepts_json(&serde_json::json!(["a", "b"])));
    assert!(!PropertyType::List.accepts_json(&serde_json::json!(["a", 2])));
    assert!(PropertyType::Number.accepts_json(&serde_json::json!(2)));
    assert!(!PropertyType::Number.accepts_json(&serde_json::json!("2")));
    assert!(PropertyType::Boolean.accepts_json(&serde_json::json!(true)));
    for field_type in [
        PropertyType::Text,
        PropertyType::LongText,
        PropertyType::Number,
        PropertyType::Boolean,
        PropertyType::List,
        PropertyType::Date,
        PropertyType::DateTime,
    ] {
        assert!(field_type.accepts_json(&serde_json::Value::Null));
    }
}

#[test]
fn normalized_json_should_replace_a_mismatched_default() {
    assert_eq!(
        PropertyType::Number.normalized_json(&serde_json::json!("2")),
        serde_json::json!(0)
    );
    assert_eq!(
        PropertyType::Boolean.normalized_json(&serde_json::json!("yes")),
        serde_json::json!(false)
    );
    assert_eq!(
        PropertyType::List.normalized_json(&serde_json::json!(2)),
        serde_json::json!([])
    );
    assert_eq!(
        PropertyType::Text.normalized_json(&serde_json::json!(2)),
        serde_json::json!("")
    );
    assert_eq!(
        PropertyType::Text.normalized_json(&serde_json::json!("kept")),
        serde_json::json!("kept")
    );
}

#[test]
fn parse_helpers_should_separate_dates_from_date_times() {
    assert!(parse_iso_date("2026-09-27").is_some());
    assert!(parse_iso_date("2026-09-27T10:30:00Z").is_none());
    assert!(parse_iso_date_time("2026-09-27T10:30:00Z").is_some());
    assert!(parse_iso_date_time("2026-09-27").is_none());
}
