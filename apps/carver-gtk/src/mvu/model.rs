//! UI-neutral application state.

use std::collections::BTreeSet;

use carver_config::{Config, DocumentWidth, EditorMode, SourceSyntaxStyle};
use carver_domain::source_analysis::SourceAnalysis;
use carver_sdk::{
    CategoryId, CategorySummary, LibraryRevision, NoteId, NoteSummary, Revision, TrashContents,
};

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
    /// Update one category's name and visual identity.
    UpdateCategory(CategoryId),
    /// Trash one category.
    TrashCategory(CategoryId),
    /// Move one note away from its prior category.
    MoveNote {
        /// Note being moved.
        note_id: NoteId,
        /// Category restored by Undo.
        source_category_id: CategoryId,
    },
    /// Undo one completed note move.
    UndoMove(NoteId),
    /// Trash one note.
    TrashNote(NoteId),
    /// Change one note's favorite metadata.
    SetNoteFavorite(NoteId),
}

/// The move that remains available for a one-click Undo action.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MoveUndo {
    /// Note that can be moved back.
    pub note_id: NoteId,
    /// Category to restore.
    pub source_category_id: CategoryId,
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
    /// A saved database-style note view.
    Base,
    /// The recovery and permanent-deletion surface.
    Trash,
    /// The active note editor.
    Editor,
}

/// Saved bases and the currently visible grid.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct BasesModel {
    /// Definition request whose loading-indicator delay has elapsed.
    pub definitions_loading_elapsed: Option<RequestId>,
    /// Row request whose loading-indicator delay has elapsed.
    pub rows_loading_elapsed: Option<RequestId>,
    /// Saved definitions rendered in the sidebar.
    pub definitions: Resource<Vec<carver_sdk::BaseDefinition>>,
    /// Selected definition.
    pub selected: Option<carver_sdk::BaseId>,
    /// Rows of the selected definition.
    pub rows: Resource<Vec<carver_sdk::BaseRow>>,
}

/// A destination waiting for an active editor to finish closing.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PendingNavigation {
    /// Show the browser, optionally scoped to one category.
    Browser(Option<CategoryId>),
    /// Show one saved base.
    Base(carver_sdk::BaseId),
}

/// Browser-specific UI-neutral state.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct BrowserModel {
    /// Whether the native notes search bar is currently shown.
    pub search_open: bool,
    /// Current untrimmed search text as entered by the user.
    pub search_query: String,
    /// Loaded note summaries for the active category and query.
    pub notes: Resource<Vec<NoteSummary>>,
    /// Favorite notes rendered above the All Notes feed.
    pub favorites: Resource<Vec<NoteSummary>>,
    /// Most recent successful note list, retained while a replacement request loads.
    ///
    /// This keeps fast-reload rendering a deterministic projection of the model rather than
    /// relying on a widget's previous contents.
    pub last_ready_notes: Option<Vec<NoteSummary>>,
    /// The debounce timer authorized to reload after the latest search change.
    pub search_timer: Option<TimerId>,
    /// Browser load allowed to reveal the loading state after a short delay.
    pub loading_indicator_request: Option<RequestId>,
    /// Whether the current browser load has exceeded the loading-indicator delay.
    pub loading_indicator_visible: bool,
}

/// User preferences needed by the renderer.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Preferences {
    /// Whether the preview may load remote HTTP(S) images.
    pub load_remote_images: bool,
    /// The editor surface last explicitly selected by the user.
    pub editor_mode: EditorMode,
    /// Milliseconds to wait after an edit before starting an autosave.
    pub autosave_delay_ms: u64,
    /// Whether source mode restores its preview split.
    pub source_split_view: bool,
    /// Whether the editor shows the shared formatting toolbar.
    pub show_formatting_toolbar: bool,
    /// Source-editor-only presentation preferences.
    pub source_editor: SourceEditorPreferences,
    /// Formatted-editor and preview presentation preferences.
    pub document: DocumentPreferences,
}

