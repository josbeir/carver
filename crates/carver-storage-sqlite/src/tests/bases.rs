use super::*;
use carver_domain::{BaseFilterOperator, BaseSort, BaseSortDirection, project_base_rows};

fn query_fixture() -> (tempfile::TempDir, SqliteLibrary, BaseDefinition) {
    let (directory, library) = library();
    let category = library
        .create_category("Projects", OffsetDateTime::UNIX_EPOCH)
        .unwrap_or_else(|error| panic!("category failed: {error}"));
    for (source, updated) in [
        (
            "---yaml\nstatus: État\nscore: 2\nenabled: true\ntags: [Rust, notes]\nproject/status: ready\n---\n# First",
            OffsetDateTime::UNIX_EPOCH,
        ),
        (
            "---yaml\nstatus: done\nscore: 10\nenabled: false\ntags: [other]\nproject/status: waiting\n---\n# Second",
            OffsetDateTime::UNIX_EPOCH + time::Duration::seconds(1),
        ),
        (
            "---yaml\nstatus: pending\ntags: []\n---\n# Third",
            OffsetDateTime::UNIX_EPOCH + time::Duration::seconds(2),
        ),
    ] {
        library
            .create_note_with_source(category.id, source, updated)
            .unwrap_or_else(|error| panic!("note failed: {error}"));
    }
    let base = library
        .create_base("Query", &[])
        .unwrap_or_else(|error| panic!("base failed: {error}"));
    (directory, library, base)
}

fn assert_sql_projection_matches_domain(
    library: &SqliteLibrary,
    base: &BaseDefinition,
    filter_mode: BaseFilterMode,
    filters: &[BaseFilter],
    sorts: &[BaseSort],
) {
    let expected = project_base_rows(
        library
            .active_base_rows()
            .unwrap_or_else(|error| panic!("active rows failed: {error}")),
        filter_mode,
        filters,
        sorts,
    );
    let updated = library
        .update_base(
            base.id,
            base.revision,
            &base.name,
            &[],
            filter_mode,
            filters,
            sorts,
        )
        .unwrap_or_else(|error| panic!("base update failed: {error}"));
    let rows = library
        .base_rows(updated.id)
        .unwrap_or_else(|error| panic!("base rows failed: {error}"));
    assert_eq!(rows, expected);
    assert_eq!(updated.row_count, expected.len());
}

#[test]
fn json1_query_should_match_unicode_text_filters() {
    let (_directory, library, base) = query_fixture();
    assert_sql_projection_matches_domain(
        &library,
        &base,
        BaseFilterMode::All,
        &[BaseFilter {
            field: BaseColumn::Property(PropertyPath("/status".to_owned())),
            operator: BaseFilterOperator::Equals,
            value: Some(serde_json::json!("e\u{301}TAT")),
        }],
        &[],
    );
}

#[test]
fn json1_query_should_match_list_membership_filters() {
    let (_directory, library, base) = query_fixture();
    assert_sql_projection_matches_domain(
        &library,
        &base,
        BaseFilterMode::All,
        &[BaseFilter {
            field: BaseColumn::Property(PropertyPath("/tags".to_owned())),
            operator: BaseFilterOperator::ListContains,
            value: Some(serde_json::json!("rust")),
        }],
        &[],
    );
}

#[test]
fn json1_query_should_match_numeric_filters_and_property_sorts() {
    let (_directory, library, base) = query_fixture();
    assert_sql_projection_matches_domain(
        &library,
        &base,
        BaseFilterMode::All,
        &[BaseFilter {
            field: BaseColumn::Property(PropertyPath("/score".to_owned())),
            operator: BaseFilterOperator::GreaterOrEqual,
            value: Some(serde_json::json!(2)),
        }],
        &[BaseSort {
            field: BaseColumn::Property(PropertyPath("/score".to_owned())),
            direction: BaseSortDirection::Descending,
        }],
    );
}

#[test]
fn json1_query_should_match_boolean_filters() {
    let (_directory, library, base) = query_fixture();
    assert_sql_projection_matches_domain(
        &library,
        &base,
        BaseFilterMode::All,
        &[BaseFilter {
            field: BaseColumn::Property(PropertyPath("/enabled".to_owned())),
            operator: BaseFilterOperator::Equals,
            value: Some(serde_json::json!(true)),
        }],
        &[],
    );
}

#[test]
fn json1_query_should_match_not_equals_null_as_a_presence_check() {
    let (_directory, library, base) = query_fixture();
    assert_sql_projection_matches_domain(
        &library,
        &base,
        BaseFilterMode::All,
        &[BaseFilter {
            field: BaseColumn::Property(PropertyPath("/enabled".to_owned())),
            operator: BaseFilterOperator::NotEquals,
            value: Some(serde_json::Value::Null),
        }],
        &[],
    );
}

#[test]
fn json1_query_should_match_escaped_property_paths() {
    let (_directory, library, base) = query_fixture();
    assert_sql_projection_matches_domain(
        &library,
        &base,
        BaseFilterMode::All,
        &[BaseFilter {
            field: BaseColumn::Property(PropertyPath("/project~1status".to_owned())),
            operator: BaseFilterOperator::Equals,
            value: Some(serde_json::json!("ready")),
        }],
        &[],
    );
}

#[test]
fn json1_query_should_match_any_filter_mode() {
    let (_directory, library, base) = query_fixture();
    assert_sql_projection_matches_domain(
        &library,
        &base,
        BaseFilterMode::Any,
        &[
            BaseFilter {
                field: BaseColumn::Property(PropertyPath("/status".to_owned())),
                operator: BaseFilterOperator::StartsWith,
                value: Some(serde_json::json!("é")),
            },
            BaseFilter {
                field: BaseColumn::Property(PropertyPath("/score".to_owned())),
                operator: BaseFilterOperator::GreaterThan,
                value: Some(serde_json::json!(5)),
            },
        ],
        &[],
    );
}

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
    let all_rows = library
        .active_base_rows()
        .unwrap_or_else(|error| panic!("unfiltered preview: {error}"));
    assert!(all_rows.len() > rows.len());
    assert_eq!(
        library
            .bases()
            .unwrap_or_else(|error| panic!("sidebar counts: {error}"))
            .iter()
            .find(|item| item.id == base.id)
            .unwrap_or_else(|| panic!("missing base"))
            .row_count,
        rows.len()
    );
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
