//! Window-local command discovery, matching, and guarded activation.

use std::ops::Range;

use carver_config::EditorMode;
use carver_sdk::{BaseId, CategoryId, NoteId, SearchHit};
use nucleo_matcher::{
    Config, Matcher, Utf32Str,
    pattern::{AtomKind, CaseMatching, Normalization, Pattern},
};

use super::{
    AppModel, AppMsg, EditorSessionId, Effect, FormatCommand, LoadState, RequestId, Route, UiError,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Stable identities for commands exposed by the palette.
pub enum CommandId {
    /// Format.
    Format(FormatCommand),
    /// Heading.
    Heading(u8),
    /// Image width.
    ImageWidth(Option<u8>),
    /// Link.
    Link,
    /// Table.
    Table,
    /// Image.
    Image,
    /// Attachment.
    Attachment,
    /// Insert template.
    InsertTemplate,
    /// Find.
    Find,
    /// Mode.
    Mode(EditorMode),
    /// Sidebar.
    Sidebar,
    /// Properties.
    Properties,
    /// Favorite.
    Favorite,
    /// Copy.
    Copy,
    /// Move.
    Move,
    /// Export.
    Export,
    /// Print.
    Print,
    /// Save template.
    SaveTemplate,
    /// Trash note.
    TrashNote,
    /// Retry save.
    RetrySave,
    /// Split preview.
    SplitPreview,
    /// Search notes.
    SearchNotes,
    /// Refresh browser.
    RefreshBrowser,
    /// Import.
    Import,
    /// Clipboard.
    Clipboard,
    /// Markdown clipboard.
    MarkdownClipboard,
    /// Search base.
    SearchBase,
    /// Configure base.
    ConfigureBase,
    /// Refresh base.
    RefreshBase,
    /// Close base tabs.
    CloseBaseTabs,
    /// Delete base.
    DeleteBase,
    /// Refresh trash.
    RefreshTrash,
    /// Undo trash.
    UndoTrash,
    /// Notes.
    Notes,
    /// Trash.
    Trash,
    /// New note.
    NewNote,
    /// Blank note.
    BlankNote,
    /// Template note.
    TemplateNote,
    /// New category.
    NewCategory,
    /// New base.
    NewBase,
    /// Templates.
    Templates,
    /// Preferences.
    Preferences,
    /// Shortcuts.
    Shortcuts,
    /// Agent.
    Agent,
    /// About.
    About,
    /// Next tab.
    NextTab,
    /// Previous tab.
    PreviousTab,
    /// Close tab.
    CloseTab,
}

impl CommandId {
    pub(crate) fn destructive(self) -> bool {
        matches!(self, Self::TrashNote | Self::DeleteBase)
    }

    pub(crate) fn contextual(self) -> bool {
        !matches!(
            self,
            Self::Notes
                | Self::Trash
                | Self::NewNote
                | Self::BlankNote
                | Self::TemplateNote
                | Self::NewCategory
                | Self::NewBase
                | Self::Templates
                | Self::Preferences
                | Self::Shortcuts
                | Self::Agent
                | Self::About
                | Self::NextTab
                | Self::PreviousTab
                | Self::CloseTab
        )
    }

    pub(crate) fn edits(self) -> bool {
        matches!(
            self,
            Self::Format(_)
                | Self::Heading(_)
                | Self::ImageWidth(_)
                | Self::Link
                | Self::Table
                | Self::Image
                | Self::Attachment
                | Self::InsertTemplate
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// A command or navigation destination that can be activated.
pub enum Target {
    /// Command.
    Command(CommandId),
    /// Category.
    Category(CategoryId),
    /// Base.
    Base(BaseId),
    /// Note.
    Note(NoteId),
    /// Search all.
    SearchAll,
    /// Retry.
    Retry,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Section {
    Commands,
    Destinations,
    Notes,
}

#[derive(Clone, Debug, Eq, PartialEq)]
/// Plain-data presentation of one palette result.
pub struct Row {
    pub(crate) target: Target,
    pub(crate) title: String,
    pub(crate) subtitle: String,
    pub(crate) icon: String,
    pub(crate) shortcut: String,
    pub(crate) section: Section,
    pub(crate) disabled: bool,
    pub(crate) active: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
/// Localized search metadata supplied by the native UI.
pub struct CommandLabel {
    pub(crate) command: CommandId,
    pub(crate) title: String,
    pub(crate) aliases: String,
    pub(crate) icon: String,
    pub(crate) shortcut: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Context {
    route: Route,
    category: Option<CategoryId>,
    base: Option<BaseId>,
    editor: Option<(EditorSessionId, EditorMode, u64)>,
}

impl Context {
    fn capture(model: &AppModel) -> Self {
        Self {
            route: model.route,
            category: model.selected_category,
            base: model.bases.selected,
            editor: model
                .editor
                .as_ref()
                .map(|doc| (doc.session, doc.mode, doc.source_generation)),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
/// An activation guarded by its originating application context.
pub struct Activation {
    pub(crate) target: Target,
    pub(crate) context: Context,
    pub(crate) source_selection: Range<usize>,
    pub(crate) query: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PaletteModel {
    pub(crate) id: RequestId,
    context: Context,
    labels: Vec<CommandLabel>,
    pub(crate) source_selection: Range<usize>,
    pub(crate) query: String,
    pub(crate) local_rows: Vec<Row>,
    pub(crate) notes: LoadState<Vec<SearchHit>>,
    pub(crate) has_more: bool,
    pub(crate) selected: Option<Target>,
    pub(crate) request: RequestId,
    pub(crate) matched: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
/// User input and asynchronous completions for the command palette.
pub enum PaletteMsg {
    /// Open requested.
    OpenRequested,
    /// Opened.
    Opened {
        /// Labels.
        labels: Vec<CommandLabel>,
        /// Source selection.
        source_selection: Range<usize>,
    },
    /// Query changed.
    QueryChanged {
        /// Id.
        id: RequestId,
        /// Query.
        query: String,
    },
    /// Move.
    Move {
        /// Id.
        id: RequestId,
        /// Delta.
        delta: i8,
    },
    /// Activate.
    Activate {
        /// Id.
        id: RequestId,
        /// Target.
        target: Option<Target>,
    },
    /// Dismissed.
    Dismissed(RequestId),
    /// Execute.
    Execute(Activation),
    /// Matched.
    Matched {
        /// Id.
        id: RequestId,
        /// Request.
        request: RequestId,
        /// Rows.
        rows: Vec<Row>,
    },
    /// Search elapsed.
    SearchElapsed {
        /// Id.
        id: RequestId,
        /// Request.
        request: RequestId,
    },
    /// Notes loaded.
    NotesLoaded {
        /// Id.
        id: RequestId,
        /// Request.
        request: RequestId,
        /// Result.
        result: Result<(Vec<SearchHit>, bool), UiError>,
    },
}

fn formatting_selection(model: &AppModel) -> carver_editor_protocol::SelectionState {
    formatting_selection_at(
        model,
        model
            .palette
            .as_ref()
            .map(|palette| palette.source_selection.clone()),
    )
}

fn formatting_selection_at(
    model: &AppModel,
    source_selection: Option<Range<usize>>,
) -> carver_editor_protocol::SelectionState {
    if let Some(doc) = &model.editor
        && doc.mode == EditorMode::Source
    {
        let range = source_selection.unwrap_or(0..0);
        super::format_command::selection_from_context(doc.analysis.context_for(range))
    } else {
        model.rich_selection.clone()
    }
}

pub(crate) fn available(model: &AppModel, command: CommandId) -> bool {
    available_at(
        model,
        command,
        model
            .palette
            .as_ref()
            .map(|palette| palette.source_selection.clone()),
    )
}

fn available_at(model: &AppModel, command: CommandId, selection: Option<Range<usize>>) -> bool {
    use CommandId as C;
    let editor = model.route == Route::Editor && model.editor.is_some();
    let editable = editor
        && model
            .editor
            .as_ref()
            .is_some_and(|doc| doc.mode != EditorMode::Rendered);
    match command {
        C::Format(_)
        | C::Heading(_)
        | C::Link
        | C::Table
        | C::Image
        | C::Attachment
        | C::InsertTemplate => editable,
        C::ImageWidth(_) => {
            editable
                && formatting_selection_at(model, selection)
                    .active
                    .iter()
                    .any(|item| item == "image")
        }
        C::Find
        | C::Mode(_)
        | C::Sidebar
        | C::Properties
        | C::Favorite
        | C::Copy
        | C::Move
        | C::Export
        | C::Print
        | C::SaveTemplate
        | C::TrashNote => editor,
        C::RetrySave => {
            editor
                && model
                    .editor
                    .as_ref()
                    .is_some_and(|doc| matches!(doc.save_state, super::EditorSaveState::Failed(_)))
        }
        C::SplitPreview => {
            editor
                && model
                    .editor
                    .as_ref()
                    .is_some_and(|doc| doc.mode == EditorMode::Source)
        }
        C::SearchNotes | C::RefreshBrowser | C::Import | C::Clipboard | C::MarkdownClipboard => {
            model.route == Route::Browser
        }
        C::SearchBase | C::ConfigureBase | C::RefreshBase | C::DeleteBase => {
            model.route == Route::Base && model.bases.selected.is_some()
        }
        C::CloseBaseTabs => {
            model.route == Route::Base
                && model
                    .tabs
                    .open
                    .iter()
                    .any(|tab| Some(tab.origin) == model.bases.selected.map(super::TabOrigin::Base))
        }
        C::RefreshTrash => model.route == Route::Trash,
        C::UndoTrash => model.undo_trash_note.is_some(),
        C::NewNote | C::BlankNote | C::TemplateNote => model.can_create_note(),
        C::NextTab | C::PreviousTab => !model.tabs.open.is_empty(),
        C::CloseTab => model.tabs.active.is_some(),
        _ => true,
    }
}

fn disabled(model: &AppModel, command: CommandId) -> bool {
    (command == CommandId::Move && !matches!(model.sidebar.state, LoadState::Ready(_)))
        || (command.edits()
            && model
                .editor
                .as_ref()
                .is_some_and(|doc| doc.external_change.is_some() || doc.pending_assets > 0))
        || (matches!(command, CommandId::ConfigureBase | CommandId::DeleteBase)
            && (model.bases.saving_configuration
                || model
                    .bases
                    .selected
                    .is_some_and(|id| model.bases.deleting.contains(&id))))
}

fn active(model: &AppModel, command: CommandId) -> bool {
    match command {
        CommandId::Format(format) => formatting_selection(model)
            .active
            .iter()
            .any(|item| item == format.rich_name()),
        CommandId::Heading(level) => formatting_selection(model).heading == level,
        CommandId::ImageWidth(width) => {
            formatting_selection(model)
                .image_width
                .and_then(|width| (width != 0).then_some(width))
                == width
        }
        CommandId::Mode(mode) => model.editor.as_ref().is_some_and(|doc| doc.mode == mode),
        CommandId::Favorite => model.editor.as_ref().is_some_and(|doc| doc.is_favorite),
        CommandId::SplitPreview => model.preferences.source_split_view,
        _ => false,
    }
}

fn candidates(model: &AppModel, labels: &[CommandLabel]) -> Vec<(Row, String)> {
    let mut rows = Vec::new();
    for label in labels
        .iter()
        .filter(|label| available(model, label.command))
    {
        rows.push((
            Row {
                target: Target::Command(label.command),
                title: label.title.clone(),
                subtitle: String::new(),
                icon: label.icon.clone(),
                shortcut: label.shortcut.clone(),
                section: Section::Commands,
                disabled: disabled(model, label.command),
                active: active(model, label.command),
            },
            label.aliases.clone(),
        ));
    }
    if let LoadState::Ready(categories) = &model.sidebar.state {
        for item in categories {
            rows.push((
                Row {
                    target: Target::Category(item.category.id),
                    title: item.category.name.clone(),
                    subtitle: String::new(),
                    icon: "folder-symbolic".into(),
                    shortcut: String::new(),
                    section: Section::Destinations,
                    disabled: false,
                    active: false,
                },
                String::new(),
            ));
        }
    }
    if let LoadState::Ready(bases) = &model.bases.definitions.state {
        for base in bases {
            rows.push((
                Row {
                    target: Target::Base(base.id),
                    title: base.name.clone(),
                    subtitle: String::new(),
                    icon: "view-grid-symbolic".into(),
                    shortcut: String::new(),
                    section: Section::Destinations,
                    disabled: false,
                    active: false,
                },
                String::new(),
            ));
        }
    }
    rows
}

/// Uses a maintained matcher rather than duplicating Unicode fuzzy matching locally.
/// Matching is run on the runtime's existing worker pool, never inside a GTK callback.
pub(crate) fn match_rows(candidates: Vec<(Row, String)>, query: &str) -> Vec<Row> {
    let mut matcher = Matcher::new(Config::DEFAULT);
    let pattern = Pattern::new(
        query.trim(),
        CaseMatching::Ignore,
        Normalization::Smart,
        AtomKind::Fuzzy,
    );
    let mut buffer = Vec::new();
    let mut ranked = Vec::new();
    for (index, (row, aliases)) in candidates.into_iter().enumerate() {
        if query.trim().is_empty() && row.section == Section::Destinations {
            continue;
        }
        let score = pattern
            .score(Utf32Str::new(&row.title, &mut buffer), &mut matcher)
            .or_else(|| pattern.score(Utf32Str::new(&aliases, &mut buffer), &mut matcher));
        if let Some(score) = score {
            let title = row.title.to_lowercase();
            let query = query.trim().to_lowercase();
            let tier = if title == query {
                0
            } else if title.starts_with(&query) {
                1
            } else {
                2
            };
            let command = match row.target {
                Target::Command(command) => Some(command),
                _ => None,
            };
            let section = u8::from(row.section != Section::Commands);
            ranked.push((
                (
                    section,
                    command.is_some_and(CommandId::destructive),
                    tier,
                    std::cmp::Reverse(score),
                    command.is_some_and(|command| !command.contextual()),
                    index,
                ),
                row,
            ));
        }
    }
    ranked.sort_by_key(|(key, _)| *key);
    ranked.into_iter().map(|(_, row)| row).collect()
}

impl PaletteModel {
    pub(crate) fn rows(&self) -> Vec<Row> {
        let mut rows = self.local_rows.clone();
        if let LoadState::Ready(notes) = &self.notes {
            rows.extend(notes.iter().map(|hit| Row {
                target: Target::Note(hit.note.id),
                title: hit.note.title.clone(),
                subtitle: hit.note.excerpt.clone(),
                icon: "document-edit-symbolic".into(),
                shortcut: String::new(),
                section: Section::Notes,
                disabled: false,
                active: false,
            }));
            if self.has_more {
                rows.push(Row {
                    target: Target::SearchAll,
                    title: String::new(),
                    subtitle: String::new(),
                    icon: "system-search-symbolic".into(),
                    shortcut: String::new(),
                    section: Section::Notes,
                    disabled: false,
                    active: false,
                });
            }
        }
        if matches!(self.notes, LoadState::Failed(_)) {
            rows.push(Row {
                target: Target::Retry,
                title: String::new(),
                subtitle: String::new(),
                icon: "view-refresh-symbolic".into(),
                shortcut: String::new(),
                section: Section::Notes,
                disabled: false,
                active: false,
            });
        }
        rows
    }

    fn reconcile_selection(&mut self) {
        let rows = self.rows();
        if !rows
            .iter()
            .any(|row| Some(row.target) == self.selected && !row.disabled)
        {
            self.selected = rows
                .iter()
                .find(|row| {
                    !row.disabled
                        && (!self.query.trim().is_empty()
                            || !matches!(row.target, Target::Command(command) if command.destructive()))
                })
                .map(|row| row.target);
        }
    }
}

fn search(model: &mut AppModel) -> Vec<Effect> {
    let request = model.next_request_id();
    let Some(palette) = model.palette.as_mut() else {
        return Vec::new();
    };
    palette.request = request;
    palette.matched = false;
    // Catalog refreshes must stop admitting old destinations before the worker replies.
    palette.local_rows.clear();
    palette.notes = LoadState::Loading(request);
    palette.has_more = false;
    let id = palette.id;
    let query = palette.query.clone();
    let labels = palette.labels.clone();
    vec![
        Effect::MatchPalette {
            id,
            request,
            candidates: candidates(model, &labels),
            query: query.clone(),
        },
        Effect::SchedulePaletteSearch {
            id,
            request,
            recent: query.is_empty(),
        },
    ]
}

// CONTEXT: One pure dispatch table keeps palette lifetime and stale-request admission explicit.
#[expect(clippy::too_many_lines)]
pub(crate) fn update(model: &mut AppModel, message: PaletteMsg) -> Vec<Effect> {
    match message {
        PaletteMsg::OpenRequested if model.palette.is_none() => vec![Effect::OpenPalette],
        PaletteMsg::Opened {
            labels,
            source_selection,
        } if model.palette.is_none() => {
            let id = model.next_request_id();
            model.palette = Some(PaletteModel {
                id,
                context: Context::capture(model),
                labels,
                source_selection,
                query: String::new(),
                local_rows: Vec::new(),
                notes: LoadState::Idle,
                has_more: false,
                selected: None,
                request: id,
                matched: false,
            });
            search(model)
        }
        PaletteMsg::QueryChanged { id, query }
            if model
                .palette
                .as_ref()
                .is_some_and(|palette| palette.id == id && palette.query != query) =>
        {
            if let Some(palette) = model.palette.as_mut() {
                palette.query = query;
                palette.local_rows.clear();
                palette.selected = None;
            }
            search(model)
        }
        PaletteMsg::SearchElapsed { id, request }
            if model
                .palette
                .as_ref()
                .is_some_and(|palette| palette.id == id && palette.request == request) =>
        {
            let query = model
                .palette
                .as_ref()
                .map(|palette| palette.query.clone())
                .unwrap_or_default();
            vec![Effect::SearchPaletteNotes { id, request, query }]
        }
        PaletteMsg::Matched { id, request, rows } => {
            if let Some(palette) = model
                .palette
                .as_mut()
                .filter(|palette| palette.id == id && palette.request == request)
            {
                palette.local_rows = rows;
                palette.matched = true;
                palette.reconcile_selection();
            }
            Vec::new()
        }
        PaletteMsg::NotesLoaded {
            id,
            request,
            result,
        } => {
            if let Some(palette) = model
                .palette
                .as_mut()
                .filter(|palette| palette.id == id && palette.request == request)
            {
                palette.notes = match result {
                    Ok((notes, more)) => {
                        palette.has_more = more;
                        LoadState::Ready(notes)
                    }
                    Err(error) => LoadState::Failed(error),
                };
                if palette.matched {
                    palette.reconcile_selection();
                }
            }
            Vec::new()
        }
        PaletteMsg::Move { id, delta } => {
            if let Some(palette) = model.palette.as_mut().filter(|palette| palette.id == id) {
                let targets: Vec<_> = palette
                    .rows()
                    .iter()
                    .filter(|row| !row.disabled)
                    .map(|row| row.target)
                    .collect();
                if let Some(index) = targets
                    .iter()
                    .position(|target| Some(*target) == palette.selected)
                {
                    let index = if delta < 0 {
                        index.saturating_sub(1)
                    } else {
                        (index + 1).min(targets.len().saturating_sub(1))
                    };
                    palette.selected = targets.get(index).copied();
                }
            }
            Vec::new()
        }
        PaletteMsg::Activate { id, target } => {
            let Some(palette) = model.palette.as_ref().filter(|palette| palette.id == id) else {
                return Vec::new();
            };
            let Some(target) = target.or(palette.selected) else {
                return Vec::new();
            };
            if !palette
                .rows()
                .iter()
                .any(|row| row.target == target && !row.disabled)
            {
                return Vec::new();
            }
            if target == Target::Retry {
                return search(model);
            }
            let activation = Activation {
                target,
                context: palette.context.clone(),
                source_selection: palette.source_selection.clone(),
                query: palette.query.clone(),
            };
            model.palette = None;
            vec![Effect::FinishPalette {
                activation: Some(activation),
            }]
        }
        PaletteMsg::Dismissed(id)
            if model
                .palette
                .as_ref()
                .is_some_and(|palette| palette.id == id) =>
        {
            let palette = model.palette.take();
            let mut effects = vec![Effect::FinishPalette { activation: None }];
            if let Some(palette) = palette
                && palette.context == Context::capture(model)
                && let Some((session, EditorMode::Source, _)) = palette.context.editor
            {
                effects.push(Effect::SelectEditorSource {
                    session,
                    selection: palette.source_selection,
                });
            }
            effects
        }
        PaletteMsg::Execute(activation) => execute(model, activation),
        _ => Vec::new(),
    }
}

fn execute(model: &mut AppModel, activation: Activation) -> Vec<Effect> {
    if activation.context != Context::capture(model) {
        return Vec::new();
    }
    match activation.target {
        Target::Command(command)
            if available_at(model, command, Some(activation.source_selection.clone()))
                && !disabled(model, command) =>
        {
            vec![Effect::ExecutePaletteCommand {
                command,
                source_selection: activation.source_selection,
            }]
        }
        Target::Category(id) => super::update(
            model,
            AppMsg::Navigation(super::NavigationMsg::SelectCategory(Some(id))),
        ),
        Target::Base(id) => super::update(model, AppMsg::Bases(super::BasesMsg::Open(id))),
        Target::Note(id) => super::update(
            model,
            AppMsg::Navigation(super::NavigationMsg::OpenNote {
                note_id: id,
                intent: super::NoteOpenIntent::Default,
            }),
        ),
        Target::SearchAll => {
            let mut effects = super::update(
                model,
                AppMsg::Navigation(super::NavigationMsg::SelectCategory(None)),
            );
            effects.extend(super::update(
                model,
                AppMsg::Browser(super::BrowserMsg::SearchOpened),
            ));
            effects.extend(super::update(
                model,
                AppMsg::Browser(super::BrowserMsg::SearchChanged(activation.query)),
            ));
            effects
        }
        _ => Vec::new(),
    }
}

/// Rebuilds candidates after an external library/catalog change without changing the query.
pub(crate) fn refresh(model: &mut AppModel) -> Vec<Effect> {
    search(model)
}

#[cfg(test)]
mod tests;
