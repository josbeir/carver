//! Typed note properties and saved database-style views.

use std::{cmp::Ordering, collections::BTreeMap, fmt};

use carve::{Options, parse_with_options};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::{NoteId, Revision};

/// A stable saved-base identifier.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct BaseId(Uuid);

impl BaseId {
    /// Creates a new time-sortable identifier.
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::now_v7())
    }

    /// Restores a persisted identifier.
    #[must_use]
    pub const fn from_uuid(value: Uuid) -> Self {
        Self(value)
    }

    /// Returns the underlying UUID.
    #[must_use]
    pub const fn as_uuid(self) -> Uuid {
        self.0
    }
}

impl Default for BaseId {
    fn default() -> Self {
        Self::new()
    }
}
impl fmt::Display for BaseId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// A flattened property path using JSON Pointer escaping.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct PropertyPath(pub String);

/// The observed JSON value kind for a frontmatter property.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub enum PropertyKind {
    /// A text value.
    Text,
    /// A numeric value.
    Number,
    /// A boolean value.
    Boolean,
    /// An array value.
    List,
    /// A null value.
    Null,
    /// Multiple value kinds were observed at this path.
    Mixed,
}

/// A discovered frontmatter property with display metadata.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct PropertyDescriptor {
    /// Canonical JSON Pointer path.
    pub path: PropertyPath,
    /// Value kind observed across active notes.
    pub kind: PropertyKind,
    /// A representative JSON value suitable for a compact UI preview.
    pub example: Option<String>,
}

impl PropertyDescriptor {
    /// Merges another observation of the same property into this descriptor.
    pub fn merge_observation(&mut self, other: &Self) {
        if self.kind != other.kind {
            self.kind = PropertyKind::Mixed;
        }
        match (&self.example, &other.example) {
            (None, Some(candidate)) => self.example = Some(candidate.clone()),
            (Some(current), Some(candidate)) if candidate < current => {
                self.example = Some(candidate.clone());
            }
            _ => {}
        }
    }
}

/// A column displayed by a saved base.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub enum BaseColumn {
    /// Derived note title.
    Name,
    /// Owning category.
    Category,
    /// Last modification timestamp.
    Updated,
    /// A typed frontmatter property.
    Property(PropertyPath),
}

/// Combines filters when evaluating a saved base.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub enum BaseFilterMode {
    /// Every filter must match.
    #[default]
    All,
    /// At least one filter must match.
    Any,
}

/// Comparison supported by a base filter.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub enum BaseFilterOperator {
    /// Exact scalar equality.
    Equals,
    /// Exact scalar inequality.
    NotEquals,
    /// Case-insensitive text containment.
    Contains,
    /// Case-insensitive text prefix matching.
    StartsWith,
    /// Numeric greater-than comparison.
    GreaterThan,
    /// Numeric less-than comparison.
    LessThan,
    /// Numeric greater-than-or-equal comparison.
    GreaterOrEqual,
    /// Numeric less-than-or-equal comparison.
    LessOrEqual,
    /// A non-null scalar or list is present.
    IsPresent,
    /// A value is absent or null.
    IsMissing,
    /// A list contains the supplied scalar.
    ListContains,
    /// A list does not contain the supplied scalar.
    ListNotContains,
}

/// One visual filter in a saved base.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct BaseFilter {
    /// Field to inspect.
    pub field: BaseColumn,
    /// Comparison to apply.
    pub operator: BaseFilterOperator,
    /// Comparison value, omitted for presence checks.
    #[serde(default)]
    pub value: Option<Value>,
}

/// Sort direction for one saved rule.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub enum BaseSortDirection {
    /// Smallest values first.
    Ascending,
    /// Largest values first.
    Descending,
}

/// One ordered sort rule in a saved base.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct BaseSort {
    /// Field to sort by.
    pub field: BaseColumn,
    /// Direction of the rule.
    pub direction: BaseSortDirection,
}

/// A saved database-style view over active notes.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct BaseDefinition {
    /// Stable identity.
    pub id: BaseId,
    /// User-visible name.
    pub name: String,
    /// Ordered visible columns. Name is always rendered first by the GTK frontend.
    pub columns: Vec<BaseColumn>,
    /// How filters are combined.
    #[serde(default)]
    pub filter_mode: BaseFilterMode,
    /// Ordered visual filters.
    #[serde(default)]
    pub filters: Vec<BaseFilter>,
    /// Ordered sort rules.
    #[serde(default)]
    pub sorts: Vec<BaseSort>,
    /// Optimistic concurrency token.
    pub revision: Revision,
    /// Number of active notes currently represented by this view.
    #[serde(default)]
    pub row_count: usize,
}

