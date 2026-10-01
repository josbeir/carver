//! Self-contained note templates and property merging.

pub mod patterns;
pub use patterns::{TemplateContext, expand_template_source};

use crate::{
    FrontmatterDocument, FrontmatterField, FrontmatterFormat, FrontmatterValue, Revision,
    parse_frontmatter_document, source_analysis::SourceAnalysis,
};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, fmt};
use thiserror::Error;
use time::OffsetDateTime;
use uuid::Uuid;

/// Stable identity of a reusable template.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[cfg_attr(feature = "json-schema", derive(schemars::JsonSchema))]
pub struct TemplateId(Uuid);
impl TemplateId {
    /// Creates a time-sortable identity.
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::now_v7())
    }
    /// Restores a persisted identity.
    #[must_use]
    pub const fn from_uuid(value: Uuid) -> Self {
        Self(value)
    }
}
impl Default for TemplateId {
    fn default() -> Self {
        Self::new()
    }
}
impl fmt::Display for TemplateId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// A named canonical source document, independent of the note library's queries.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct NoteTemplate {
    /// Stable identity.
    pub id: TemplateId,
    /// Trimmed display name.
    pub name: String,
    /// Canonical Carve source, including authored properties.
    pub source: String,
    /// Optimistic concurrency revision.
    pub revision: Revision,
    /// Creation time.
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    /// Last material edit time.
    #[serde(with = "time::serde::rfc3339")]
    pub updated_at: OffsetDateTime,
}

/// A template cannot be safely instantiated.
#[derive(Debug, Error)]
pub enum TemplateError {
    /// Unknown pattern, unclosed delimiter, or unsupported date/time format.
    #[error("invalid template pattern: {{{{{0}}}}}")]
    Pattern(String),
    /// Invalid frontmatter or repeated property keys.
    #[error("invalid template frontmatter: {0}")]
    Frontmatter(String),
    /// Managed assets require ownership and are unsupported in static templates.
    #[error("templates cannot contain managed images or attachments")]
    ManagedAssets,
    /// Serializing the merged properties failed.
    #[error(transparent)]
    Serialize(#[from] crate::FrontmatterError),
}

/// Validates frontmatter and rejects references to note-owned assets.
///
/// # Errors
/// Returns an error for invalid frontmatter, duplicate keys, or managed assets.
pub fn validate_template_source(source: &str) -> Result<(), TemplateError> {
    expand_template_source(
        source,
        &TemplateContext {
            now: OffsetDateTime::UNIX_EPOCH,
            category: String::new(),
        },
    )?;
    validate_template_structure(source)
}
fn validate_template_structure(source: &str) -> Result<(), TemplateError> {
    if !SourceAnalysis::parse(source).media().is_empty() {
        return Err(TemplateError::ManagedAssets);
    }
    if let Some(document) = parse_frontmatter_document(source) {
        if let Some(error) = document.error {
            return Err(TemplateError::Frontmatter(error));
        }
        let mut keys = BTreeSet::new();
        for field in &document.fields {
            if !keys.insert(&field.key) || has_duplicate_keys(&field.value) {
                return Err(TemplateError::Frontmatter(format!(
                    "duplicate property: {}",
                    field.key
                )));
            }
        }
    }
    Ok(())
}
fn has_duplicate_keys(value: &FrontmatterValue) -> bool {
    match value {
        FrontmatterValue::Object(entries) => {
            let mut keys = BTreeSet::new();
            entries
                .iter()
                .any(|(key, value)| !keys.insert(key) || has_duplicate_keys(value))
        }
        FrontmatterValue::List(values) => values.iter().any(has_duplicate_keys),
        _ => false,
    }
}

/// Adds enabled defaults absent from a template; authored values always win.
///
/// The caller resolves default values at creation time. An empty defaults slice disables seeding.
/// Body bytes and the template's frontmatter format are preserved.
///
/// # Errors
/// Returns validation or property serialization errors.
pub fn merge_template_source(
    source: &str,
    defaults: &[FrontmatterField],
    format: FrontmatterFormat,
) -> Result<String, TemplateError> {
    validate_template_structure(source)?;
    let mut document = parse_frontmatter_document(source).unwrap_or(FrontmatterDocument {
        format,
        fields: Vec::new(),
        error: None,
    });
    let initial = document.fields.len();
    for field in defaults {
        if !document
            .fields
            .iter()
            .any(|existing| existing.key == field.key)
        {
            document.fields.push(field.clone());
        }
    }
    if document.fields.len() == initial {
        return Ok(source.to_owned());
    }
    Ok(crate::frontmatter::replace_frontmatter_preserving_body(
        source, &document,
    )?)
}

#[cfg(test)]
mod tests;
