//! Messages accepted by the application reducer.

use carver_config::EditorMode;
use carver_sdk::{
    CategoryId, CategorySummary, NoteId, NoteSummary, Revision, TrashContents, TrashPurgeResult,
};

use super::{ActionKey, EditorSaveRequest, EditorSessionId, RequestId, TimerId, UiError};

/// A UI event or asynchronous completion accepted by the reducer.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AppMsg {
    /// Navigation intent.
    Navigation(NavigationMsg),
    /// Browser intent.
    Browser(BrowserMsg),
    /// Sidebar intent.
    Sidebar(SidebarMsg),
    /// Trash intent.
    Trash(TrashMsg),
    /// Editor intent.
    Editor(EditorMsg),
    /// Preference intent.
    Preferences(PreferencesMsg),
    /// Window lifecycle intent.
    Window(WindowMsg),
    /// Note and category mutation intent.
    Action(ActionMsg),
    /// Completion from an effect that accessed the library.
    Library(LibraryReply),
}

/// Messages that change the high-level visible surface.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NavigationMsg {
    /// Start initial data loading.
    Started,
    /// Show notes for a category, or all notes when absent.
    SelectCategory(Option<CategoryId>),
    /// Load a note into the editor.
    OpenNote(NoteId),
    /// Create a note in the selected category, or the first active category.
    CreateNote,
    /// Show the trash surface.
    ShowTrash,
    /// Return to the browser surface.
    ShowBrowser,
}

/// Browser events.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BrowserMsg {
    /// Reload the current browser query and category.
    Reload,
    /// Replace the user-entered search text.
    SearchChanged(String),
    /// A delayed search timer fired.
    SearchTimerFired(TimerId),
}

/// Sidebar events.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SidebarMsg {
    /// Reload sidebar categories and active-note counts.
    Reload,
}

/// Trash events.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TrashMsg {
    /// Reload recoverable deleted content.
    Reload,
    /// Restore a trashed category.
    RestoreCategory(CategoryId),
    /// Restore a trashed note.
    RestoreNote(NoteId),
    /// Permanently remove all recoverable content after confirmation.
    Empty,
}

/// Editor events whose persistence is introduced in the editor migration parts.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EditorMsg {
    /// Load a persisted note into the canonical editor document.
    Load {
        /// Persisted note to edit.
        note_id: NoteId,
        /// Revision expected by the next save.
        revision: Revision,
        /// Canonical Carve source read from the library.
        source: String,
    },
    /// Replace the canonical source after a source or rich projection changed it.
    SourceChanged(String),
    /// Debounce persistence after a source edit.
    AutosaveRequested,
    /// The latest editor autosave timer fired.
    AutosaveElapsed {
        /// Editor lifetime that scheduled the timer.
        session: EditorSessionId,
        /// Timer identity used to reject superseded source edits.
        timer_id: TimerId,
    },
    /// The latest debounced preview timer fired.
    PreviewElapsed {
        /// Editor lifetime that scheduled the preview.
        session: EditorSessionId,
        /// Timer identity used to reject superseded source edits.
        timer_id: TimerId,
    },
    /// Re-render editor projections after the desktop palette or accent changes.
    ThemeChanged,
    /// Retry the current failed save without changing canonical source.
    RetrySave,
    /// Return to the browser after saving the latest canonical source.
    BackRequested,
    /// Move the active editor note to trash.
    TrashRequested,
    /// Store an image supplied by the rich editor as a managed note asset.
    PasteImage {
        /// File extension selected from the `WebKit` MIME type.
        extension: String,
        /// Decoded image bytes from the `WebKit` bridge.
        bytes: Vec<u8>,
    },
    /// Store a native image selected outside the rich-editor bridge.
    ImportImage {
        /// Validated image extension.
        extension: String,
        /// Image content read by the GTK adapter.
        bytes: Vec<u8>,
        /// User-visible alternative text.
        alt: String,
    },
    /// Close the active editor lifetime.
    Close(EditorSessionId),
}

/// Preference changes requested by a view.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PreferencesMsg {
    /// Set the remote-image loading policy.
    SetRemoteImages(bool),
    /// Set the delay before an unsaved editor source is persisted.
    SetAutosaveDelay(u64),
    /// Set the preferred editor surface.
    SetEditorMode(EditorMode),
    /// Set source split-preview visibility.
    SetSourceSplitView(bool),
    /// Set source-editor line-number gutter visibility.
    SetSourceLineNumbers(bool),
    /// Set source-editor current-line highlighting.
    SetSourceHighlightCurrentLine(bool),
}