impl BaseDefinition {
    /// Returns a definition with the default all-notes view configuration.
    #[must_use]
    pub fn defaults(
        id: BaseId,
        name: String,
        columns: Vec<BaseColumn>,
        revision: Revision,
    ) -> Self {
        Self {
            id,
            name,
            columns,
            filter_mode: BaseFilterMode::All,
            filters: Vec::new(),
            sorts: Vec::new(),
            revision,
            row_count: 0,
        }
    }
}

/// One row returned for a base.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct BaseRow {
    /// Source note.
    pub note_id: NoteId,
    /// Note revision used by inline edits.
    pub revision: Revision,
    /// Derived title.
    pub name: String,
    /// Owning category name.
    pub category: String,
    /// RFC 3339 update time.
    pub updated: String,
    /// Extracted property document, or null for notes without supported frontmatter.
    pub properties: Value,
}

fn field_value(row: &BaseRow, field: &BaseColumn) -> Option<Value> {
    match field {
        BaseColumn::Name => Some(Value::String(row.name.clone())),
        BaseColumn::Category => Some(Value::String(row.category.clone())),
        BaseColumn::Updated => Some(Value::String(row.updated.clone())),
        BaseColumn::Property(path) => row.properties.pointer(&path.0).cloned(),
    }
}

fn scalar_equal(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::String(left), Value::String(right)) => left.eq_ignore_ascii_case(right),
        (Value::Number(left), Value::Number(right)) => left == right,
        (Value::Bool(left), Value::Bool(right)) => left == right,
        _ => false,
    }
}

fn number(value: &Value) -> Option<f64> {
    value.as_f64()
}

fn filter_matches(row: &BaseRow, filter: &BaseFilter) -> bool {
    let value = field_value(row, &filter.field).filter(|value| !value.is_null());
    match filter.operator {
        BaseFilterOperator::IsPresent => value.is_some(),
        BaseFilterOperator::IsMissing => value.is_none(),
        BaseFilterOperator::ListContains | BaseFilterOperator::ListNotContains => {
            let contains = value
                .as_ref()
                .and_then(Value::as_array)
                .zip(filter.value.as_ref())
                .is_some_and(|(values, needle)| {
                    values.iter().any(|value| scalar_equal(value, needle))
                });
            if matches!(filter.operator, BaseFilterOperator::ListNotContains) {
                value.as_ref().and_then(Value::as_array).is_some() && !contains
            } else {
                contains
            }
        }
        BaseFilterOperator::Contains | BaseFilterOperator::StartsWith => {
            let Some(Value::String(actual)) = value.as_ref() else {
                return false;
            };
            let Some(Value::String(expected)) = filter.value.as_ref() else {
                return false;
            };
            let actual = actual.to_ascii_lowercase();
            let expected = expected.to_ascii_lowercase();
            if matches!(filter.operator, BaseFilterOperator::Contains) {
                actual.contains(&expected)
            } else {
                actual.starts_with(&expected)
            }
        }
        BaseFilterOperator::Equals | BaseFilterOperator::NotEquals => {
            let equal = value
                .as_ref()
                .zip(filter.value.as_ref())
                .is_some_and(|(actual, expected)| scalar_equal(actual, expected));
            if matches!(filter.operator, BaseFilterOperator::NotEquals) {
                value.is_some() && !equal
            } else {
                equal
            }
        }
        BaseFilterOperator::GreaterThan
        | BaseFilterOperator::LessThan
        | BaseFilterOperator::GreaterOrEqual
        | BaseFilterOperator::LessOrEqual => {
            let Some(left) = value.as_ref().and_then(number) else {
                return false;
            };
            let Some(right) = filter.value.as_ref().and_then(number) else {
                return false;
            };
            match filter.operator {
                BaseFilterOperator::GreaterThan => left > right,
                BaseFilterOperator::LessThan => left < right,
                BaseFilterOperator::GreaterOrEqual => left >= right,
                BaseFilterOperator::LessOrEqual => left <= right,
                _ => false,
            }
        }
    }
}

