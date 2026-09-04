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
