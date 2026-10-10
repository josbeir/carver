//! Pure state transitions for the GTK frontend.
//!
//! GTK callbacks translate widget input into [`AppMsg`]. The reducer in this module updates
//! [`AppModel`] and returns typed [`Effect`] values for the runtime to execute.

mod effect;
pub(crate) mod format_command;
pub(crate) mod palette;
pub(crate) use format_command::FormatCommand;
mod media_filename;
mod model;
mod msg;
mod runtime;
mod source_edit;
mod templates;
mod update;
pub use templates::{
    InsertTarget, TemplatePreview, TemplateProperty, TemplatePropertyOrigin, TemplatePurpose,
    TemplatesMsg,
};

pub use effect::Effect;
pub use model::{
    ActionKey, AppModel, BaseCellEdit, BasesModel, BrowserModel, CategoryChoice,
    DocumentPreferences, EditorCopyRequest, EditorCopyScope, EditorDocument,
    EditorExportDialogRequest, EditorExportProgress, EditorExportWarningRequest, EditorLinkDialog,
    EditorPdfExportRequest, EditorPreview, EditorPropertiesRequest, EditorSaveRequest,
    EditorSaveState, EditorSessionId, FrontmatterEdit, LinkDialogOrigin, LoadState, MediaFile,
    MoveUndo, NoteTab, PendingBaseCellEdit, Preferences, PropertiesSave, RequestId, Resource,
    Route, SidebarSelection, SourceEditorPreferences, TabId, TabOrigin, Tabs, TimerId, UiError,
};
pub use msg::{
    ActionMsg, AppMsg, BasesMsg, BrowserMsg, EditorExportFormat, EditorMsg, ImportFileSource,
    ImportTarget, LibraryReply, NavigationMsg, NoteOpenIntent, PreferencesMsg, SidebarMsg,
    SourceImageTarget, StoredMedia, TabsMsg, TrashMsg, TrashMutation, WindowMsg,
};
pub use runtime::{AppDispatcher, AppRuntime};
pub use source_edit::{SourceCommand, SourceEdit, SourceInput, SourceInputOutcome};
pub use update::update;

pub(crate) use media_filename::safe_media_filename;

#[cfg(test)]
pub(crate) mod tests;

#[cfg(test)]
pub(crate) use runtime::tests::export_runtime_should_cover_completion_cancellation_and_failures;
#[cfg(test)]
pub(crate) use runtime::tests::runtime_error_paths_should_surface_failures;
