use super::*;
use carver_domain::BaseFilterOperator;

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
fn property_descriptors_should_ignore_the_active_base_filter() {
    let (_directory, library) = library();
    let now = OffsetDateTime::UNIX_EPOCH;
    let category = library
        .create_category("Projects", now)
        .unwrap_or_else(|error| panic!("category failed: {error}"));
    library
        .create_note_with_source(
            category.id,
            "---yaml\nstatus: active\npriority: high\n---\n# Active",
            now,
        )
        .unwrap_or_else(|error| panic!("active note failed: {error}"));
    library
        .create_note_with_source(
            category.id,
            "---yaml\nstatus: done\npriority: low\n---\n# Done",
            now,
        )
        .unwrap_or_else(|error| panic!("done note failed: {error}"));
    let trashed = library
        .create_note_with_source(
            category.id,
            "---yaml\ntrashed-only: true\n---\n# Trashed",
            now,
        )
        .unwrap_or_else(|error| panic!("trashed note failed: {error}"));
    library
        .trash_note(trashed.id, now)
        .unwrap_or_else(|error| panic!("trash failed: {error}"));
    let base = library
        .create_base(
            "Active",
            &[BaseColumn::Property(PropertyPath("/status".to_owned()))],
        )
        .unwrap_or_else(|error| panic!("base failed: {error}"));
    library
        .update_base(
            base.id,
            base.revision,
            "Active",
            &[BaseColumn::Property(PropertyPath("/status".to_owned()))],
            BaseFilterMode::All,
            &[BaseFilter {
                field: BaseColumn::Property(PropertyPath("/status".to_owned())),
                operator: BaseFilterOperator::Equals,
                value: Some(serde_json::json!("active")),
            }],
            &[],
        )
        .unwrap_or_else(|error| panic!("base update failed: {error}"));

    assert_eq!(library.base_rows(base.id).unwrap_or_default().len(), 1);
    let descriptors = library
        .property_descriptors()
        .unwrap_or_else(|error| panic!("descriptors failed: {error}"));
    assert_eq!(
        descriptors
            .iter()
            .map(|descriptor| descriptor.path.clone())
            .collect::<Vec<_>>(),
        vec![
            PropertyPath("/priority".to_owned()),
            PropertyPath("/status".to_owned()),
        ]
    );
    assert_eq!(descriptors[0].kind, carver_domain::PropertyKind::Text);
    assert_eq!(descriptors[0].example.as_deref(), Some("high"));
    assert!(
        !descriptors
            .iter()
            .any(|descriptor| { descriptor.path == PropertyPath("/trashed-only".to_owned()) })
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

#[test]
fn base_configuration_should_filter_rows_and_guard_revision() {
    let (_directory, library) = library();
    let now = OffsetDateTime::UNIX_EPOCH;
    let category = library
        .create_category("Work", now)
        .unwrap_or_else(|error| panic!("category failed: {error}"));
    library
        .create_note_with_source(category.id, "---yaml\nstatus: active\n---\n# Active", now)
        .unwrap_or_else(|error| panic!("active note failed: {error}"));
    library
        .create_note_with_source(category.id, "---yaml\nstatus: done\n---\n# Done", now)
        .unwrap_or_else(|error| panic!("done note failed: {error}"));
    let base = library
        .create_base("Pipeline", &[])
        .unwrap_or_else(|error| panic!("base failed: {error}"));
    let updated = library
        .update_base(
            base.id,
            base.revision,
            "Pipeline",
            &[BaseColumn::Property(PropertyPath("/status".to_owned()))],
            BaseFilterMode::All,
            &[BaseFilter {
                field: BaseColumn::Property(PropertyPath("/status".to_owned())),
                operator: BaseFilterOperator::Equals,
                value: Some(serde_json::json!("active")),
            }],
            &[],
        )
        .unwrap_or_else(|error| panic!("update failed: {error}"));
    assert_eq!(updated.row_count, 1);
    let rows = library
        .base_rows(base.id)
        .unwrap_or_else(|error| panic!("filtered rows failed: {error}"));
    assert_eq!(rows.len(), 1);
    assert!(matches!(
        library.update_base(
            base.id,
            base.revision,
            "Pipeline",
            &[],
            BaseFilterMode::All,
            &[],
            &[],
        ),
        Err(StorageError::Conflict)
    ));
}

#[test]
fn legacy_columns_only_base_definitions_should_load_with_default_configuration() {
    let (_directory, library) = library();
    let base = library
        .create_base("Legacy", &[BaseColumn::Category])
        .unwrap_or_else(|error| panic!("base failed: {error}"));
    library
        .connection
        .execute(
            "UPDATE bases SET definition_json = ?1 WHERE id = ?2",
            rusqlite::params![
                serde_json::to_string(&vec![BaseColumn::Category]).unwrap_or_default(),
                base.id.to_string()
            ],
        )
        .unwrap_or_else(|error| panic!("legacy definition update failed: {error}"));
    let loaded = library
        .bases()
        .unwrap_or_else(|error| panic!("bases failed: {error}"));
    assert_eq!(loaded[0].columns, vec![BaseColumn::Category]);
    assert_eq!(loaded[0].filter_mode, BaseFilterMode::All);
    assert!(loaded[0].filters.is_empty());
    assert!(loaded[0].sorts.is_empty());
}
