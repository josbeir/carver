//! UI-neutral application state.

use std::collections::{BTreeMap, BTreeSet};

use carver_config::{Config, DocumentWidth, EditorMode, SourceSyntaxStyle};
use carver_domain::source_analysis::SourceAnalysis;
use carver_sdk::{
    BaseId, CategoryId, CategorySummary, LibraryRevision, NoteId, NoteLinks, NoteSummary, Revision,
    TrashContents,
};

/// Identifies one asynchronous resource request.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct RequestId(pub u64);

/// Identifies an editor lifetime so stale callbacks cannot affect a newer note.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct EditorSessionId(pub u64);

/// Identifies one open note tab in the workspace.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TabId(pub u64);

/// The surface a note tab was opened from.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TabOrigin {
    /// Opened while browsing notes or managing the trash.
    Browser,
    /// Opened from a saved Base view.
    Base(BaseId),
}

/// Lightweight metadata for one open note tab.
///
/// The active tab's full [`EditorDocument`] lives in `AppModel.editor`; inactive tabs keep their
/// documents in `AppModel.tabs.background`, so every other part of the app still reasons
/// about exactly one active document.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NoteTab {
    /// Stable tab identity.
    pub id: TabId,
    /// Note shown by this tab.
    pub note_id: NoteId,
    /// Display title, refreshed from the note.
    pub title: String,
    /// Whether the note is currently a favorite.
    pub is_favorite: bool,
    /// Whether the note is still loading into the tab.
    pub loading: bool,
    /// Surface this tab was opened from, for context-scoped tab actions.
    pub origin: TabOrigin,
}

/// The open note tabs and the documents behind them.
///
/// The active tab's document lives in [`AppModel::editor`]; inactive tabs keep theirs in
/// [`Tabs::background`], and tabs that were closed while dirty keep saving in [`Tabs::closing`].
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Tabs {
    /// Ordered open note tabs.
    pub open: Vec<NoteTab>,
    /// Active note tab, or `None` when the Notes list is showing.
    pub active: Option<TabId>,
    /// Most recently active note tab, remembered while the Notes list is showing.
    pub(crate) last_active: Option<TabId>,
    /// Previously active tabs, newest last, for browser-style Back.
    pub(crate) history: Vec<Option<TabId>>,
    /// Documents for inactive note tabs, keyed by tab id.
    pub(crate) background: BTreeMap<TabId, EditorDocument>,
    /// Dirty documents whose tab was closed and that are still saving.
    pub(crate) closing: BTreeMap<EditorSessionId, EditorDocument>,
}

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

/// A Base configuration dialog waiting for the library-wide field catalog.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum PendingBaseConfiguration {
    /// Configure a new, not-yet-persisted Base.
    New,
    /// Configure an existing saved Base definition.
    Existing(carver_sdk::BaseDefinition),
}

/// One in-flight inline edit of a Base cell.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BaseCellEdit {
    /// Note whose frontmatter is being edited.
    pub note_id: NoteId,
    /// JSON Pointer path of the edited property.
    pub path: String,
    /// Row revision captured when the edit began.
    pub revision: Revision,
    /// Identity used to ignore a stale completion.
    pub request_id: RequestId,
}

/// A Base cell edit waiting for the same note's earlier edit to finish.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PendingBaseCellEdit {
    /// Note whose frontmatter is edited.
    pub note_id: NoteId,
    /// JSON Pointer path of the edited property.
    pub path: String,
    /// Value to persist, or `None` to clear the property.
    pub value: Option<serde_json::Value>,
}