/// Source-editor presentation preferences needed by the renderer.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceEditorPreferences {
    /// Whether source mode shows a line-number gutter.
    pub show_line_numbers: bool,
    /// Whether source mode highlights the line containing the cursor.
    pub highlight_current_line: bool,
    /// Visual density of Carve syntax highlighting in source mode.
    pub syntax_style: SourceSyntaxStyle,
    /// Optional Pango font description selected for source mode.
    ///
    /// `None` delegates font selection to the desktop monospace preference.
    pub font: Option<String>,
}

/// Formatted-editor and preview presentation preferences needed by the renderer.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DocumentPreferences {
    /// Optional Pango font description selected for formatted surfaces.
    ///
    /// `None` delegates font selection to the desktop document preference.
    pub font: Option<String>,
    /// Line height as a percentage of the selected font size.
    pub line_height_percent: u16,
    /// Maximum readable measure for the formatted content.
    pub width: DocumentWidth,
}

impl From<&Config> for Preferences {
    fn from(config: &Config) -> Self {
        Self {
            load_remote_images: config.images.load_remote_automatically,
            editor_mode: config.editor.last_mode,
            autosave_delay_ms: config.editor.autosave_delay_ms,
            source_split_view: config.editor.source_split_view,
            show_formatting_toolbar: config.editor.show_formatting_toolbar,
            source_editor: SourceEditorPreferences {
                show_line_numbers: config.editor.source_line_numbers,
                highlight_current_line: config.editor.source_highlight_current_line,
                syntax_style: config.editor.source_syntax_style,
                font: config.editor.source_font.clone(),
            },
            document: DocumentPreferences {
                font: config.editor.document_font.clone(),
                line_height_percent: config.editor.document_line_height_percent,
                width: config.editor.document_width,
            },
        }
    }
}

/// The save lifecycle of the active editor document.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub enum EditorSaveState {
    /// The canonical source matches the last persisted source.
    #[default]
    Clean,
    /// The canonical source has changed and needs to be persisted.
    Dirty,
    /// A snapshot of the canonical source is being persisted.
    Saving(EditorSaveRequest),
    /// The last save failed; the canonical source remains available for retry.
    Failed(UiError),
}

/// The identity and immutable source snapshot for one editor save.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EditorSaveRequest {
    /// Editor lifetime that started this save.
    pub session: EditorSessionId,
    /// Note being saved.
    pub note_id: NoteId,
    /// Revision the save must still match.
    pub expected_revision: Revision,
    /// Canonical Carve source captured for this save.
    pub source: String,
}

/// A persisted change that must be resolved before saving the local draft.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExternalChange {
    /// Another client saved a different revision.
    Edited(Revision),
    /// Another client trashed or removed the note.
    Deleted,
}

/// The UI-neutral, canonical representation of one note being edited.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EditorDocument {
    /// Lifetime identity used to reject stale editor work.
    pub session: EditorSessionId,
    /// Persisted note represented by this document.
    pub note_id: NoteId,
    /// Last persisted revision of the note.
    pub revision: Revision,
    /// Whether this note appears in the Favorites carousel.
    pub is_favorite: bool,
    /// Canonical Carve source shared by the source and rich projections.
    pub source: String,
    /// Currently selected editor surface.
    pub mode: EditorMode,
    /// Shared AST analysis for the current canonical document.
    pub analysis: std::sync::Arc<SourceAnalysis>,
    /// Monotonic identity of the current source snapshot.
    pub source_generation: u64,
    /// Heading currently selected in the editor.
    pub selected_heading: Option<usize>,
    /// Source range of the media currently selected in the editor.
    pub selected_media: Option<std::ops::Range<usize>>,
    /// Requested asset bytes; absent results represent unavailable files.
    pub media_files: std::collections::BTreeMap<String, Option<MediaFile>>,
    /// Thumbnail requirements of in-flight and cached asset detail requests.
    pub media_file_kinds: std::collections::BTreeMap<String, bool>,
    /// Current visibility of the editor's document navigation sidebar.
    pub document_sidebar: DocumentSidebarVisibility,
    /// Latest favorite state requested before the current mutation completes.
    pub(crate) pending_favorite: Option<bool>,
    /// Whether a favorite mutation is in flight for this editor document.
    pub(crate) favorite_mutation_in_flight: bool,
    /// Current persistence state of the document.
    pub save_state: EditorSaveState,
    /// External conflict awaiting explicit resolution before saving local edits.
    pub external_change: Option<ExternalChange>,
    save_timer: Option<TimerId>,
    close_requested: bool,
}

