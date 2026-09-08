//! Exports the editor event wire schema for development-time TypeScript generation.

use carver_editor_protocol::{DocumentTarget, EditorEvent};
use schemars::generate::SchemaSettings;

fn main() -> Result<(), serde_json::Error> {
    let generator = SchemaSettings::draft07().for_serialize().into_generator();
    let schema = if std::env::args().nth(1).as_deref() == Some("target") {
        generator.into_root_schema_for::<DocumentTarget>()
    } else {
        generator.into_root_schema_for::<EditorEvent>()
    };
    println!("{}", serde_json::to_string_pretty(&schema)?);
    Ok(())
}
