//! Side effects requested by the pure reducer.

use std::ops::Range;

use carver_config::Config;
use carver_sdk::{CategoryAppearance, CategoryId, DocumentImportFormat, NoteId};

use super::{
    ActionKey, EditorExportFormat, EditorSaveRequest, EditorSessionId, RequestId, TimerId,
};

/// Work that the runtime performs after rendering an updated model.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Effect {
    /// Atomically persist an immutable configuration snapshot.
    PersistConfig {
        /// Complete configuration to write.
        config: Config,
    },
    /// Create the initial default category when a new library is empty.
    EnsureDefaultCategory,
    /// Create a new note in an active category.
    CreateNote {
        /// Category that owns the note.
        category_id: CategoryId,
    },
    /// Convert and import one source document into an active category.
    ImportNote {
        /// Category that owns the imported note.
        category_id: CategoryId,
        /// File format selected by the GTK adapter.
        format: DocumentImportFormat,
        /// Immutable UTF-8 source snapshot.
        source: String,
    },
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
    /// Wait before accepting the latest editor source for preview rendering.
    SchedulePreview {
        /// Editor lifetime that scheduled the preview.
        session: EditorSessionId,
        /// Timer identity used to ignore superseded source edits.
        timer_id: TimerId,
    },
    /// Persist one immutable canonical editor source snapshot.
    SaveNote {
        /// Session, revision, and source to persist.
        request: EditorSaveRequest,
    },
    /// Prepare a non-PDF export from an immutable editor snapshot.
    PrepareEditorExport {
        /// Request identity used to retain and later write the prepared bytes.
        request_id: u64,
        /// Editor session that owns the source and assets.
        session: EditorSessionId,
        /// Managed note whose assets may be packaged.
        note_id: NoteId,
        /// Canonical source captured when export started.
        source: String,
        /// Root filename for portable archives.
        filename_stem: String,
        /// Selected direct export format.
        format: EditorExportFormat,
        /// Whether to package available managed images in a ZIP archive.
        include_assets: bool,
        /// URI selected by the user through the GTK file dialog.
        target_uri: String,
    },
    /// Persist a previously prepared export after confirmation.
    WriteEditorExport {
        /// Prepared export identity.
        request_id: u64,
    },
    /// Drop a prepared export whose warnings the user declined.
    DiscardEditorExport {
        /// Prepared export identity.
        request_id: u64,
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
        /// Alternative text to retain on completion.
        alt: String,
        /// Source selection to replace after storage completes, when applicable.
        source_selection: Option<Range<usize>>,
    },
    /// Load sidebar categories and active-note counts.
    LoadSidebar {
        /// Identity for stale-completion protection.
        request_id: RequestId,
    },
    /// Read the semantic revision after a local change wake-up.
    LoadLibraryRevision {
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
    /// Create a category with its selected visual identity.
    CreateCategoryWithAppearance {
        /// User-entered category name.
        name: String,
        /// Selected visual identity.
        appearance: CategoryAppearance,
    },
    /// Create a category, then move a note into it as one user action.
    CreateCategoryAndMoveNote {
        /// Mutation identity used by the completion reply and Undo state.
        action: ActionKey,
        /// User-entered category name.
        name: String,
        /// Note to move after creation succeeds.
        note_id: NoteId,
    },
    /// Rename a category.
    RenameCategory {
        /// Category to rename.
        category_id: CategoryId,
        /// User-entered category name.
        name: String,
    },
    /// Update a category name and visual identity.
    UpdateCategory {
        /// Category to update.
        category_id: CategoryId,
        /// User-entered category name.
        name: String,
        /// Selected visual identity.
        appearance: CategoryAppearance,
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