/// The canonical source snapshot currently authorized for preview rendering.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EditorPreview {
    /// Editor lifetime that produced this snapshot.
    pub session: EditorSessionId,
    /// Canonical Carve source accepted for preview rendering.
    pub source: String,
}

/// The source of a native clipboard request.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EditorCopyScope {
    /// The dedicated action requested the complete note.
    Note,
    /// The rich editor requested its current selection.
    Selection,
}

/// A one-shot request to copy canonical content through the GTK adapter.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EditorCopyRequest {
    /// Monotonic identity that allows identical repeated copy requests.
    pub request_id: u64,
    /// Editor lifetime that owns the canonical source snapshot.
    pub session: EditorSessionId,
    /// Canonical source to copy, including unsaved edits.
    pub source: String,
    /// Content scope used for user-facing completion feedback.
    pub scope: EditorCopyScope,
}

/// A requested native export dialog for the active editor snapshot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EditorExportDialogRequest {
    /// Monotonic identity used to ignore duplicate view renders.
    pub request_id: u64,
    /// Editor lifetime that owns the snapshot.
    pub session: EditorSessionId,
    /// Note that owns the managed assets.
    pub note_id: NoteId,
    /// Canonical source captured when the export started.
    pub source: String,
    /// Safe user-facing filename stem derived from the snapshot.
    pub filename_stem: String,
}

/// A completed export request waiting for warning acknowledgement.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EditorExportWarningRequest {
    /// Identity of the prepared export retained by the runtime.
    pub request_id: u64,
    /// Editor lifetime that initiated the request.
    pub session: EditorSessionId,
    /// Human-readable non-fatal conversion or asset warnings.
    pub warnings: Vec<String>,
}

/// An export retained by the runtime until it is written or discarded.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EditorExportProgress {
    /// Prepared export identity.
    pub request_id: u64,
    /// Editor session that owns the export snapshot.
    pub session: EditorSessionId,
}

/// A one-shot request for the GTK adapter to render and print a PDF export.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EditorPdfExportRequest {
    /// Monotonic identity used to ignore duplicate view renders.
    pub request_id: u64,
    /// Editor lifetime that owns the source snapshot.
    pub session: EditorSessionId,
    /// Canonical Carve source to render.
    pub source: String,
    /// Destination URI selected through GTK's file chooser.
    pub target_uri: String,
    /// Whether the native printer dialog should be shown instead of writing PDF directly.
    pub print_dialog: bool,
}

impl EditorDocument {
    pub(super) fn new(
        session: EditorSessionId,
        note_id: NoteId,
        revision: Revision,
        is_favorite: bool,
        source: String,
        mode: EditorMode,
    ) -> Self {
        let analysis = std::sync::Arc::new(SourceAnalysis::parse(&source));
        Self {
            session,
            note_id,
            revision,
            is_favorite,
            source,
            mode,
            analysis,
            source_generation: 0,
            selected_heading: None,
            selected_media: None,
            media_files: std::collections::BTreeMap::new(),
            media_file_kinds: std::collections::BTreeMap::new(),
            document_sidebar: DocumentSidebarVisibility::Hidden,
            pending_favorite: None,
            favorite_mutation_in_flight: false,
            save_state: EditorSaveState::Clean,
            external_change: None,
            save_timer: None,
            close_requested: false,
        }
    }