/// Returns whether a row satisfies the saved filter set.
#[must_use]
pub fn base_row_matches(row: &BaseRow, mode: BaseFilterMode, filters: &[BaseFilter]) -> bool {
    if filters.is_empty() {
        return true;
    }
    match mode {
        BaseFilterMode::All => filters.iter().all(|filter| filter_matches(row, filter)),
        BaseFilterMode::Any => filters.iter().any(|filter| filter_matches(row, filter)),
    }
}

/// Applies filters and deterministic ordered sorting to base rows.
#[must_use]
pub fn project_base_rows(
    mut rows: Vec<BaseRow>,
    mode: BaseFilterMode,
    filters: &[BaseFilter],
    sorts: &[BaseSort],
) -> Vec<BaseRow> {
    if !filters.is_empty() {
        rows.retain(|row| base_row_matches(row, mode, filters));
    }
    rows.sort_by(|left, right| {
        let ordering = sorts
            .iter()
            .find_map(|sort| {
                let left_value = field_value(left, &sort.field);
                let right_value = field_value(right, &sort.field);
                let ordering = match (
                    left_value.filter(|value| !value.is_null()),
                    right_value.filter(|value| !value.is_null()),
                ) {
                    (None, Some(_)) => Ordering::Greater,
                    (Some(_), None) => Ordering::Less,
                    (Some(Value::String(left)), Some(Value::String(right))) => {
                        left.to_ascii_lowercase().cmp(&right.to_ascii_lowercase())
                    }
                    (Some(Value::Number(left)), Some(Value::Number(right))) => left
                        .as_f64()
                        .partial_cmp(&right.as_f64())
                        .unwrap_or(Ordering::Equal),
                    (Some(Value::Bool(left)), Some(Value::Bool(right))) => left.cmp(&right),
                    _ => Ordering::Equal,
                };
                (ordering != Ordering::Equal).then_some(
                    if matches!(sort.direction, BaseSortDirection::Ascending) {
                        ordering
                    } else {
                        ordering.reverse()
                    },
                )
            })
            .unwrap_or(Ordering::Equal);
        ordering.then_with(|| left.note_id.cmp(&right.note_id))
    });
    if sorts.is_empty() {
        rows.sort_by(|left, right| {
            right
                .updated
                .cmp(&left.updated)
                .then_with(|| left.note_id.cmp(&right.note_id))
        });
    }
    rows
}

/// Result of indexing one Carve document's typed frontmatter.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FrontmatterProjection {
    /// Typed format accepted by Bases.
    pub format: Option<&'static str>,
    /// Compact JSON object used by SQLite JSON1.
    pub json: Option<String>,
    /// Parse or shape error; malformed metadata never blocks note persistence.
    pub error: Option<String>,
}

/// Extracts YAML (including Carve's default `---` form), JSON, or TOML frontmatter through
/// Carve's parsed document binding.
#[must_use]
pub fn project_frontmatter(source: &str) -> FrontmatterProjection {
    let document = parse_with_options(source, &Options::default().with_positions(true));
    let Some(raw) = document.frontmatter_raw else {
        return FrontmatterProjection {
            format: None,
            json: None,
            error: None,
        };
    };
    let (format, value) = match raw.format.as_str() {
        "json" => (
            "json",
            serde_json::from_str::<Value>(&raw.content).map_err(|error| error.to_string()),
        ),
        "toml" => (
            "toml",
            toml::from_str::<toml::Value>(&raw.content)
                .map_err(|error| error.to_string())
                .and_then(|value| serde_json::to_value(value).map_err(|error| error.to_string())),
        ),
        "yaml" => (
            "yaml",
            yaml_serde::from_str::<Value>(&raw.content).map_err(|error| error.to_string()),
        ),
        _ => {
            return FrontmatterProjection {
                format: None,
                json: None,
                error: None,
            };
        }
    };
    match value {
        Ok(Value::Object(map)) => FrontmatterProjection {
            format: Some(format),
            json: serde_json::to_string(&Value::Object(map)).ok(),
            error: None,
        },
        Ok(_) => FrontmatterProjection {
            format: Some(format),
            json: None,
            error: Some("frontmatter root must be an object".to_owned()),
        },
        Err(error) => FrontmatterProjection {
            format: Some(format),
            json: None,
            error: Some(error),
        },
    }
}

/// Flattens nested object leaves into discoverable JSON Pointer paths.
#[must_use]
pub fn property_paths(value: &Value) -> Vec<PropertyPath> {
    property_descriptors(value)
        .into_iter()
        .map(|descriptor| descriptor.path)
        .collect()
}

