//! External-consumer contract tests for `carver-editor-protocol`.

use carver_editor_protocol::{EditorCommand, EditorEvent};

#[test]
fn protocol_types_should_round_trip_through_the_public_json_contract()
-> Result<(), serde_json::Error> {
    let command = EditorCommand::InsertLink {
        text: "Carver".to_owned(),
        destination: "https://example.test".to_owned(),
    };
    let event = EditorEvent::Changed {
        session: 7,
        revision: 3,
        source: "# Note".to_owned(),
    };

    assert_eq!(
        serde_json::from_str::<EditorCommand>(&serde_json::to_string(&command)?)?,
        command
    );
    assert_eq!(
        serde_json::from_str::<EditorEvent>(&serde_json::to_string(&event)?)?,
        event
    );
    Ok(())
}

#[cfg(feature = "json-schema")]
#[test]
fn selection_schema_should_require_nullable_image_width_in_serialized_events()
-> Result<(), serde_json::Error> {
    let schema = schemars::generate::SchemaSettings::draft07()
        .for_serialize()
        .into_generator()
        .into_root_schema_for::<EditorEvent>();
    let schema = serde_json::to_value(schema)?;
    let selection = &schema["definitions"]["SelectionState"];
    assert_eq!(
        selection["properties"]["image_width"]["type"],
        serde_json::json!(["integer", "null"])
    );
    assert!(
        selection["required"]
            .as_array()
            .is_some_and(|fields| fields.iter().any(|field| field == "image_width"))
    );
    Ok(())
}