    pub(super) fn source_changed(&mut self, source: String) -> bool {
        if self.source != source {
            self.source = source;
            self.analysis = std::sync::Arc::new(SourceAnalysis::parse(&self.source));
            self.source_generation = self.source_generation.wrapping_add(1);
            self.selected_heading = None;
            self.selected_media = None;
            if !matches!(self.save_state, EditorSaveState::Saving(_)) {
                self.save_state = EditorSaveState::Dirty;
            }
            return true;
        }
        false
    }

    pub(super) fn accept_external_note(&mut self, note: &carver_sdk::Note) {
        self.source_changed(note.source.clone());
        self.revision = note.revision;
        self.is_favorite = note.is_favorite;
        self.save_state = EditorSaveState::Clean;
        self.external_change = None;
        self.save_timer = None;
        self.close_requested = false;
        self.pending_favorite = None;
    }

    pub(super) fn schedule_save(&mut self, timer_id: TimerId) {
        self.save_timer = Some(timer_id);
    }

    pub(super) fn begin_save(&mut self) -> Option<EditorSaveRequest> {
        if self.external_change.is_some()
            || !matches!(
                self.save_state,
                EditorSaveState::Dirty | EditorSaveState::Failed(_)
            )
        {
            return None;
        }
        self.save_timer = None;
        let request = EditorSaveRequest {
            session: self.session,
            note_id: self.note_id,
            expected_revision: self.revision,
            source: self.source.clone(),
        };
        self.save_state = EditorSaveState::Saving(request.clone());
        Some(request)
    }

    pub(super) fn is_current_timer(&self, timer_id: TimerId) -> bool {
        self.save_timer == Some(timer_id)
    }

    pub(super) fn request_close(&mut self) {
        self.close_requested = true;
    }

    pub(super) fn close_is_requested(&self) -> bool {
        self.close_requested
    }
}

/// Visibility state for the editor's document navigation sidebar.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum DocumentSidebarVisibility {
    /// The sidebar is not taking editor space.
    #[default]
    Hidden,
    /// The sidebar is visible beside the editor.
    Visible,
}

impl DocumentSidebarVisibility {
    /// Toggles visibility.
    pub const fn toggled(self) -> Self {
        match self {
            Self::Hidden => Self::Visible,
            Self::Visible => Self::Hidden,
        }
    }

    /// Returns whether the sidebar is visible.
    pub const fn is_visible(self) -> bool {
        matches!(self, Self::Visible)
    }
}

/// All persistent application state, with no GTK or `WebKit` objects.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AppModel {
    /// Full persisted configuration used to create atomic save snapshots.
    pub config: Config,
    /// Current high-level surface.
    pub route: Route,
    /// Surface restored after the current editor session closes.
    pub(crate) editor_return_route: Route,
    /// Category selected by the user, or all categories when absent.
    pub selected_category: Option<CategoryId>,
    /// Destination to show once a pending editor close has safely completed.
    pub(crate) pending_navigation: Option<PendingNavigation>,
    /// Categories rendered by the sidebar.
    pub sidebar: Resource<Vec<CategorySummary>>,
    /// Browser state and its loaded note summaries.
    pub browser: BrowserModel,
    /// Saved database-style views.
    pub bases: BasesModel,
    /// Recoverable deleted content.
    pub trash: Resource<TrashContents>,
    /// The most recent mutation error for the view to surface.
    pub notice: Option<UiError>,
    /// Mutations currently admitted by the reducer.
    pub pending_actions: BTreeSet<ActionKey>,
    /// The latest successful move that can be undone.
    pub undo_move: Option<MoveUndo>,
    /// The latest note moved to trash that can be restored with Undo.
    pub undo_trash_note: Option<NoteId>,
    /// Preferences used by the view and effects.
    pub preferences: Preferences,
    /// Active editor document, if the editor is open.
    pub editor: Option<EditorDocument>,
    /// Latest debounced editor preview snapshot.
    pub editor_preview: Option<EditorPreview>,
    /// One-shot request for the GTK adapter to copy a canonical source snapshot.
    pub editor_copy_request: Option<EditorCopyRequest>,
    /// One-shot request for the view to show native export options.
    pub editor_export_dialog_request: Option<EditorExportDialogRequest>,
    /// Prepared export that requires user acknowledgement before writing.
    pub editor_export_warning_request: Option<EditorExportWarningRequest>,
    /// Native export currently being prepared, confirmed, or written.
    pub editor_export_progress: Option<EditorExportProgress>,
    /// One-shot native PDF or print request for the current editor snapshot.
    pub editor_pdf_export_request: Option<EditorPdfExportRequest>,
    /// Monotonic revision that asks editor projections to refresh their theme.
    pub editor_theme_revision: u64,
    pub(crate) preview_timer: Option<(EditorSessionId, TimerId)>,
    /// The request currently loading a note into the editor.
    pub editor_load_request: Option<RequestId>,
    /// The editor load that should open export options after its note snapshot is ready.
    pub editor_export_after_load: Option<RequestId>,
    pub(crate) library_revision: Option<LibraryRevision>,
    pub(crate) library_revision_request: Option<LibraryRevisionRequest>,
    pub(crate) library_revision_pending: bool,
    pub(crate) editor_refresh_request: Option<RequestId>,
    pub(crate) editor_refresh_pending: bool,
    pub(crate) editor_refresh_retry: Option<EditorSessionId>,
    next_request_id: u64,
    next_editor_session_id: u64,
    next_timer_id: u64,
    next_preview_timer_id: u64,
    next_editor_copy_request_id: u64,
    next_editor_export_request_id: u64,
}

