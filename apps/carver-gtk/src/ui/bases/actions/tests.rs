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

#[test]
fn visible_column_reordering_should_move_a_custom_field_before_a_builtin_field() {
    let tag = BaseColumn::Property(carver_sdk::PropertyPath("/tag".to_owned()));
    let mut fields = vec![
        BaseColumn::Name,
        BaseColumn::Category,
        BaseColumn::Updated,
        tag.clone(),
    ];

    assert!(move_visible_column_by_offset(&mut fields, &tag, -1,));
    assert!(move_visible_column_by_offset(&mut fields, &tag, -1,));
    assert_eq!(
        fields,
        vec![
            BaseColumn::Name,
            tag,
            BaseColumn::Category,
            BaseColumn::Updated,
        ]
    );
}

#[test]
fn visible_column_reordering_should_keep_name_first() {
    let mut fields = vec![BaseColumn::Name, BaseColumn::Category];

    assert!(!move_visible_column_by_offset(
        &mut fields,
        &BaseColumn::Category,
        -1,
    ));
    assert_eq!(fields, vec![BaseColumn::Name, BaseColumn::Category]);
}
