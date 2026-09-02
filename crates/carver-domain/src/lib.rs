//! The UI-independent domain model and Carve document helpers.

#![forbid(unsafe_code)]

use std::{collections::BTreeMap, fmt};

use carve::{Options, parse_with_options, render_carve, to_plain_text};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use time::OffsetDateTime;
use uuid::Uuid;

/// A stable category identifier.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct CategoryId(Uuid);

impl CategoryId {
    /// Creates a new time-sortable identifier.
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::now_v7())
    }

    /// Restores an identifier persisted by a repository.
    #[must_use]
    pub const fn from_uuid(value: Uuid) -> Self {
        Self(value)
    }

    /// Returns the underlying UUID for storage adapters.
    #[must_use]
    pub const fn as_uuid(self) -> Uuid {
        self.0
    }
}

impl Default for CategoryId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for CategoryId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

/// A stable note identifier.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct NoteId(Uuid);

impl NoteId {
    /// Creates a new time-sortable identifier.
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::now_v7())
    }

    /// Restores an identifier persisted by a repository.
    #[must_use]
    pub const fn from_uuid(value: Uuid) -> Self {
        Self(value)
    }

    /// Returns the underlying UUID for storage adapters.
    #[must_use]
    pub const fn as_uuid(self) -> Uuid {
        self.0
    }
}

impl Default for NoteId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for NoteId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

/// A monotonic version used to detect concurrent note writes.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Revision(pub i64);

/// A logical container for notes.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Category {
    /// Stable identity.
    pub id: CategoryId,
    /// User-visible category name.
    pub name: String,
    /// Explicit sidebar order.
    pub position: i64,
    /// Creation time.
    pub created_at: OffsetDateTime,
    /// Last metadata modification time.
    pub updated_at: OffsetDateTime,
    /// Time the category was moved to trash.
    pub trashed_at: Option<OffsetDateTime>,
}

/// A complete editable note.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Note {
    /// Stable identity.
    pub id: NoteId,
    /// Owning category.
    pub category_id: CategoryId,
    /// Canonical Carve source.
    pub source: String,
    /// Cached title derived from source.
    pub title: String,
    /// Cached text used by search and list excerpts.
    pub plain_text: String,
    /// Optimistic concurrency token.
    pub revision: Revision,
    /// Creation time.
    pub created_at: OffsetDateTime,
    /// Last content modification time.
    pub updated_at: OffsetDateTime,
    /// Time the note was moved to trash.
    pub trashed_at: Option<OffsetDateTime>,
}

/// Lightweight list data that avoids loading note bodies.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct NoteSummary {
    /// Stable identity.
    pub id: NoteId,
    /// Owning category.
    pub category_id: CategoryId,
    /// Derived display title.
    pub title: String,
    /// Short plaintext excerpt.
    pub excerpt: String,
    /// Last edit time.
    pub updated_at: OffsetDateTime,
    /// Whether the note has managed image assets.
    pub has_images: bool,
}

/// A date-heading and its recently edited notes.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RecentGroup {
    /// ISO-8601 local calendar date used for grouping.
    pub day: time::Date,
    /// Notes, newest first.
    pub notes: Vec<NoteSummary>,
}

/// A full-text search match.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SearchHit {
    /// List data for the match.
    pub note: NoteSummary,
    /// FTS5 generated snippet, safe to render as plain text.
    pub snippet: String,
}

/// A validated, derived representation of Carve source.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DerivedContent {
    /// User-visible title.
    pub title: String,
    /// Searchable text.
    pub plain_text: String,
}

/// Errors emitted when normalizing Carve source.
#[derive(Debug, Error)]
pub enum DocumentError {
    /// The Carve writer rejected a tree after rich editing.
    #[error("Carve source cannot be represented safely: {0}")]
    Unspellable(#[from] carve::RenderCarveError),
}

/// Derives a title and searchable plaintext from Carve source.
#[must_use]
pub fn derive_content(source: &str) -> DerivedContent {
    let document = parse_with_options(source, &Options::default().with_positions(true));
    let plain_text = to_plain_text(source);
    let title = first_heading(&document)
        .or_else(|| {
            plain_text
                .lines()
                .map(str::trim)
                .find(|line| !line.is_empty())
                .map(ToOwned::to_owned)
        })
        .unwrap_or_else(|| "Untitled Note".to_owned());

    DerivedContent { title, plain_text }
}

/// Canonicalizes source generated through a rich-editing session.
///
/// # Errors
///
/// Returns an error when the Carve writer cannot safely represent the parsed document.
pub fn canonicalize_rich_source(source: &str) -> Result<String, DocumentError> {
    let document = parse_with_options(source, &Options::default().with_positions(true));
    Ok(render_carve(&document)?)
}

fn first_heading(document: &carve::Document) -> Option<String> {
    document.children.iter().find_map(|block| match block {
        carve::BlockNode::Heading(heading) => {
            let source = carve::Document {
                frontmatter: BTreeMap::default(),
                frontmatter_raw: None,
                footnote_defs: BTreeMap::default(),
                footnote_def_pos: BTreeMap::default(),
                children: vec![carve::BlockNode::Heading(heading.clone())],
                source_len: 0,
                ingest_payload_len: 0,
            };
            let heading_text = to_plain_text(&render_carve(&source).ok()?);
            let heading_text = heading_text.trim();
            (!heading_text.is_empty()).then(|| heading_text.to_owned())
        }
        _ => None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derives_title_from_first_heading() {
        let content = derive_content("# A /useful/ note\n\nBody text");
        assert_eq!(content.title, "A useful note");
        assert!(content.plain_text.contains("Body text"));
    }

    #[test]
    fn derives_title_from_first_text_when_heading_is_missing() {
        assert_eq!(
            derive_content("\n\nHello world\n\nMore").title,
            "Hello world"
        );
    }

    #[test]
    fn gives_empty_notes_a_safe_title() {
        assert_eq!(derive_content(" ").title, "Untitled Note");
    }
}