impl AppModel {
    /// Creates a model from persisted configuration without accessing GTK or storage.
    #[must_use]
    pub fn new(config: &Config) -> Self {
        Self {
            config: config.clone(),
            route: Route::Browser,
            editor_return_route: Route::Browser,
            selected_category: None,
            pending_navigation: None,
            sidebar: Resource::default(),
            browser: BrowserModel::default(),
            bases: BasesModel::default(),
            trash: Resource::default(),
            notice: None,
            pending_actions: BTreeSet::new(),
            undo_move: None,
            undo_trash_note: None,
            preferences: Preferences::from(config),
            editor: None,
            editor_preview: None,
            editor_copy_request: None,
            editor_export_dialog_request: None,
            editor_export_warning_request: None,
            editor_export_progress: None,
            editor_pdf_export_request: None,
            editor_theme_revision: 0,
            preview_timer: None,
            editor_load_request: None,
            editor_export_after_load: None,
            library_revision: None,
            library_revision_request: None,
            library_revision_pending: false,
            editor_refresh_request: None,
            editor_refresh_pending: false,
            editor_refresh_retry: None,
            next_request_id: 1,
            next_editor_session_id: 1,
            next_timer_id: 1,
            next_preview_timer_id: 1,
            next_editor_copy_request_id: 1,
            next_editor_export_request_id: 1,
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

    pub(super) fn next_preview_timer_id(&mut self) -> TimerId {
        let timer_id = TimerId(self.next_preview_timer_id);
        self.next_preview_timer_id = self.next_preview_timer_id.wrapping_add(1);
        timer_id
    }

    pub(super) fn next_editor_copy_request_id(&mut self) -> u64 {
        let request_id = self.next_editor_copy_request_id;
        self.next_editor_copy_request_id = self.next_editor_copy_request_id.wrapping_add(1);
        request_id
    }

    pub(super) fn next_editor_export_request_id(&mut self) -> u64 {
        let request_id = self.next_editor_export_request_id;
        self.next_editor_export_request_id = self.next_editor_export_request_id.wrapping_add(1);
        request_id
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct LibraryRevisionRequest {
    pub(crate) request_id: RequestId,
    pub(crate) reason: LibraryRevisionCheckReason,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum LibraryRevisionCheckReason {
    InitialLoad,
    LocalMutation,
    ExternalWakeup,
}

/// Resolved file details used by the sidebar’s Media section.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MediaFile {
    /// Original file size in bytes.
    pub size: u64,
    /// Encoded image bytes for thumbnail rendering.
    pub preview: Option<std::sync::Arc<Vec<u8>>>,
}