/// Saved bases and the currently visible grid.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct BasesModel {
    /// Whether the native Base search bar is currently shown.
    pub search_open: bool,
    /// Current untrimmed Base search text as entered by the user.
    pub search_query: String,
    /// The debounce timer authorized to reload after the latest Base search change.
    pub search_timer: Option<TimerId>,
    /// Latest configuration snapshot request.
    pub configuration_request: Option<RequestId>,
    /// Dialog intent retained until the field catalog becomes ready.
    pub(crate) pending_configuration: Option<PendingBaseConfiguration>,
    /// Identity of the configuration dialog currently presented by the GTK adapter.
    pub configuration_dialog: Option<RequestId>,
    /// Latest configuration preview-count request.
    pub configuration_preview_request: Option<(RequestId, RequestId)>,
    /// Debounce timer for the latest configuration preview draft.
    pub configuration_preview_timer: Option<TimerId>,
    /// Draft retained until its preview debounce timer elapses.
    pub configuration_preview_draft: Option<(
        RequestId,
        carver_sdk::BaseFilterMode,
        Vec<carver_sdk::BaseFilter>,
    )>,
    /// Whether a configuration update is pending.
    pub saving_configuration: bool,
    /// Base deletions currently in flight.
    pub deleting: BTreeSet<carver_sdk::BaseId>,
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
    /// Offset for the next Base row page.
    pub rows_next_offset: usize,
    /// Whether another Base row page is available.
    pub rows_has_more: bool,
    /// Incremental Base row request currently in flight.
    pub rows_append_request: Option<RequestId>,
    /// Recoverable failure while loading another Base row page.
    pub rows_append_error: Option<UiError>,
    /// Typed frontmatter properties currently present in active notes.
    pub property_descriptors: Resource<Vec<carver_sdk::PropertyDescriptor>>,
    /// Inline cell edits currently being persisted.
    pub cell_edits: Vec<BaseCellEdit>,
    /// Inline cell edits queued behind another edit on the same note.
    pub pending_cell_edits: Vec<PendingBaseCellEdit>,
    /// The Base row whose document-properties dialog is being prepared.
    pub base_properties_request: Option<RequestId>,
}

/// The single navigation destination highlighted in the sidebar.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SidebarSelection {
    /// All notes (`None`) or one category.
    Category(Option<CategoryId>),
    /// One saved Base.
    Base(carver_sdk::BaseId),
    /// No category or Base is active (for example, Trash).
    None,
}

impl AppModel {
    /// Derives sidebar selection from navigation, preserving an editor's origin.
    pub fn sidebar_selection(&self) -> SidebarSelection {
        let route = if self.route == Route::Editor {
            self.editor_return_route
        } else {
            self.route
        };
        match route {
            Route::Base => self
                .bases
                .selected
                .map_or(SidebarSelection::None, SidebarSelection::Base),
            Route::Browser => SidebarSelection::Category(self.selected_category),
            _ => SidebarSelection::None,
        }
    }
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
    /// Offset for the next browser result page.
    pub next_offset: usize,
    /// Whether another browser result page is available.
    pub has_more: bool,
    /// Incremental browser request currently in flight.
    pub append_request: Option<RequestId>,
    /// Recoverable failure while loading another browser page.
    pub append_error: Option<UiError>,
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
    /// HTML rendering behavior for previews and presentation exports.
    pub html_profile: carver_domain::rendering::HtmlProfile,
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
            html_profile: carver_domain::rendering::HtmlProfile::from_enabled(
                config.editor.enhanced_carve_rendering,
            ),
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
    /// Destination category when the note should also move, applied after the content save.
    pub move_to: Option<CategoryId>,
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
    /// Category the note currently belongs to.
    pub category_id: CategoryId,
    /// Category to move the note to on the next save, when the dialog changed it.
    pub pending_category: Option<CategoryId>,
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
    /// Outgoing internal-link targets and backlinks for this note.
    pub links: Resource<NoteLinks>,
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
    /// Note that owns the managed assets referenced by the source.
    pub note_id: NoteId,
    /// Canonical source to copy, including unsaved edits.
    pub source: String,
    /// Content scope used for user-facing completion feedback.
    pub scope: EditorCopyScope,
}

/// A requested native export dialog for the active editor snapshot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EditorExportDialogRequest {
    /// HTML behavior captured with the source when export starts.
    pub html_profile: carver_domain::rendering::HtmlProfile,
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
    /// HTML behavior captured with the source when export starts.
    pub html_profile: carver_domain::rendering::HtmlProfile,
    /// Monotonic identity used to ignore duplicate view renders.
    pub request_id: u64,
    /// Editor lifetime that owns the source snapshot.
    pub session: EditorSessionId,
    /// Note that owns the managed assets referenced by the source.
    pub note_id: NoteId,
    /// Canonical Carve source to render.
    pub source: String,
    /// Destination URI selected through GTK's file chooser.
    pub target_uri: String,
    /// Whether the native printer dialog should be shown instead of writing PDF directly.
    pub print_dialog: bool,
}

