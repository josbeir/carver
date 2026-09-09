//! Typed note properties and saved database-style views.

use std::{collections::BTreeSet, fmt};

use carve::{Options, parse_with_options};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::{NoteId, Revision};

/// A stable saved-base identifier.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
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
pub struct PropertyPath(pub String);

/// A column displayed by a saved base.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
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

/// A saved database-style view over active notes.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct BaseDefinition {
    /// Stable identity.
    pub id: BaseId,
    /// User-visible name.
    pub name: String,
    /// Ordered visible columns. Name is always rendered first by the GTK frontend.
    pub columns: Vec<BaseColumn>,
    /// Optimistic concurrency token.
    pub revision: Revision,
}

/// One row returned for a base.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
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

/// Extracts exact `---json` or `---toml` frontmatter through Carve's parsed document binding.
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
    fn visit(value: &Value, path: &mut String, found: &mut BTreeSet<String>) {
        if let Value::Object(map) = value {
            for (key, child) in map {
                let old_len = path.len();
                path.push('/');
                path.push_str(&key.replace('~', "~0").replace('/', "~1"));
                if child.is_object() {
                    visit(child, path, found);
                } else {
                    found.insert(path.clone());
                }
                path.truncate(old_len);
            }
        }
    }
    let mut found = BTreeSet::new();
    visit(value, &mut String::new(), &mut found);
    found.into_iter().map(PropertyPath).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn projection_should_ignore_yaml_and_report_malformed_toml() {
        assert_eq!(project_frontmatter("---yaml\na: b\n---").format, None);
        let malformed = project_frontmatter("---toml\na = [\n---");
        assert_eq!(malformed.format, Some("toml"));
        assert!(malformed.error.is_some());
    }
}
