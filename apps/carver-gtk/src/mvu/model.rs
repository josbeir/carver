//! UI-neutral application state.

use carver_config::{Config, EditorMode};
use carver_sdk::{CategoryId, CategorySummary, NoteSummary, TrashContents};

/// Identifies one asynchronous resource request.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct RequestId(pub u64);

/// Identifies an editor lifetime so stale callbacks cannot affect a newer note.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct EditorSessionId(pub u64);

/// Identifies a scheduled UI timer such as a debounced search.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct TimerId(pub u64);

/// A user-visible error that does not expose an infrastructure-specific error type to views.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UiError {
    /// Short message suitable for a status page or toast.
    pub message: String,
}

impl UiError {
    /// Creates an error suitable for display to the user.
    #[must_use]
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

/// The loading status of independently rendered data.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub enum LoadState<T> {
    /// No request has started yet.
    #[default]
    Idle,
    /// A request is in flight.
    Loading(RequestId),
    /// The last request completed successfully.
    Ready(T),
    /// The last request failed and can be retried.
    Failed(UiError),
}

/// The currently visible high-level application surface.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Route {
    /// The category browser and note list.
    #[default]
    Browser,
    /// The recovery and permanent-deletion surface.
    Trash,
    /// The active note editor.
    Editor,
}

/// Browser-specific UI-neutral state.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct BrowserModel {
    /// Current untrimmed search text as entered by the user.
    pub search_query: String,
    /// Loaded note summaries for the active category and query.
    pub notes: LoadState<Vec<NoteSummary>>,
}

/// User preferences needed by the renderer.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Preferences {
    /// Whether the preview may load remote HTTP(S) images.
    pub load_remote_images: bool,
    /// The editor surface last explicitly selected by the user.
    pub editor_mode: EditorMode,
    /// Whether source mode restores its preview split.
    pub source_split_view: bool,
}

impl From<&Config> for Preferences {
    fn from(config: &Config) -> Self {
        Self {
            load_remote_images: config.images.load_remote_automatically,
            editor_mode: config.editor.last_mode,
            source_split_view: config.editor.source_split_view,
        }
    }
}

/// All persistent application state, with no GTK or `WebKit` objects.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AppModel {
    /// Current high-level surface.
    pub route: Route,
    /// Category selected by the user, or all categories when absent.
    pub selected_category: Option<CategoryId>,
    /// Categories rendered by the sidebar.
    pub sidebar: LoadState<Vec<CategorySummary>>,
    /// Browser state and its loaded note summaries.
    pub browser: BrowserModel,
    /// Recoverable deleted content.
    pub trash: LoadState<TrashContents>,
    /// Preferences used by the view and effects.
    pub preferences: Preferences,
    /// Active editor lifetime, if an editor is open.
    pub editor_session: Option<EditorSessionId>,
    next_request_id: u64,
    next_editor_session_id: u64,
}

impl AppModel {
    /// Creates a model from persisted configuration without accessing GTK or storage.
    #[must_use]
    pub fn new(config: &Config) -> Self {
        Self {
            route: Route::Browser,
            selected_category: None,
            sidebar: LoadState::Idle,
            browser: BrowserModel::default(),
            trash: LoadState::Idle,
            preferences: Preferences::from(config),
            editor_session: None,
            next_request_id: 1,
            next_editor_session_id: 1,
        }
    }

    pub(super) fn next_request_id(&mut self) -> RequestId {
        let request_id = RequestId(self.next_request_id);
        self.next_request_id = self.next_request_id.wrapping_add(1);
        request_id
    }

    pub(super) fn next_editor_session_id(&mut self) -> EditorSessionId {
        let session_id = EditorSessionId(self.next_editor_session_id);
        self.next_editor_session_id = self.next_editor_session_id.wrapping_add(1);
        session_id
    }
}
