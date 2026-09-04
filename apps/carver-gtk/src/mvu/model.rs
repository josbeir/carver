//! UI-neutral application state.

use std::collections::BTreeSet;

use carver_config::{Config, EditorMode};
use carver_sdk::{CategoryId, CategorySummary, NoteId, NoteSummary, TrashContents};

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

/// Identifies a mutation for duplicate-action admission control.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum ActionKey {
    /// Create a category.
    CreateCategory,
    /// Rename one category.
    RenameCategory(CategoryId),
    /// Trash one category.
    TrashCategory(CategoryId),
    /// Move one note.
    MoveNote(NoteId),
    /// Trash one note.
    TrashNote(NoteId),
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

/// A loadable resource with one coalesced reload slot.
///
/// Repeated invalidations while a request is running do not allocate more work. The reducer
/// starts exactly one follow-up request after the active request completes.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Resource<T> {
    /// State rendered by a view.
    pub state: LoadState<T>,
    reload_requested: bool,
}

impl<T> Resource<T> {
    pub(super) fn begin_reload(&mut self, request_id: RequestId) -> bool {
        if matches!(self.state, LoadState::Loading(_)) {
            self.reload_requested = true;
            return false;
        }
        self.state = LoadState::Loading(request_id);
        true
    }

    pub(super) fn finish(&mut self, request_id: RequestId, result: Result<T, UiError>) -> bool {
        if !matches!(self.state, LoadState::Loading(current) if current == request_id) {
            return false;
        }
        self.state = match result {
            Ok(value) => LoadState::Ready(value),
            Err(error) => LoadState::Failed(error),
        };
        std::mem::take(&mut self.reload_requested)
    }
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
    pub notes: Resource<Vec<NoteSummary>>,
    /// The debounce timer authorized to reload after the latest search change.
    pub search_timer: Option<TimerId>,
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
    pub sidebar: Resource<Vec<CategorySummary>>,
    /// Browser state and its loaded note summaries.
    pub browser: BrowserModel,
    /// Recoverable deleted content.
    pub trash: Resource<TrashContents>,
    /// The most recent mutation error for the view to surface.
    pub notice: Option<UiError>,
    /// Mutations currently admitted by the reducer.
    pub pending_actions: BTreeSet<ActionKey>,
    /// Preferences used by the view and effects.
    pub preferences: Preferences,
    /// Active editor lifetime, if an editor is open.
    pub editor_session: Option<EditorSessionId>,
    next_request_id: u64,
    next_editor_session_id: u64,
    next_timer_id: u64,
}

impl AppModel {
    /// Creates a model from persisted configuration without accessing GTK or storage.
    #[must_use]
    pub fn new(config: &Config) -> Self {
        Self {
            route: Route::Browser,
            selected_category: None,
            sidebar: Resource::default(),
            browser: BrowserModel::default(),
            trash: Resource::default(),
            notice: None,
            pending_actions: BTreeSet::new(),
            preferences: Preferences::from(config),
            editor_session: None,
            next_request_id: 1,
            next_editor_session_id: 1,
            next_timer_id: 1,
        }
    }

    pub(super) fn next_request_id(&mut self) -> RequestId {
        let request_id = RequestId(self.next_request_id);
        self.next_request_id = self.next_request_id.wrapping_add(1);
        request_id
    }

    pub(super) fn begin_action(&mut self, action: ActionKey) -> bool {
        self.pending_actions.insert(action)
    }

    pub(super) fn finish_action(&mut self, action: ActionKey) {
        self.pending_actions.remove(&action);
    }

    pub(super) fn next_editor_session_id(&mut self) -> EditorSessionId {
        let session_id = EditorSessionId(self.next_editor_session_id);
        self.next_editor_session_id = self.next_editor_session_id.wrapping_add(1);
        session_id
    }

    pub(super) fn next_timer_id(&mut self) -> TimerId {
        let timer_id = TimerId(self.next_timer_id);
        self.next_timer_id = self.next_timer_id.wrapping_add(1);
        timer_id
    }
}
