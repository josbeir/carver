use super::*;

#[test]
fn filter_values_should_round_trip_without_changing_type() {
    for value in [
        serde_json::json!("true"),
        serde_json::json!("123"),
        serde_json::json!(""),
        serde_json::json!(" padded "),
        serde_json::json!("ready"),
        serde_json::json!(true),
        serde_json::json!(123),
        serde_json::json!(["tag"]),
    ] {
        assert_eq!(value_from_text(&filter_value_text(&value)), Some(value));
    }
}