/// Where the unified link dialog writes its insert.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LinkDialogOrigin {
    /// The rich editor owns the insert.
    Rich {
        /// Prefilled link label.
        text: String,
        /// Prefilled destination.
        destination: String,
    },
    /// The source editor owns the insert over a captured selection.
    Source {
        /// Character range replaced by the inserted link.
        selection: std::ops::Range<usize>,
        /// Selected source text offered as the initial link label.
        text: String,
    },
}

/// State for the unified note-link dialog's asynchronous note search.
///
/// Entry contents remain ephemeral GTK state; only the search query and its results are modeled so
/// the SDK boundary stays behind typed effects.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EditorLinkDialog {
    /// Dialog identity used to reject stale search completions.
    pub dialog_id: RequestId,
    /// Editor lifetime that opened the dialog.
    pub session: EditorSessionId,
    /// Insert target and prefill captured when the dialog opened.
    pub origin: LinkDialogOrigin,
    /// Current note-search input.
    pub query: String,
    /// Debounce timer authorized to search for the latest query.
    pub search_timer: Option<TimerId>,
    /// Matching notes for the current query.
    pub candidates: Resource<Vec<NoteSummary>>,
}

/// An active category offered for a note's category in the properties dialog.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CategoryChoice {
    /// Category identity.
    pub id: CategoryId,
    /// Category display name.
    pub name: String,
}

/// Where a document-properties dialog writes its edit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PropertiesSave {
    /// The open editor session splices the edit into canonical source.
    Editor {
        /// Editor lifetime that owns the snapshot.
        session: EditorSessionId,
    },
    /// A Base row loads the note, splices the edit, and saves under its revision.
    Base {
        /// Note being edited.
        note_id: NoteId,
        /// Row revision captured when the dialog opened.
        revision: Revision,
    },
}

/// Immutable snapshot used to open the native document-properties dialog.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EditorPropertiesRequest {
    /// Target that persists the dialog's edit.
    pub save: PropertiesSave,
    /// Persisted note being edited.
    pub note_id: NoteId,
    /// Parsed frontmatter, or `None` when the note has no block.
    pub document: Option<carver_domain::FrontmatterDocument>,
    /// Authored content between the fences, used by the raw fallback.
    pub raw: Option<String>,
    /// Plain text of the document's first heading, used to prefill the title row.
    pub heading_title: Option<String>,
    /// Configured default properties always offered as value-only rows.
    pub defaults: Vec<carver_config::DocumentProperty>,
    /// Format used when the dialog creates a new frontmatter block.
    pub default_format: carver_domain::FrontmatterFormat,
    /// The note's current category, or `None` to hide the category row.
    pub category_id: Option<CategoryId>,
    /// Active categories offered when the category row is shown.
    pub categories: Vec<CategoryChoice>,
}

/// A frontmatter edit produced by the native document-properties dialog.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FrontmatterEdit {
    /// Structured fields parsed and edited by the dialog.
    Parsed(carver_domain::FrontmatterDocument),
    /// Raw block content for malformed or complex frontmatter.
    Raw {
        /// Format selected in the dialog.
        format: carver_domain::FrontmatterFormat,
        /// Content between the fences, without them.
        content: String,
    },
}