/// Window state that must survive the next launch.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WindowMsg {
    /// Persist the geometry reported by GTK during a close request.
    SaveGeometry {
        /// Window width in logical pixels.
        width: i32,
        /// Window height in logical pixels.
        height: i32,
        /// Whether the window is maximized.
        maximized: bool,
    },
}

/// Note and category mutations requested by a view.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ActionMsg {
    /// Create a category with a user-entered name.
    CreateCategory(String),
    /// Create a category, then move a note into it.
    CreateCategoryAndMoveNote {
        /// User-entered category name.
        name: String,
        /// Note to move after creation succeeds.
        note_id: NoteId,
        /// Current category, retained for Undo.
        source_category_id: CategoryId,
    },
    /// Rename a category with a user-entered name.
    RenameCategory {
        /// Category to rename.
        category_id: CategoryId,
        /// Replacement name.
        name: String,
    },
    /// Move a category and its notes to trash.
    TrashCategory(CategoryId),
    /// Move a note to another category.
    MoveNote {
        /// Note to move.
        note_id: NoteId,
        /// Current category, retained for Undo.
        source_category_id: CategoryId,
        /// Destination category.
        category_id: CategoryId,
    },
    /// Move the most recently moved note back to its source category.
    UndoMove,
    /// Move a note to trash.
    TrashNote(NoteId),
}

impl ActionMsg {
    pub(super) fn key(&self) -> Option<ActionKey> {
        Some(match self {
            Self::CreateCategory(_) => ActionKey::CreateCategory,
            Self::CreateCategoryAndMoveNote {
                note_id,
                source_category_id,
                ..
            }
            | Self::MoveNote {
                note_id,
                source_category_id,
                ..
            } => ActionKey::MoveNote {
                note_id: *note_id,
                source_category_id: *source_category_id,
            },
            Self::RenameCategory { category_id, .. } => ActionKey::RenameCategory(*category_id),
            Self::TrashCategory(category_id) => ActionKey::TrashCategory(*category_id),
            Self::UndoMove => return None,
            Self::TrashNote(note_id) => ActionKey::TrashNote(*note_id),
        })
    }
}

/// Values returned by asynchronous library effects.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LibraryReply {
    /// A newly created note is ready to open in the editor.
    NoteCreated {
        /// Created note or a displayable failure.
        result: Result<carver_sdk::Note, UiError>,
    },
    /// A configuration save completed.
    ConfigPersisted {
        /// Successful completion or an error that must remain visible.
        result: Result<(), UiError>,
    },
    /// Startup default-category initialization completed.
    DefaultCategoryEnsured {
        /// Successful completion or a displayable failure.
        result: Result<(), UiError>,
    },
    /// A note or category mutation completed.
    ActionFinished {
        /// Identity of the mutation admitted by the reducer.
        action: ActionKey,
        /// Successful result or a displayable failure.
        result: Result<(), UiError>,
    },
    /// Sidebar categories completed loading.
    SidebarLoaded {
        /// Identity of the initiating request.
        request_id: RequestId,
        /// Successful result or a displayable failure.
        result: Result<Vec<CategorySummary>, UiError>,
    },
    /// Browser summaries completed loading.
    BrowserLoaded {
        /// Identity of the initiating request.
        request_id: RequestId,
        /// Successful result or a displayable failure.
        result: Result<Vec<NoteSummary>, UiError>,
    },
    /// A complete note finished loading for the editor.
    EditorLoaded {
        /// Identity of the initiating request.
        request_id: RequestId,
        /// Successful note or a displayable failure.
        result: Result<carver_sdk::Note, UiError>,
    },
    /// Trash contents completed loading.
    TrashLoaded {
        /// Identity of the initiating request.
        request_id: RequestId,
        /// Successful result or a displayable failure.
        result: Result<TrashContents, UiError>,
    },
    /// A trash mutation completed.
    TrashMutationFinished {
        /// Successful mutation result or a displayable failure.
        result: Result<TrashMutation, UiError>,
    },
    /// A session- and revision-identified editor save completed.
    EditorSaved {
        /// Save identity used to reject stale completion work.
        request: EditorSaveRequest,
        /// Persisted revision or a user-displayable failure.
        result: Result<Revision, UiError>,
    },
    /// A session-identified managed editor asset finished storing.
    EditorAssetStored {
        /// Editor lifetime that requested the asset.
        session: EditorSessionId,
        /// Alternative text selected when the import began.
        alt: String,
        /// Portable managed asset path or a user-displayable failure.
        result: Result<String, UiError>,
    },
}

/// Successful effects that invalidate the same dependent resources.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TrashMutation {
    /// A category was restored.
    CategoryRestored,
    /// A note was restored.
    NoteRestored,
    /// Trash was permanently emptied.
    Emptied(TrashPurgeResult),
}
