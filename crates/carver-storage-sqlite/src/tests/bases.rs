use super::*;

#[test]
fn base_rows_should_project_yaml_json_and_toml_frontmatter() {
    let (_directory, library) = library();
    let now = OffsetDateTime::UNIX_EPOCH;
    let category = library
        .create_category("Projects", now)
        .unwrap_or_else(|error| panic!("category failed: {error}"));
    let json = library
        .create_note_with_source(
            category.id,
            "---json\n{\"status\":\"active\",\"score\":3}\n---\n# JSON",
            now,
        )
        .unwrap_or_else(|error| panic!("JSON note failed: {error}"));
    let _toml = library
        .create_note_with_source(
            category.id,
            "---toml\nstatus = \"waiting\"\n[owner]\nname = \"Ada\"\n---\n# TOML",
            now,
        )
        .unwrap_or_else(|error| panic!("TOML note failed: {error}"));
    let yaml = library
        .create_note_with_source(
            category.id,
            "---\nstatus: ready\nowner:\n  name: Grace\n---\n# YAML",
            now,
        )
        .unwrap_or_else(|error| panic!("YAML note failed: {error}"));
    let base = library
        .create_base(
            "Pipeline",
            &[
                BaseColumn::Category,
                BaseColumn::Property(PropertyPath("/status".to_owned())),
            ],
        )
        .unwrap_or_else(|error| panic!("base failed: {error}"));
    assert_eq!(base.row_count, 3);
    assert_eq!(
        library
            .bases()
            .unwrap_or_else(|error| panic!("bases failed: {error}"))[0]
            .row_count,
        3
    );

    let rows = library
        .base_rows(base.id)
        .unwrap_or_else(|error| panic!("rows failed: {error}"));
    let json_row = rows
        .iter()
        .find(|row| row.note_id == json.id)
        .unwrap_or_else(|| panic!("JSON row missing"));

    assert_eq!(
        json_row.properties.pointer("/status"),
        Some(&serde_json::json!("active"))
    );
    let yaml_row = rows
        .iter()
        .find(|row| row.note_id == yaml.id)
        .unwrap_or_else(|| panic!("YAML row missing"));
    assert_eq!(
        yaml_row.properties.pointer("/owner/name"),
        Some(&serde_json::json!("Grace"))
    );
    assert_eq!(json_row.updated, "1970-01-01T00:00:00Z");
    assert_eq!(
        library
            .property_paths()
            .unwrap_or_else(|error| panic!("paths failed: {error}")),
        vec![
            PropertyPath("/owner/name".to_owned()),
            PropertyPath("/score".to_owned()),
            PropertyPath("/status".to_owned()),
        ]
    );
}

#[test]
fn malformed_frontmatter_should_not_block_note_save_or_enter_the_projection() {
    let (_directory, library) = library();
    let now = OffsetDateTime::UNIX_EPOCH;
    let category = library
        .create_category("Inbox", now)
        .unwrap_or_else(|error| panic!("category failed: {error}"));
    let note = library
        .create_note_with_source(category.id, "---json\n{broken}\n---\n# Safe", now)
        .unwrap_or_else(|error| panic!("note failed: {error}"));
    let base = library
        .create_base("All", &[])
        .unwrap_or_else(|error| panic!("base failed: {error}"));
    let rows = library
        .base_rows(base.id)
        .unwrap_or_else(|error| panic!("rows failed: {error}"));

    assert_eq!(rows[0].note_id, note.id);
    assert!(rows[0].properties.is_null());
}