impl EditorDocument {
    pub(super) fn new(
        session: EditorSessionId,
        note_id: NoteId,
        category_id: CategoryId,
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
            category_id,
            pending_category: None,
            is_favorite,
            source,
            mode,
            analysis,
            source_generation: 0,
            selected_heading: None,
            selected_media: None,
            media_files: std::collections::BTreeMap::new(),
            media_file_kinds: std::collections::BTreeMap::new(),
            links: Resource::default(),
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
        self.category_id = note.category_id;
        self.pending_category = None;
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
            move_to: self.pending_category.take(),
        };
        self.save_state = EditorSaveState::Saving(request.clone());
        Some(request)
    }

    pub(super) fn is_current_timer(&self, timer_id: TimerId) -> bool {
        self.save_timer == Some(timer_id)
    }

    #[cfg(test)]
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
    /// Monotonic revision that advances whenever `notice` is set.
    ///
    /// Views compare it against the last notice they surfaced so two consecutive identical
    /// notices each raise a toast instead of the second being suppressed as a duplicate.
    pub notice_revision: u64,
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
    /// Open note tabs and their documents.
    pub tabs: Tabs,
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
    /// Active unified link dialog, if one is open.
    pub editor_link_dialog: Option<EditorLinkDialog>,
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
    next_tab_id: u64,
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
            notice_revision: 0,
            pending_actions: BTreeSet::new(),
            undo_move: None,
            undo_trash_note: None,
            preferences: Preferences::from(config),
            editor: None,
            tabs: Tabs::default(),
            editor_preview: None,
            editor_copy_request: None,
            editor_export_dialog_request: None,
            editor_export_warning_request: None,
            editor_export_progress: None,
            editor_pdf_export_request: None,
            editor_link_dialog: None,
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
            next_tab_id: 1,
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

    pub(super) fn next_tab_id(&mut self) -> TabId {
        let tab_id = TabId(self.next_tab_id);
        self.next_tab_id = self.next_tab_id.wrapping_add(1);
        tab_id
    }

    /// Returns one open note tab by identity.
    #[must_use]
    pub fn note_tab(&self, tab_id: TabId) -> Option<&NoteTab> {
        self.tabs.open.iter().find(|tab| tab.id == tab_id)
    }

    pub(super) fn note_tab_mut(&mut self, tab_id: TabId) -> Option<&mut NoteTab> {
        self.tabs.open.iter_mut().find(|tab| tab.id == tab_id)
    }

    /// Returns the tab showing `note_id`, if the note is already open.
    #[must_use]
    pub fn note_tab_for_note(&self, note_id: NoteId) -> Option<TabId> {
        self.tabs
            .open
            .iter()
            .find(|tab| tab.note_id == note_id)
            .map(|tab| tab.id)
    }

    /// Whether a new note can be created from the current surface.
    ///
    /// A Base owns its own filtering, so a new note there is ambiguous; an editor opened from a
    /// Base inherits that. This single predicate drives the tab-bar action and the accelerator.
    #[must_use]
    pub fn can_create_note(&self) -> bool {
        match self.route {
            Route::Browser => true,
            Route::Editor => self.editor_return_route == Route::Browser,
            Route::Base | Route::Trash => false,
        }
    }

    /// Resolves an editor document by session across the active and background tabs.
    #[must_use]
    pub fn document_for_session(&self, session: EditorSessionId) -> Option<&EditorDocument> {
        self.editor
            .as_ref()
            .filter(|document| document.session == session)
            .or_else(|| {
                self.tabs
                    .background
                    .values()
                    .find(|document| document.session == session)
            })
            .or_else(|| self.tabs.closing.get(&session))
    }

    pub(super) fn document_for_session_mut(
        &mut self,
        session: EditorSessionId,
    ) -> Option<&mut EditorDocument> {
        if self
            .editor
            .as_ref()
            .is_some_and(|document| document.session == session)
        {
            return self.editor.as_mut();
        }
        if let Some(document) = self
            .tabs
            .background
            .values_mut()
            .find(|document| document.session == session)
        {
            return Some(document);
        }
        self.tabs.closing.get_mut(&session)
    }

    /// Resolves a document by note across the active and background tabs.
    #[must_use]
    pub fn document_for_note(&self, note_id: NoteId) -> Option<&EditorDocument> {
        self.editor
            .as_ref()
            .filter(|document| document.note_id == note_id)
            .or_else(|| {
                self.tabs
                    .background
                    .values()
                    .find(|document| document.note_id == note_id)
            })
    }

    /// Resolves a mutable document by note across the active and background tabs.
    pub(super) fn document_for_note_mut(&mut self, note_id: NoteId) -> Option<&mut EditorDocument> {
        if self
            .editor
            .as_ref()
            .is_some_and(|document| document.note_id == note_id)
        {
            return self.editor.as_mut();
        }
        self.tabs
            .background
            .values_mut()
            .find(|document| document.note_id == note_id)
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

    /// Records a user-visible notice, advancing the revision views use to detect repeats.
    pub(super) fn set_notice(&mut self, notice: UiError) {
        self.notice_revision = self.notice_revision.wrapping_add(1);
        self.notice = Some(notice);
    }

    /// Clears the current user-visible notice.
    pub(super) fn clear_notice(&mut self) {
        self.notice = None;
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
