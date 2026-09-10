//! Side effects requested by the pure reducer.

use carver_config::Config;
use carver_editor_protocol::EditorCommand;
use carver_sdk::{
    BaseColumn, BaseFilter, BaseFilterMode, BaseId, BaseSort, CategoryAppearance, CategoryId,
    DocumentImportFormat, NoteId, Revision,
};

use super::{
    ActionKey, EditorCopyRequest, EditorExportDialogRequest, EditorExportFormat,
    EditorExportWarningRequest, EditorPdfExportRequest, EditorSaveRequest, EditorSessionId,
    RequestId, SourceImageTarget, TimerId,
};

/// Work that the runtime performs after rendering an updated model.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Effect {
    /// Delete only a saved Base definition, preserving its notes.
    DeleteBase {
        /// Definition to remove.
        base_id: BaseId,
    },
    /// Load saved base definitions.
    LoadBases {
        /// Identity for stale-completion protection.
        request_id: RequestId,
    },
    /// Load the library-wide frontmatter property descriptors used by Base configuration.
    LoadPropertyDescriptors {
        /// Identity for stale-completion protection.
        request_id: RequestId,
    },
    /// Load rows for one saved base.
    LoadBaseRows {
        /// Identity for stale-completion protection.
        request_id: RequestId,
        /// Saved view to query.
        base_id: BaseId,
    },
    /// Create a saved base.
    CreateBase {
        /// User-visible view name.
        name: String,
        /// Ordered initial columns.
        columns: Vec<BaseColumn>,
    },
    /// Save a complete Base configuration.
    UpdateBase {
        /// Definition identity.
        base_id: BaseId,
        /// Revision read when the editor opened.
        revision: Revision,
        /// User-visible name.
        name: String,
        /// Ordered visible columns.
        columns: Vec<BaseColumn>,
        /// Filter combination mode.
        filter_mode: BaseFilterMode,
        /// Visual filters.
        filters: Vec<BaseFilter>,
        /// Ordered sort rules.
        sorts: Vec<BaseSort>,
    },
    /// Read and store native files sequentially with a bounded per-file read.
    ImportEditorFiles {
        /// Initiating document and source position.
        target: super::ImportTarget,
        /// Owning note.
        note_id: NoteId,
        /// Files in selection order.
        files: Vec<super::ImportFileSource>,
    },
    /// Read and stage one managed attachment for external preview.
    PrepareMediaPreview {
        /// Requesting editor lifetime.
        session: EditorSessionId,
        /// Note that owns the asset.
        note_id: NoteId,
        /// Canonical managed asset path.
        path: String,
        /// Friendly title for the preview copy.
        label: String,
    },
    /// Open the prepared copy through Sushi or the desktop file launcher.
    ShowMediaPreview {
        /// Requesting editor lifetime.
        session: EditorSessionId,
        /// Path of the isolated copy.
        path: std::path::PathBuf,
    },

    /// Apply an editor command through the selected rich-text projection.
    ApplyRichEditorCommand {
        /// Immutable protocol command accepted by the reducer.
        command: EditorCommand,
    },
    /// Reload the rich-text projection after an asynchronous source mutation.
    ReloadRichEditor {
        /// Active editor lifetime that owns the rich-text projection.
        session: EditorSessionId,
        /// Canonical source to load into the projection.
        source: String,
    },
    /// Restore the source-editor selection after a reducer-owned source edit renders.
    SelectEditorSource {
        /// Active editor lifetime that owns the selection.
        session: EditorSessionId,
        /// Character-based selection in the canonical source.
        selection: std::ops::Range<usize>,
    },
    /// Resolve a managed asset through the asynchronous SDK boundary.
    LoadMediaFile {
        /// Whether image bytes are needed for a thumbnail.
        image: bool,
        /// Editor lifetime receiving the result.
        session: EditorSessionId,
        /// Owning note.
        note_id: NoteId,
        /// Canonical asset path.
        path: String,
    },
    /// Focus a document occurrence through the active projection.
    FocusDocumentTarget {
        /// Document lifetime that owns the occurrence.
        session: EditorSessionId,
        /// Current canonical source generation.
        generation: u64,
        /// Source range to focus, or an empty range for a heading caret.
        selection: std::ops::Range<usize>,
        /// Projection-neutral occurrence address.
        target: carver_editor_protocol::DocumentTarget,
    },
    /// Publish a canonical editor snapshot through the native clipboard adapter.
    CopyEditorDocument {
        /// Immutable copy request owned by the current editor session.
        request: EditorCopyRequest,
    },
    /// Present the native export-options dialog for an immutable editor snapshot.
    ShowEditorExportDialog {
        /// Immutable dialog request owned by the current editor session.
        request: EditorExportDialogRequest,
    },
    /// Present warnings emitted while preparing an export.
    ShowEditorExportWarning {
        /// Immutable warning request awaiting a user decision.
        request: EditorExportWarningRequest,
    },
    /// Render and write or print a PDF through the native GTK adapter.
    ExportEditorPdf {
        /// Immutable PDF request owned by the current editor session.
        request: EditorPdfExportRequest,
    },
    /// Atomically persist an immutable configuration snapshot.
    PersistConfig {
        /// Complete configuration to write.
        config: Config,
    },
    /// Create the default category when the library has no active categories.
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
        /// HTML profile captured with the export source.
        html_profile: carver_domain::rendering::HtmlProfile,
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
        /// Requested markup kind, independent of the canonical filename.
        image: bool,
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
        /// Source target to replace after storage completes, when applicable.
        source_target: Option<SourceImageTarget>,
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
    /// Load browser notes and favorites together for the selected category and query.
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
    /// Refresh an already open note without navigating.
    RefreshEditorNote {
        /// Identity of this refresh.
        request_id: RequestId,
        /// Editor lifetime that requested the refresh.
        session: EditorSessionId,
        /// Revision and source at the start of the read.
        snapshot: EditorSaveRequest,
        /// Whether the user explicitly authorized discarding local edits.
        discard_local: bool,
    },
    /// Offer an explicit reload after an external edit conflicts with local work.
    ShowExternalEdit {
        /// Editor lifetime affected by the conflict.
        session: EditorSessionId,
        /// Whether the note was deleted rather than edited.
        deleted: bool,
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
    /// Set one note's favorite state.
    SetNoteFavorite {
        /// Mutation identity used by the completion reply.
        action: ActionKey,
        /// Note to update.
        note_id: NoteId,
        /// Revision expected by the mutation.
        revision: Revision,
        /// Desired favorite state.
        is_favorite: bool,
    },
}
