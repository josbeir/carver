//! Format-neutral messages exchanged by an editor host and an editing surface.
//!
//! The protocol carries source text, commands, and UI state rather than a
//! lowest-common-denominator document tree. A Carve, Markdown, or future
//! adapter can therefore own its own faithful document model.

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};

/// A command initiated by the host's formatting controls.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EditorCommand {
    /// Toggle an inline or block command known by the active adapter.
    Named(String),
    /// Set a heading level, with zero selecting ordinary paragraph text.
    Heading(u8),
    /// Insert a table with the supplied dimensions.
    InsertTable {
        /// Total number of rows, including the optional header row.
        rows: u8,
        /// Number of columns.
        columns: u8,
        /// Whether the first row is a header.
        header: bool,
    },
    /// Replace the current selection with a labelled link.
    InsertLink {
        /// Visible link text.
        text: String,
        /// Link destination.
        destination: String,
    },
    /// Set a selected image's responsive width percentage, or restore intrinsic width.
    ImageWidth(Option<u8>),
}

/// Selection information used to reflect state in host-native controls.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SelectionState {
    /// Active formatting identifiers.
    pub active: Vec<String>,
    /// Heading level at the selection, or zero for a paragraph.
    pub heading: u8,
    /// Selected image width percentage, when an image is selected.
    pub image_width: Option<u8>,
}

/// Events emitted by an editing surface.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum EditorEvent {
    /// The web surface has loaded and can accept a document.
    Ready,
    /// A material document change produced canonical source.
    Changed {
        /// Monotonically increasing host session identifier.
        session: u64,
        /// Monotonically increasing revision within the session.
        revision: u64,
        /// Serialized source.
        source: String,
    },
    /// Selection or active formatting changed without editing source.
    Selection {
        /// Host document session.
        session: u64,
        /// Selected state.
        state: SelectionState,
    },
    /// The adapter cannot safely edit the document.
    Unsupported {
        /// Host document session.
        session: u64,
        /// Unsupported node names.
        unsupported: Vec<String>,
        /// Nodes whose original shape would degrade.
        degraded: Vec<String>,
    },
    /// A pasted image awaits managed-asset storage.
    PasteImage {
        /// Host document session.
        session: u64,
        /// Browser-reported media type.
        mime_type: String,
        /// Base64-encoded image bytes.
        data: String,
    },
}

#[cfg(test)]
mod tests {
    use super::{EditorCommand, EditorEvent, SelectionState};

    #[test]
    fn commands_round_trip_through_the_host_protocol() {
        let command = EditorCommand::InsertTable {
            rows: 2,
            columns: 3,
            header: true,
        };
        let encoded = serde_json::to_string(&command).unwrap_or_default();
        assert_eq!(
            encoded,
            "{\"insert-table\":{\"rows\":2,\"columns\":3,\"header\":true}}"
        );

        let link = EditorCommand::InsertLink {
            text: String::from("Carve"),
            destination: String::from("https://github.com/markup-carve"),
        };
        let encoded = serde_json::to_string(&link).unwrap_or_default();
        assert_eq!(
            encoded,
            "{\"insert-link\":{\"text\":\"Carve\",\"destination\":\"https://github.com/markup-carve\"}}"
        );
    }

    #[test]
    fn selection_messages_round_trip_without_loss() {
        let event = EditorEvent::Selection {
            session: 4,
            state: SelectionState {
                active: vec![String::from("bold"), String::from("table")],
                heading: 2,
                image_width: None,
            },
        };
        let encoded = serde_json::to_string(&event).unwrap_or_default();
        let decoded: EditorEvent = serde_json::from_str(&encoded).unwrap_or(EditorEvent::Ready);
        assert_eq!(decoded, event);
    }
}
