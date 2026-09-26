//! Exports the editor wire schemas for development-time TypeScript generation.
//!
//! Emits a JSON object keyed by type name. The web generation script compiles each
//! entry into `protocol.generated.ts`.

use carver_editor_protocol::{DocumentTarget, EditorEvent, LinkCommand, TableCommand};
use schemars::generate::SchemaSettings;
use schemars::{JsonSchema, Schema};
use serde_json::{Map, Value};

/// Generates a self-contained serialization schema for one wire type.
///
/// Each schema gets its own generator so shared definitions, such as
/// `SelectionState`, are emitted only alongside the type that references them.
fn schema_for<T: JsonSchema>() -> Schema {
    SchemaSettings::draft07()
        .for_serialize()
        .into_generator()
        .into_root_schema_for::<T>()
}

fn main() -> Result<(), serde_json::Error> {
    let mut schemas = Map::new();
    for (name, schema) in [
        ("EditorEvent", schema_for::<EditorEvent>()),
        ("DocumentTarget", schema_for::<DocumentTarget>()),
        ("TableCommand", schema_for::<TableCommand>()),
        ("LinkCommand", schema_for::<LinkCommand>()),
    ] {
        schemas.insert(name.to_owned(), serde_json::to_value(schema)?);
    }
    println!("{}", serde_json::to_string_pretty(&Value::Object(schemas))?);
    Ok(())
}
