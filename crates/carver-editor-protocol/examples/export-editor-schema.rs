//! Exports the editor event wire schema for development-time TypeScript generation.

use carver_editor_protocol::EditorEvent;
use schemars::generate::SchemaSettings;

fn main() -> Result<(), serde_json::Error> {
    let schema = SchemaSettings::draft07()
        .for_serialize()
        .into_generator()
        .into_root_schema_for::<EditorEvent>();
    println!("{}", serde_json::to_string_pretty(&schema)?);
    Ok(())
}
