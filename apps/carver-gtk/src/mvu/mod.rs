//! Pure state transitions for the GTK frontend.
//!
//! GTK callbacks translate widget input into [`AppMsg`]. The reducer in this module updates
//! [`AppModel`] and returns typed [`Effect`] values for the runtime to execute.

mod effect;
mod model;
mod msg;
mod runtime;
mod source_edit;
mod update;

pub use effect::Effect;
pub use model::{
    ActionKey, AppModel, BrowserModel, EditorCopyRequest, EditorDocument,
    EditorExportDialogRequest, EditorExportProgress, EditorExportWarningRequest,
    EditorPdfExportRequest, EditorPreview, EditorSaveRequest, EditorSaveState, EditorSessionId,
    LoadState, MediaFile, MoveUndo, Preferences, RequestId, Resource, Route,
    SourceEditorPreferences, TimerId, UiError,
};
pub use msg::{
    ActionMsg, AppMsg, BrowserMsg, EditorExportFormat, EditorMsg, LibraryReply, NavigationMsg,
    PreferencesMsg, SidebarMsg, SourceImageTarget, TrashMsg, TrashMutation, WindowMsg,
};
pub use runtime::{AppDispatcher, AppRuntime};
pub use source_edit::{SourceCommand, SourceEdit};
pub use update::update;

#[cfg(test)]
pub(crate) mod tests;
