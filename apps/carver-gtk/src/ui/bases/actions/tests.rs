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

#[test]
fn new_base_definition_should_start_with_only_the_implicit_title() {
    let definition = new_base_definition();

    assert!(
        definition.columns.is_empty(),
        "a new Base shows only the implicit Title column until fields are added"
    );
    assert_eq!(definition.view, BaseView::Grid);
}

#[test]
fn base_view_selection_should_map_to_the_grid_and_list() {
    assert_eq!(base_view_from_active(false), BaseView::Grid);
    assert_eq!(base_view_from_active(true), BaseView::List);
}
