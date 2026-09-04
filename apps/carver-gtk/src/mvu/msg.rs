//! Messages accepted by the application reducer.

use carver_config::EditorMode;
use carver_sdk::{
    CategoryId, CategorySummary, NoteId, NoteSummary, TrashContents, TrashPurgeResult,
};

use super::{EditorSessionId, RequestId, TimerId, UiError};

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
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EditorMsg {
    /// Open a note in a new editor lifetime.
    Open(NoteId),
    /// Close the active editor lifetime.
    Close(EditorSessionId),
}

/// Preference changes requested by a view.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PreferencesMsg {
    /// Set the remote-image loading policy.
    SetRemoteImages(bool),
    /// Set the preferred editor surface.
    SetEditorMode(EditorMode),
    /// Set source split-preview visibility.
    SetSourceSplitView(bool),
}

/// Values returned by asynchronous library effects.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LibraryReply {
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