/// Flattens nested object leaves into descriptors for field pickers.
#[must_use]
pub fn property_descriptors(value: &Value) -> Vec<PropertyDescriptor> {
    fn kind(value: &Value) -> PropertyKind {
        match value {
            Value::String(_) => PropertyKind::Text,
            Value::Number(_) => PropertyKind::Number,
            Value::Bool(_) => PropertyKind::Boolean,
            Value::Array(_) => PropertyKind::List,
            Value::Null => PropertyKind::Null,
            Value::Object(_) => PropertyKind::Mixed,
        }
    }

    fn example(value: &Value) -> String {
        match value {
            Value::String(text) => text.clone(),
            _ => value.to_string(),
        }
    }

    fn visit(value: &Value, path: &mut String, found: &mut BTreeMap<String, PropertyDescriptor>) {
        if let Value::Object(map) = value {
            for (key, child) in map {
                let old_len = path.len();
                path.push('/');
                path.push_str(&key.replace('~', "~0").replace('/', "~1"));
                if child.is_object() {
                    visit(child, path, found);
                } else {
                    let descriptor = PropertyDescriptor {
                        path: PropertyPath(path.clone()),
                        kind: kind(child),
                        example: Some(example(child)),
                    };
                    if let Some(existing) = found.get_mut(path) {
                        existing.merge_observation(&descriptor);
                    } else {
                        found.insert(path.clone(), descriptor);
                    }
                }
                path.truncate(old_len);
            }
        }
    }
    let mut found = BTreeMap::new();
    visit(value, &mut String::new(), &mut found);
    found.into_values().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "json-schema")]
    #[test]
    fn base_wire_types_should_support_schema_generation() {
        let definition = schemars::schema_for!(BaseDefinition);
        assert!(definition.as_value()["properties"]["columns"].is_object());
        let row = schemars::schema_for!(BaseRow);
        assert!(row.as_value()["properties"]["properties"].is_object());
    }

    #[test]
    fn projection_should_accept_typed_json_and_flatten_nested_leaves() {
        let projection = project_frontmatter(
            "---json\n{\"status\":\"active\",\"owner\":{\"name\":\"Ada\"}}\n---\n# Note",
        );
        let value: Value = serde_json::from_str(projection.json.as_deref().unwrap_or("null"))
            .unwrap_or(Value::Null);
        assert_eq!(projection.format, Some("json"));
        assert_eq!(
            property_paths(&value),
            vec![
                PropertyPath("/owner/name".to_owned()),
                PropertyPath("/status".to_owned())
            ]
        );
    }

    #[test]
    fn projection_should_accept_yaml_and_report_malformed_toml() {
        let yaml = project_frontmatter(
            "---\nstatus: active\nowner:\n  name: Ada\ntags:\n  - rust\n  - notes\n---",
        );
        assert_eq!(yaml.format, Some("yaml"));
        let value: Value =
            serde_json::from_str(yaml.json.as_deref().unwrap_or("null")).unwrap_or(Value::Null);
        assert_eq!(
            value.pointer("/owner/name"),
            Some(&Value::String("Ada".to_owned()))
        );
        assert_eq!(
            value.pointer("/tags/1"),
            Some(&Value::String("notes".to_owned()))
        );
        let malformed = project_frontmatter("---toml\na = [\n---");
        assert_eq!(malformed.format, Some("toml"));
        assert!(malformed.error.is_some());
    }

    #[test]
    fn projection_should_handle_absent_and_non_object_frontmatter() {
        assert_eq!(
            project_frontmatter("# No metadata"),
            FrontmatterProjection {
                format: None,
                json: None,
                error: None,
            }
        );
        let scalar = project_frontmatter("---json\n[1, 2]\n---");
        assert_eq!(scalar.format, Some("json"));
        assert_eq!(scalar.json, None);
        assert_eq!(
            scalar.error.as_deref(),
            Some("frontmatter root must be an object")
        );
        let yaml_scalar = project_frontmatter("---yaml\n- one\n- two\n---");
        assert_eq!(yaml_scalar.format, Some("yaml"));
        assert_eq!(
            yaml_scalar.error.as_deref(),
            Some("frontmatter root must be an object")
        );
        assert!(project_frontmatter("---yaml\nkey: [\n---").error.is_some());
    }

    #[test]
    fn property_paths_should_escape_json_pointer_tokens_and_ignore_scalar_roots() {
        assert_eq!(property_paths(&serde_json::json!(true)), Vec::new());
        assert_eq!(
            property_paths(&serde_json::json!({"a/b": {"~key": 1}})),
            vec![PropertyPath("/a~1b/~0key".to_owned())]
        );
    }

    #[test]
    fn property_descriptors_should_include_leaf_kind_and_example() {
        assert_eq!(
            property_descriptors(&serde_json::json!({
                "title": "Roadmap",
                "priority": 2,
                "done": false,
                "tags": ["rust", "gtk"],
                "owner": {"name": "Ada"},
                "empty": null,
            })),
            vec![
                PropertyDescriptor {
                    path: PropertyPath("/done".to_owned()),
                    kind: PropertyKind::Boolean,
                    example: Some("false".to_owned()),
                },
                PropertyDescriptor {
                    path: PropertyPath("/empty".to_owned()),
                    kind: PropertyKind::Null,
                    example: Some("null".to_owned()),
                },
                PropertyDescriptor {
                    path: PropertyPath("/owner/name".to_owned()),
                    kind: PropertyKind::Text,
                    example: Some("Ada".to_owned()),
                },
                PropertyDescriptor {
                    path: PropertyPath("/priority".to_owned()),
                    kind: PropertyKind::Number,
                    example: Some("2".to_owned()),
                },
                PropertyDescriptor {
                    path: PropertyPath("/tags".to_owned()),
                    kind: PropertyKind::List,
                    example: Some("[\"rust\",\"gtk\"]".to_owned()),
                },
                PropertyDescriptor {
                    path: PropertyPath("/title".to_owned()),
                    kind: PropertyKind::Text,
                    example: Some("Roadmap".to_owned()),
                },
            ]
        );
    }

    #[test]
    fn property_descriptors_should_merge_mixed_values_and_choose_a_stable_example() {
        let first = property_descriptors(&serde_json::json!({
            "status": "ready",
            "metadata": {"owner": "Zoe"},
            "a/b": {"~name": true},
        }));
        let second = property_descriptors(&serde_json::json!({
            "status": 2,
            "metadata": {"owner": "Ada"},
            "a/b": {"~name": false},
        }));
        let mut merged = BTreeMap::new();
        for descriptor in first.into_iter().chain(second) {
            merged
                .entry(descriptor.path.0.clone())
                .and_modify(|existing: &mut PropertyDescriptor| {
                    existing.merge_observation(&descriptor);
                })
                .or_insert(descriptor);
        }

        assert_eq!(
            merged["/status"],
            PropertyDescriptor {
                path: PropertyPath("/status".to_owned()),
                kind: PropertyKind::Mixed,
                example: Some("2".to_owned()),
            }
        );
        assert_eq!(merged["/metadata/owner"].example.as_deref(), Some("Ada"));
        assert_eq!(merged["/a~1b/~0name"].kind, PropertyKind::Boolean);
    }

    #[test]
    fn base_id_should_round_trip_its_uuid_and_display() {
        let id = BaseId::default();
        assert_eq!(BaseId::from_uuid(id.as_uuid()), id);
        assert_eq!(id.to_string(), id.as_uuid().to_string());
    }

    #[test]
    fn base_rows_should_filter_lists_and_sort_missing_values_last() {
        let row = |id: u128, name: &str, priority: i64, tags: &[&str]| BaseRow {
            note_id: NoteId::from_uuid(Uuid::from_u128(id)),
            revision: Revision(1),
            name: name.to_owned(),
            category: "Work".to_owned(),
            updated: format!("2026-01-0{id}T00:00:00Z"),
            properties: serde_json::json!({"priority": priority, "tags": tags}),
        };
        let rows = project_base_rows(
            vec![
                row(2, "Beta", 2, &["rust", "gtk"]),
                row(1, "Alpha", 1, &["notes"]),
            ],
            BaseFilterMode::All,
            &[BaseFilter {
                field: BaseColumn::Property(PropertyPath("/tags".to_owned())),
                operator: BaseFilterOperator::ListContains,
                value: Some(Value::String("rust".to_owned())),
            }],
            &[BaseSort {
                field: BaseColumn::Property(PropertyPath("/priority".to_owned())),
                direction: BaseSortDirection::Descending,
            }],
        );
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].name, "Beta");
    }

    #[test]
    fn empty_filter_sets_should_match_every_row() {
        let row = BaseRow {
            note_id: NoteId::default(),
            revision: Revision(1),
            name: "Note".to_owned(),
            category: "Work".to_owned(),
            updated: String::new(),
            properties: Value::Null,
        };
        assert!(base_row_matches(&row, BaseFilterMode::Any, &[]));
    }
}
