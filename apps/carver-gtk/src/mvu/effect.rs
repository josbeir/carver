//! Side effects requested by the pure reducer.

use carver_sdk::{CategoryId, NoteId};

use super::{ActionKey, EditorSaveRequest, EditorSessionId, RequestId, TimerId};

/// Work that the runtime performs after rendering an updated model.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Effect {
    /// Wait before dispatching the current search timer identity.
    ScheduleSearch {
        /// Identity used to ignore a superseded debounce timer.
        timer_id: TimerId,
    },
    /// Wait before attempting to persist the latest editor source.
    ScheduleEditorSave {
        /// Editor lifetime that scheduled the autosave.
        session: EditorSessionId,
        /// Identity used to ignore a superseded autosave timer.
        timer_id: TimerId,
        /// Debounce duration from persisted preferences.
        delay_ms: u64,
    },
    /// Persist one immutable canonical editor source snapshot.
    SaveNote {
        /// Session, revision, and source to persist.
        request: EditorSaveRequest,
    },
    /// Store a rich-editor image as a managed asset for the active note.
    StoreEditorAsset {
        /// Editor lifetime that requested the asset.
        session: EditorSessionId,
        /// Owning note.
        note_id: NoteId,
        /// Validated file extension.
        extension: String,
        /// Image content to store.
        bytes: Vec<u8>,
    },
    /// Load sidebar categories and active-note counts.
    LoadSidebar {
        /// Identity for stale-completion protection.
        request_id: RequestId,
    },
    /// Load browser note summaries for the selected category and query.
    LoadBrowser {
        /// Identity for stale-completion protection.
        request_id: RequestId,
        /// Category to restrict the listing to, if any.
        category_id: Option<CategoryId>,
        /// Search input to apply.
        query: String,
    },
    /// Load a complete note before showing it in the editor.
    LoadEditorNote {
        /// Identity for stale-completion protection.
        request_id: RequestId,
        /// Note to open.
        note_id: NoteId,
    },
    /// Load recoverable deleted content.
    LoadTrash {
        /// Identity for stale-completion protection.
        request_id: RequestId,
    },
    /// Restore a category from trash.
    RestoreCategory {
        /// Category to restore.
        category_id: CategoryId,
    },
    /// Restore a note from trash.
    RestoreNote {
        /// Note to restore.
        note_id: NoteId,
    },
    /// Permanently remove all trashed content.
    EmptyTrash,
    /// Create a category.
    CreateCategory {
        /// User-entered category name.
        name: String,
    },
    /// Rename a category.
    RenameCategory {
        /// Category to rename.
        category_id: CategoryId,
        /// User-entered category name.
        name: String,
    },
    /// Move a category to trash.
    TrashCategory {
        /// Category to trash.
        category_id: CategoryId,
    },
    /// Move a note between categories.
    MoveNote {
        /// Mutation identity used by the completion reply.
        action: ActionKey,
        /// Note to move.
        note_id: NoteId,
        /// Destination category.
        category_id: CategoryId,
    },
    /// Move a note to trash.
    TrashNote {
        /// Note to trash.
        note_id: NoteId,
    },
}
