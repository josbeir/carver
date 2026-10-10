//! Pure state transitions for the application model.

use gettextrs::gettext;

use super::model::{
    ExternalChange, LibraryRevisionCheckReason, LibraryRevisionRequest, PendingBaseConfiguration,
    PendingNavigation,
};
use super::{
    ActionKey, ActionMsg, AppModel, AppMsg, BasesMsg, BrowserMsg, EditorMsg, EditorSaveRequest,
    EditorSessionId, Effect, LibraryReply, MoveUndo, NavigationMsg, PreferencesMsg, SidebarMsg,
    SourceEdit, TrashMsg, UiError, WindowMsg,
};

/// Applies one message and returns the work a runtime must perform afterwards.
#[must_use]
pub fn update(model: &mut AppModel, message: AppMsg) -> Vec<Effect> {
    let refresh_palette = matches!(
        &message,
        AppMsg::Library(LibraryReply::SidebarLoaded { .. } | LibraryReply::BasesLoaded { .. })
    );
    let categories_ready = matches!(model.sidebar.state, super::LoadState::Ready(_));
    let command_states = super::palette::command_states(model);
    let prior_session = model.editor.as_ref().map(|doc| doc.session);
    let mut effects = dispatch(model, message);
    if prior_session != model.editor.as_ref().map(|doc| doc.session) {
        model.rich_selection = carver_editor_protocol::SelectionState::default();
    }
    if (refresh_palette
        || categories_ready != matches!(model.sidebar.state, super::LoadState::Ready(_))
        || (command_states.is_some() && command_states != super::palette::command_states(model)))
        && model.palette.is_some()
    {
        effects.extend(super::palette::refresh(model));
    }
    // Keep tab titles in step with the document while it is edited.
    sync_tab_titles(model);
    effects
}

#[must_use]
#[expect(
    clippy::too_many_lines,
    reason = "the top-level dispatch table keeps every message route visible in one place"
)]
fn dispatch(model: &mut AppModel, message: AppMsg) -> Vec<Effect> {
    let mut effects = match message {
        AppMsg::Palette(message) => super::palette::update(model, message),
        AppMsg::Templates(message) => super::templates::update(model, message),
        AppMsg::Navigation(NavigationMsg::Started) => {
            vec![Effect::EnsureDefaultCategory]
        }
        AppMsg::Navigation(NavigationMsg::SelectCategory(category_id)) => {
            select_category(model, category_id)
        }
        AppMsg::Navigation(NavigationMsg::OpenNote { note_id, intent }) => {
            open_note_tab(model, note_id, intent)
        }
        AppMsg::Navigation(NavigationMsg::ExportNote(note_id)) => {
            if let Some(tab_id) = model.note_tab_for_note(note_id) {
                let mut effects = activate_tab(model, tab_id);
                effects.extend(request_editor_export_dialog(model));
                effects
            } else {
                create_note_tab(model, note_id, current_tab_origin(model), false, true)
            }
        }
        AppMsg::Navigation(NavigationMsg::CreateNote) => {
            if let Some(category_id) = model.active_category_id() {
                vec![Effect::CreateCategoryNote { category_id }]
            } else {
                Vec::new()
            }
        }
        AppMsg::Navigation(NavigationMsg::CreateBlankNote) => create_note_effect(model),
        AppMsg::Navigation(NavigationMsg::ImportNote { format, source }) => {
            import_note_effect(model, format, source)
        }
        AppMsg::Navigation(NavigationMsg::CreateNoteFromClipboard {
            text,
            intent,
            category_id,
        }) => create_note_from_clipboard_effect(model, &text, intent, category_id),
        AppMsg::Navigation(NavigationMsg::ImportFailed(message)) => {
            model.set_notice(UiError::new(message));
            Vec::new()
        }
        AppMsg::Navigation(NavigationMsg::ShowTrash) => {
            model.route = super::Route::Trash;
            reload_trash(model).into_iter().collect()
        }
        AppMsg::Navigation(NavigationMsg::ShowBrowser) => {
            model.route = super::Route::Browser;
            Vec::new()
        }
        AppMsg::Navigation(NavigationMsg::SearchShortcutRequested) => match model.route {
            super::Route::Browser => open_browser_search(model),
            super::Route::Base => open_base_search(model),
            super::Route::Trash | super::Route::Editor => Vec::new(),
        },
        AppMsg::Browser(message) => update_browser(model, message),
        AppMsg::Sidebar(SidebarMsg::Reload) => reload_sidebar(model).into_iter().collect(),
        AppMsg::Trash(TrashMsg::Reload) => reload_trash(model).into_iter().collect(),
        AppMsg::Trash(TrashMsg::RestoreCategory(category_id)) => {
            vec![Effect::RestoreCategory { category_id }]
        }
        AppMsg::Trash(TrashMsg::RestoreNote(note_id)) => {
            if model.undo_trash_note == Some(note_id) {
                model.undo_trash_note = None;
            }
            vec![Effect::RestoreNote { note_id }]
        }
        AppMsg::Trash(TrashMsg::Empty) => vec![Effect::EmptyTrash],
        AppMsg::Editor(message) => update_editor(model, message),
        AppMsg::Tabs(message) => update_tabs(model, message),
        AppMsg::Preferences(preference) => update_preferences(model, preference),
        AppMsg::Window(WindowMsg::SaveGeometry {
            width,
            height,
            maximized,
        }) => {
            model.config.window.width = width;
            model.config.window.height = height;
            model.config.window.maximized = maximized;
            persist_config_effect(model)
        }
        AppMsg::Action(action) => update_action(model, action),
        AppMsg::LibraryChangedExternally => {
            request_library_revision(model, LibraryRevisionCheckReason::ExternalWakeup)
                .into_iter()
                .collect()
        }
        AppMsg::Library(reply) => update_library(model, reply),
        AppMsg::Bases(message) => update_bases(model, message),
    };
    resume_editor_refresh(model, &mut effects);
    if let Some(document) = model.editor.as_mut()
        && document.document_sidebar.is_visible()
    {
        let mut requested = std::collections::BTreeMap::new();
        for media in document.analysis.media() {
            let image = media.kind == carver_domain::source_analysis::MediaKind::Image;
            requested
                .entry(media.path.clone())
                .and_modify(|value| *value |= image)
                .or_insert(image);
        }
        document
            .media_files
            .retain(|path, _| requested.contains_key(path));
        document
            .media_file_kinds
            .retain(|path, _| requested.contains_key(path));
        for (path, image) in requested {
            if document.media_file_kinds.get(&path) != Some(&image) {
                document.media_file_kinds.insert(path.clone(), image);
                document.media_files.insert(path.clone(), None);
                effects.push(Effect::LoadMediaFile {
                    session: document.session,
                    note_id: document.note_id,
                    path,
                    image,
                });
            }
        }
    }
    effects
}

// CONTEXT: Keeping related Base transitions in one reducer branch makes the MVU workflow auditable.
#[expect(
    clippy::too_many_lines,
    reason = "Base transitions share navigation state"
)]
fn update_bases(model: &mut AppModel, message: BasesMsg) -> Vec<Effect> {
    match message {
        BasesMsg::SearchShortcutRequested if model.route == super::Route::Base => {
            open_base_search(model)
        }
        BasesMsg::SearchOpened => open_base_search(model),
        BasesMsg::SearchVisibilityChanged(visible) => update_base_search_visibility(model, visible),
        BasesMsg::SearchChanged(query) => {
            if model.bases.search_query == query {
                return Vec::new();
            }
            model.bases.search_query = query;
            let timer_id = model.next_timer_id();
            model.bases.search_timer = Some(timer_id);
            vec![Effect::ScheduleBaseSearch { timer_id }]
        }
        BasesMsg::SearchTimerFired(timer_id) if model.bases.search_timer == Some(timer_id) => {
            model.bases.search_timer = None;
            model
                .bases
                .selected
                .and_then(|base_id| reload_base_rows(model, base_id))
                .into_iter()
                .collect()
        }
        BasesMsg::SetSorts { sorts } => save_base_sorts(model, sorts),
        BasesMsg::Open(base_id) => {
            if let Some(effects) = editor_external_change_effects(model) {
                return effects;
            }
            if model.tabs.active.is_some() {
                stash_active_document(model);
                record_tab_history(model);
                model.tabs.active = None;
                model.editor = None;
                model.editor_preview = None;
                model.preview_timer = None;
                model.editor_link_dialog = None;
            }
            model.editor_return_route = super::Route::Base;
            open_base(model, base_id)
        }
        BasesMsg::CreateConfigured {
            name,
            columns,
            filter_mode,
            filters,
            sorts,
            view,
        } => {
            let name = name.trim().to_owned();
            if name.is_empty() {
                model.set_notice(UiError::new("Base names cannot be empty."));
                Vec::new()
            } else {
                if model.bases.saving_configuration {
                    return Vec::new();
                }
                model.bases.saving_configuration = true;
                vec![Effect::CreateConfiguredBase {
                    name,
                    columns,
                    filter_mode,
                    filters,
                    sorts,
                    view,
                }]
            }
        }
        BasesMsg::ConfigureNew => {
            if model.bases.configuration_request.is_some()
                || model.bases.configuration_dialog.is_some()
                || model.bases.saving_configuration
            {
                return Vec::new();
            }
            request_base_configuration(model, PendingBaseConfiguration::New)
        }
        BasesMsg::CancelNewConfiguration => {
            if model.bases.configuration_dialog.is_none()
                && !matches!(
                    model.bases.pending_configuration,
                    Some(PendingBaseConfiguration::Existing(_))
                )
            {
                model.bases.configuration_request = None;
                model.bases.pending_configuration = None;
            }
            Vec::new()
        }
        BasesMsg::Configure => {
            if model.bases.saving_configuration
                || model.bases.configuration_dialog.is_some()
                || model.route != super::Route::Base
            {
                return Vec::new();
            }
            let super::LoadState::Ready(definitions) = &model.bases.definitions.state else {
                return Vec::new();
            };
            let Some(definition) = definitions
                .iter()
                .find(|base| Some(base.id) == model.bases.selected)
                .cloned()
            else {
                return Vec::new();
            };
            request_base_configuration(model, PendingBaseConfiguration::Existing(definition))
        }
        BasesMsg::Update {
            base_id,
            revision,
            name,
            columns,
            filter_mode,
            filters,
            sorts,
            view,
        } => {
            let name = name.trim().to_owned();
            if name.is_empty() {
                model.set_notice(UiError::new("Base names cannot be empty."));
                Vec::new()
            } else {
                if model.bases.saving_configuration {
                    return Vec::new();
                }
                model.bases.saving_configuration = true;
                vec![Effect::UpdateBase {
                    base_id,
                    revision,
                    name,
                    columns,
                    filter_mode,
                    filters,
                    sorts,
                    view,
                }]
            }
        }
        BasesMsg::Reload => reload_bases(model).into_iter().collect(),
        BasesMsg::PreviewCount {
            dialog_id,
            filter_mode,
            filters,
        } => {
            if model.bases.configuration_dialog != Some(dialog_id) {
                return Vec::new();
            }
            let timer_id = model.next_timer_id();
            model.bases.configuration_preview_request = None;
            model.bases.configuration_preview_timer = Some(timer_id);
            model.bases.configuration_preview_draft = Some((dialog_id, filter_mode, filters));
            vec![Effect::ScheduleBasePreview { timer_id }]
        }
        BasesMsg::PreviewCountTimerFired(timer_id)
            if model.bases.configuration_preview_timer == Some(timer_id) =>
        {
            model.bases.configuration_preview_timer = None;
            let Some((dialog_id, filter_mode, filters)) =
                model.bases.configuration_preview_draft.take()
            else {
                return Vec::new();
            };
            if model.bases.configuration_dialog != Some(dialog_id) {
                return Vec::new();
            }
            let request_id = model.next_request_id();
            model.bases.configuration_preview_request = Some((dialog_id, request_id));
            vec![Effect::PreviewBaseRowCount {
                dialog_id,
                request_id,
                filter_mode,
                filters,
            }]
        }
        BasesMsg::ConfigurationDismissed(dialog_id) => {
            if model.bases.configuration_dialog == Some(dialog_id) {
                model.bases.configuration_dialog = None;
                model.bases.configuration_preview_request = None;
                model.bases.configuration_preview_timer = None;
                model.bases.configuration_preview_draft = None;
            }
            Vec::new()
        }
        BasesMsg::LoadMoreRows => load_more_base_rows(model).into_iter().collect(),
        BasesMsg::CommitCellEdit {
            note_id,
            path,
            revision,
            value,
        } => commit_base_cell_edit(model, note_id, path, revision, value),
        BasesMsg::EditProperties { note_id, revision } => {
            if model.route != super::Route::Base || model.bases.base_properties_request.is_some() {
                return Vec::new();
            }
            let request_id = model.next_request_id();
            model.bases.base_properties_request = Some(request_id);
            let categories = active_category_choices(model);
            vec![Effect::LoadBaseProperties {
                request_id,
                note_id,
                revision,
                defaults: model.config.document_properties.entries.clone(),
                format: model.config.document_properties.format,
                categories,
            }]
        }
        BasesMsg::ApplyProperties {
            note_id,
            revision,
            edit,
            category,
        } => {
            if model.route != super::Route::Base || model.bases.selected.is_none() {
                return Vec::new();
            }
            let request_id = model.next_request_id();
            model.bases.cell_edits.push(super::BaseCellEdit {
                note_id,
                path: String::new(),
                revision,
                request_id,
            });
            vec![Effect::SaveBaseProperties {
                request_id,
                note_id,
                revision,
                edit,
                category,
            }]
        }
        BasesMsg::Delete(base_id) => {
            if model.bases.deleting.insert(base_id) {
                vec![Effect::DeleteBase { base_id }]
            } else {
                Vec::new()
            }
        }
        BasesMsg::LoadingIndicatorElapsed(request_id) => {
            if model.bases.definitions.state == super::LoadState::Loading(request_id) {
                model.bases.definitions_loading_elapsed = Some(request_id);
            }
            if model.bases.rows.state == super::LoadState::Loading(request_id) {
                model.bases.rows_loading_elapsed = Some(request_id);
            }
            Vec::new()
        }
        BasesMsg::PreviewCountTimerFired(_)
        | BasesMsg::SearchShortcutRequested
        | BasesMsg::SearchTimerFired(_) => Vec::new(),
    }
}

fn request_base_configuration(
    model: &mut AppModel,
    target: PendingBaseConfiguration,
) -> Vec<Effect> {
    let request_id = model.next_request_id();
    model.bases.configuration_request = Some(request_id);
    match &model.bases.property_descriptors.state {
        super::LoadState::Ready(_) => prepare_base_configuration(request_id, target),
        super::LoadState::Loading(_) => {
            model.bases.pending_configuration = Some(target);
            Vec::new()
        }
        super::LoadState::Idle | super::LoadState::Failed(_) => {
            model.bases.pending_configuration = Some(target);
            reload_property_descriptors(model).into_iter().collect()
        }
    }
}

fn present_base_configuration_dialog(model: &mut AppModel, dialog_id: super::RequestId) {
    model.bases.configuration_dialog = Some(dialog_id);
    model.bases.configuration_preview_request = None;
    model.bases.configuration_preview_timer = None;
    model.bases.configuration_preview_draft = None;
}

fn prepare_base_configuration(
    request_id: super::RequestId,
    target: PendingBaseConfiguration,
) -> Vec<Effect> {
    match target {
        PendingBaseConfiguration::New => vec![Effect::PrepareNewBaseConfiguration { request_id }],
        PendingBaseConfiguration::Existing(definition) => {
            vec![Effect::PrepareBaseConfiguration {
                request_id,
                definition,
            }]
        }
    }
}

fn resume_editor_refresh(model: &mut AppModel, effects: &mut Vec<Effect>) {
    if model.editor_refresh_pending
        && model.editor_refresh_request.is_none()
        && model.editor.as_ref().is_none_or(|document| {
            !matches!(document.save_state, super::EditorSaveState::Saving(_))
                && !document.favorite_mutation_in_flight
        })
    {
        model.editor_refresh_pending = false;
        effects.extend(refresh_open_editor(model, false));
    }
}

/// Opens a note according to the requested intent and the configured default.
fn open_note_tab(
    model: &mut AppModel,
    note_id: carver_sdk::NoteId,
    intent: super::NoteOpenIntent,
) -> Vec<Effect> {
    // Focus an already open note unless a background open was requested.
    if let Some(tab_id) = model.note_tab_for_note(note_id) {
        return match intent {
            super::NoteOpenIntent::Background => Vec::new(),
            super::NoteOpenIntent::Default | super::NoteOpenIntent::NewTab => {
                activate_tab(model, tab_id)
            }
        };
    }
    let origin = current_tab_origin(model);
    if matches!(intent, super::NoteOpenIntent::Background) {
        return create_note_tab(model, note_id, origin, true, false);
    }
    // The preference applies to a plain open; a forced new tab always wins.
    if matches!(intent, super::NoteOpenIntent::Default)
        && model.config.editor.note_open_behavior == carver_config::NoteOpenBehavior::CurrentTab
        && let Some(tab_id) = reusable_note_tab(model)
    {
        // Closing saves a dirty note in the background and removes its tab. If it refuses to
        // close (an unresolved external change), keep the old tab and do not open a replacement.
        let mut effects = close_tab(model, tab_id);
        if model.note_tab(tab_id).is_none() {
            effects.extend(create_note_tab(model, note_id, origin, false, false));
        }
        return effects;
    }
    create_note_tab(model, note_id, origin, false, false)
}

/// The tab a "Current tab" open should replace.
///
/// Prefers the active note tab, then the most recently active one remembered
/// while the Notes list was showing, then the newest open note tab.
fn reusable_note_tab(model: &AppModel) -> Option<super::TabId> {
    model
        .tabs
        .active
        .filter(|tab_id| model.note_tab(*tab_id).is_some())
        .or_else(|| {
            model
                .tabs
                .last_active
                .filter(|tab_id| model.note_tab(*tab_id).is_some())
        })
        .or_else(|| model.tabs.open.last().map(|tab| tab.id))
}

/// The surface a newly opened note tab belongs to.
///
/// Links and backlinks inherit the current editor's origin so a note opened
/// from a Base session stays attributed to that Base.
fn current_tab_origin(model: &AppModel) -> super::TabOrigin {
    if model.route == super::Route::Editor
        && let Some(tab) = model.tabs.active.and_then(|tab_id| model.note_tab(tab_id))
    {
        return tab.origin;
    }
    match model.route {
        super::Route::Base => model
            .bases
            .selected
            .map_or(super::TabOrigin::Browser, super::TabOrigin::Base),
        _ => super::TabOrigin::Browser,
    }
}

/// Adds a note tab and returns its identity.
fn push_note_tab(
    model: &mut AppModel,
    note_id: carver_sdk::NoteId,
    origin: super::TabOrigin,
    title: String,
    is_favorite: bool,
    loading: bool,
) -> super::TabId {
    let tab_id = model.next_tab_id();
    model.tabs.open.push(super::NoteTab {
        id: tab_id,
        note_id,
        title,
        is_favorite,
        loading,
        origin,
    });
    tab_id
}

/// Records the outgoing tab, stashes its document, and selects `tab_id`.
///
/// The caller installs the tab's document (or a loading placeholder) afterwards.
fn begin_tab_switch(model: &mut AppModel, tab_id: super::TabId) {
    if model.route != super::Route::Editor {
        model.editor_return_route = model.route;
    }
    record_tab_history(model);
    stash_active_document(model);
    model.tabs.active = Some(tab_id);
    model.tabs.last_active = Some(tab_id);
    model.editor_preview = None;
    model.preview_timer = None;
    model.editor_link_dialog = None;
    model.route = super::Route::Editor;
}

/// Keeps each tab's title in step with the document behind it while editing.
fn sync_tab_titles(model: &mut AppModel) {
    let open = &mut model.tabs.open;
    if let Some(document) = model.editor.as_ref()
        && let Some(tab) = open.iter_mut().find(|tab| tab.note_id == document.note_id)
    {
        let title = document.analysis.title();
        if tab.title != title {
            tab.title.clear();
            tab.title.push_str(title);
        }
    }
    for (tab_id, document) in &model.tabs.background {
        if let Some(tab) = open.iter_mut().find(|tab| tab.id == *tab_id) {
            let title = document.analysis.title();
            if tab.title != title {
                tab.title.clear();
                tab.title.push_str(title);
            }
        }
    }
}

/// Creates a new note tab and loads the note into it.
fn create_note_tab(
    model: &mut AppModel,
    note_id: carver_sdk::NoteId,
    origin: super::TabOrigin,
    background: bool,
    export_after_load: bool,
) -> Vec<Effect> {
    let tab_id = push_note_tab(model, note_id, origin, String::new(), false, true);
    // Remember the surface this tab was opened from, even for a background open.
    if model.route != super::Route::Editor {
        model.editor_return_route = model.route;
    }
    let request_id = model.next_request_id();
    if background {
        return vec![Effect::LoadEditorNote {
            request_id,
            tab_id,
            note_id,
        }];
    }
    begin_tab_switch(model, tab_id);
    model.editor = None;
    model.editor_load_request = Some(request_id);
    model.editor_export_after_load = export_after_load.then_some(request_id);
    vec![Effect::LoadEditorNote {
        request_id,
        tab_id,
        note_id,
    }]
}

/// Opens an already-loaded note as a new active tab.
fn open_note_as_active_tab(
    model: &mut AppModel,
    note: &carver_sdk::Note,
) -> super::EditorSessionId {
    let origin = current_tab_origin(model);
    let tab_id = push_note_tab(
        model,
        note.id,
        origin,
        note.title.clone(),
        note.is_favorite,
        false,
    );
    begin_tab_switch(model, tab_id);
    open_editor(
        model,
        note.id,
        note.category_id,
        note.revision,
        note.is_favorite,
        note.source.clone(),
        super::model::DocumentOrigin::Library,
    )
}

/// Moves the active document into its background tab slot.
fn stash_active_document(model: &mut AppModel) {
    let Some(active) = model.tabs.active else {
        return;
    };
    if let Some(document) = model.editor.take() {
        model.tabs.background.insert(active, document);
    }
}

/// Maximum number of remembered tabs for back navigation.
const MAX_TAB_HISTORY: usize = 64;

/// Records the outgoing tab so browser-style Back can return to it.
fn record_tab_history(model: &mut AppModel) {
    model.tabs.history.push(model.tabs.active);
    if model.tabs.history.len() > MAX_TAB_HISTORY {
        let overflow = model.tabs.history.len() - MAX_TAB_HISTORY;
        model.tabs.history.drain(0..overflow);
    }
}

/// Makes one note tab active, recording the previous tab.
fn activate_tab(model: &mut AppModel, tab_id: super::TabId) -> Vec<Effect> {
    if model.note_tab(tab_id).is_none() || tab_is_active(model, tab_id) {
        return Vec::new();
    }
    record_tab_history(model);
    show_tab(model, tab_id)
}

/// Whether a note tab is both selected and actually showing the editor.
///
/// Showing the Notes list keeps `active_tab` as the tab to return to, so a
/// matching `active_tab` alone must not suppress switching back to it.
fn tab_is_active(model: &AppModel, tab_id: super::TabId) -> bool {
    model.tabs.active == Some(tab_id) && model.route == super::Route::Editor
}

/// Makes one note tab active without touching the back history.
fn show_tab(model: &mut AppModel, tab_id: super::TabId) -> Vec<Effect> {
    if model.note_tab(tab_id).is_none() || tab_is_active(model, tab_id) {
        return Vec::new();
    }
    if model.route != super::Route::Editor {
        model.editor_return_route = model.route;
    }
    stash_active_document(model);
    model.editor = model.tabs.background.remove(&tab_id);
    model.tabs.active = Some(tab_id);
    model.tabs.last_active = Some(tab_id);
    model.route = super::Route::Editor;
    // A restored tab already carries its document, so publish its own preview now. Clearing to
    // `None` here would leave an active Rendered/Source-split tab blank until its next edit.
    // A tab whose background load is still in flight has no document yet; its preview arrives
    // through `open_editor` when the load completes.
    model.editor_preview = model.editor.as_ref().map(|document| super::EditorPreview {
        session: document.session,
        source: document.source.clone(),
    });
    model.preview_timer = None;
    model.editor_link_dialog = None;
    let mut effects = Vec::new();
    let refresh_from_library = model
        .editor
        .as_ref()
        .is_some_and(|document| document.origin == super::model::DocumentOrigin::Library);
    if let Some(session) = model.editor.as_ref().map(|document| document.session) {
        effects.push(Effect::FocusEditor { session });
        if model.editor.as_ref().is_some_and(|document| {
            matches!(
                document.save_state,
                super::EditorSaveState::Dirty | super::EditorSaveState::Failed(_)
            )
        }) {
            effects.extend(schedule_editor_save(model));
        }
        effects.extend(reload_note_links(model));
        // A background tab may have changed externally while it was hidden; refresh it now.
        if refresh_from_library {
            effects.extend(refresh_open_editor(model, false));
        }
    }
    effects
}

/// Moves to the neighbouring page in the visible strip, wrapping to the pinned root page.
///
/// The strip is `[root, note tabs…]`, where the root is whichever surface the pinned page
/// currently hosts: the Notes list, a Base, or Trash. Switching here does not touch the Back
/// history, unlike clicking a tab.
fn activate_relative_page(model: &mut AppModel, step: isize) -> Vec<Effect> {
    let pages = model.tabs.open.len() + 1;
    let current = if model.route == super::Route::Editor {
        model
            .tabs
            .active
            .and_then(|tab_id| model.tabs.open.iter().position(|tab| tab.id == tab_id))
            .map_or(0, |index| index + 1)
    } else {
        0
    };
    let target = if step < 0 {
        if current == 0 { pages - 1 } else { current - 1 }
    } else if current + 1 == pages {
        0
    } else {
        current + 1
    };
    if target == current {
        return Vec::new();
    }
    if target == 0 {
        return show_notes(model);
    }
    let tab_id = model.tabs.open.get(target - 1).map(|tab| tab.id);
    tab_id.map_or_else(Vec::new, |tab_id| show_tab(model, tab_id))
}

/// Returns to the previously active tab, falling back to the Notes list.
fn navigate_back(model: &mut AppModel) -> Vec<Effect> {
    loop {
        match model.tabs.history.pop() {
            Some(Some(tab_id)) if model.note_tab(tab_id).is_some() => {
                return show_tab(model, tab_id);
            }
            Some(None) | None => return show_notes(model),
            Some(Some(_)) => {}
        }
    }
}

/// Shows the external-edit resolver when the active note conflicts with an external change.
fn editor_external_change_effects(model: &AppModel) -> Option<Vec<Effect>> {
    let document = model.editor.as_ref()?;
    let change = document.external_change.as_ref()?;
    Some(vec![Effect::ShowExternalEdit {
        session: document.session,
        deleted: matches!(change, ExternalChange::Deleted),
    }])
}

/// Shows the pinned Notes tab, recording the previous tab.
fn activate_notes_tab(model: &mut AppModel) -> Vec<Effect> {
    if model.tabs.active.is_none() {
        return Vec::new();
    }
    record_tab_history(model);
    show_notes(model)
}

/// Shows the pinned Notes tab, restoring the surface behind the editor.
fn show_notes(model: &mut AppModel) -> Vec<Effect> {
    if model.tabs.active.is_none() {
        return Vec::new();
    }
    if let Some(effects) = editor_external_change_effects(model) {
        return effects;
    }
    stash_active_document(model);
    model.tabs.active = None;
    model.editor = None;
    model.editor_preview = None;
    model.preview_timer = None;
    model.editor_link_dialog = None;
    restore_editor_origin(model);
    reload_return_surface(model)
}

/// Handles workspace tab intent.
#[expect(
    clippy::needless_pass_by_value,
    reason = "the tab message mirrors the AppMsg contract and stays a plain owned enum"
)]
fn update_tabs(model: &mut AppModel, message: super::TabsMsg) -> Vec<Effect> {
    match message {
        super::TabsMsg::OpenNote { note_id, intent } => open_note_tab(model, note_id, intent),
        super::TabsMsg::Activate(tab_id) => activate_tab(model, tab_id),
        super::TabsMsg::ActivateNext => activate_relative_page(model, 1),
        super::TabsMsg::ActivatePrevious => activate_relative_page(model, -1),
        super::TabsMsg::Close(tab_id) => close_tab(model, tab_id),
        super::TabsMsg::CloseActive => {
            let active = model.tabs.active;
            active.map_or_else(Vec::new, |tab_id| close_tab(model, tab_id))
        }
        super::TabsMsg::Reordered { tab_id, position } => reorder_tab(model, tab_id, position),
        super::TabsMsg::CloseOrigin(origin) => close_tabs_from_origin(model, origin),
        super::TabsMsg::ActivateNotes => activate_notes_tab(model),
    }
}

/// Closes every note tab opened from one surface, saving dirty notes in the background.
fn close_tabs_from_origin(model: &mut AppModel, origin: super::TabOrigin) -> Vec<Effect> {
    let tab_ids: Vec<super::TabId> = model
        .tabs
        .open
        .iter()
        .filter(|tab| tab.origin == origin)
        .map(|tab| tab.id)
        .collect();
    let mut effects = Vec::new();
    for tab_id in tab_ids {
        effects.extend(close_tab(model, tab_id));
    }
    effects
}

/// Closes one note tab immediately; a dirty document keeps saving in the background.
fn close_tab(model: &mut AppModel, tab_id: super::TabId) -> Vec<Effect> {
    let Some(index) = model.tabs.open.iter().position(|tab| tab.id == tab_id) else {
        return Vec::new();
    };
    let was_active = model.tabs.active == Some(tab_id);
    // A document with an unresolved external change cannot autosave; bring it to the front and show
    // the resolver instead of parking the draft somewhere with no way back. This applies to a
    // background tab too, where an earlier conflict may already be set.
    let unresolved_change = if was_active {
        model
            .editor
            .as_ref()
            .and_then(|document| document.external_change)
    } else {
        model
            .tabs
            .background
            .get(&tab_id)
            .and_then(|document| document.external_change)
    };
    if unresolved_change.is_some() {
        let mut effects = if was_active {
            Vec::new()
        } else {
            show_tab(model, tab_id)
        };
        effects.extend(editor_external_change_effects(model).unwrap_or_default());
        return effects;
    }
    let document = if was_active {
        model.editor.take()
    } else {
        model.tabs.background.remove(&tab_id)
    };
    model.tabs.open.retain(|tab| tab.id != tab_id);
    model.tabs.history.retain(|entry| *entry != Some(tab_id));

    let mut effects = Vec::new();
    if let Some(mut document) = document {
        let dirty = matches!(
            document.save_state,
            super::EditorSaveState::Dirty
                | super::EditorSaveState::Saving(_)
                | super::EditorSaveState::Failed(_)
        );
        if dirty && let Some(request) = document.begin_save() {
            effects.push(Effect::SaveNote { request });
        }
        // Keep a clean document too while a managed-asset store is in flight, so its completion
        // can still insert the markup and schedule the save.
        if dirty || document.pending_assets > 0 {
            model.tabs.closing.insert(document.session, document);
        }
    }

    if !was_active {
        return effects;
    }
    model.tabs.active = None;
    model.editor = None;
    model.editor_preview = None;
    model.preview_timer = None;
    model.editor_link_dialog = None;
    if model.tabs.open.is_empty() {
        restore_editor_origin(model);
        effects.extend(reload_return_surface(model));
        return effects;
    }
    let neighbor = model
        .tabs
        .open
        .get(index.min(model.tabs.open.len() - 1))
        .map(|tab| tab.id);
    if let Some(neighbor) = neighbor {
        effects.extend(show_tab(model, neighbor));
    }
    effects
}

/// Removes a tab and activates a neighbor, or the Notes list when none remain.
fn remove_note_tab(model: &mut AppModel, tab_id: super::TabId) -> Vec<Effect> {
    let index = model
        .tabs
        .open
        .iter()
        .position(|tab| tab.id == tab_id)
        .unwrap_or(0);
    model.tabs.open.retain(|tab| tab.id != tab_id);
    model.tabs.background.remove(&tab_id);
    model.tabs.history.retain(|entry| *entry != Some(tab_id));
    if model.tabs.active != Some(tab_id) {
        return Vec::new();
    }
    model.tabs.active = None;
    model.editor = None;
    model.editor_preview = None;
    model.preview_timer = None;
    model.editor_link_dialog = None;
    if model.tabs.open.is_empty() {
        restore_editor_origin(model);
        return reload_return_surface(model);
    }
    let neighbor = model
        .tabs
        .open
        .get(index.min(model.tabs.open.len() - 1))
        .map(|tab| tab.id);
    // Activating the neighbor is automatic, so it does not become Back history.
    neighbor.map_or_else(Vec::new, |tab_id| show_tab(model, tab_id))
}

/// Mirrors a user-driven tab reorder into the model.
///
/// `AdwTabView` is authoritative for tab order: it emits `page-reordered`, and this keeps
/// `tabs.open` in the same order so neighbor selection on close stays visually correct.
fn reorder_tab(model: &mut AppModel, tab_id: super::TabId, position: usize) -> Vec<Effect> {
    let Some(index) = model.tabs.open.iter().position(|tab| tab.id == tab_id) else {
        return Vec::new();
    };
    let tab = model.tabs.open.remove(index);
    let position = position.min(model.tabs.open.len());
    model.tabs.open.insert(position, tab);
    Vec::new()
}

fn select_category(
    model: &mut AppModel,
    category_id: Option<carver_sdk::CategoryId>,
) -> Vec<Effect> {
    if let Some(effects) = editor_external_change_effects(model) {
        return effects;
    }
    if model.tabs.active.is_some() {
        stash_active_document(model);
        record_tab_history(model);
        model.tabs.active = None;
        model.editor = None;
        model.editor_preview = None;
        model.preview_timer = None;
        model.editor_link_dialog = None;
    }
    model.editor_return_route = super::Route::Browser;
    model.selected_category = category_id;
    model.route = super::Route::Browser;
    reload_browser(model).into_iter().collect()
}

fn complete_pending_navigation(model: &mut AppModel) -> Vec<Effect> {
    let Some(destination) = model.pending_navigation.take() else {
        return Vec::new();
    };
    match destination {
        PendingNavigation::Browser(category_id) => {
            model.selected_category = category_id;
            model.route = super::Route::Browser;
            reload_browser(model).into_iter().collect()
        }
        PendingNavigation::Base(base_id) => open_base(model, base_id),
    }
}

fn open_base(model: &mut AppModel, base_id: carver_sdk::BaseId) -> Vec<Effect> {
    if model.bases.selected != Some(base_id) {
        reset_base_search(model);
    }
    model.route = super::Route::Base;
    model.bases.selected = Some(base_id);
    let mut effects: Vec<_> = reload_base_rows(model, base_id).into_iter().collect();
    if matches!(model.bases.definitions.state, super::LoadState::Failed(_)) {
        effects.extend(reload_bases(model));
    }
    effects
}

fn open_base_search(model: &mut AppModel) -> Vec<Effect> {
    if model.bases.search_open {
        return Vec::new();
    }
    model.bases.search_open = true;
    Vec::new()
}

fn update_base_search_visibility(model: &mut AppModel, visible: bool) -> Vec<Effect> {
    if model.bases.search_open == visible {
        return Vec::new();
    }
    model.bases.search_open = visible;
    if visible {
        return Vec::new();
    }
    let search_changed = !model.bases.search_query.is_empty();
    reset_base_search(model);
    if !search_changed {
        return Vec::new();
    }
    model
        .bases
        .selected
        .and_then(|base_id| reload_base_rows(model, base_id))
        .into_iter()
        .collect()
}

fn reset_base_search(model: &mut AppModel) {
    model.bases.search_open = false;
    model.bases.search_query.clear();
    model.bases.search_timer = None;
}

fn save_base_sorts(model: &mut AppModel, sorts: Vec<carver_sdk::BaseSort>) -> Vec<Effect> {
    if model.route != super::Route::Base || model.bases.saving_configuration {
        return Vec::new();
    }
    let super::LoadState::Ready(definitions) = &model.bases.definitions.state else {
        return Vec::new();
    };
    let Some(definition) = definitions
        .iter()
        .find(|definition| Some(definition.id) == model.bases.selected)
        .cloned()
    else {
        return Vec::new();
    };
    if definition.sorts == sorts {
        return Vec::new();
    }
    update_bases(
        model,
        BasesMsg::Update {
            base_id: definition.id,
            revision: definition.revision,
            name: definition.name,
            columns: definition.columns,
            filter_mode: definition.filter_mode,
            filters: definition.filters,
            sorts,
            view: definition.view,
        },
    )
}

fn update_browser(model: &mut AppModel, message: BrowserMsg) -> Vec<Effect> {
    match message {
        BrowserMsg::Reload => reload_browser(model).into_iter().collect(),
        BrowserMsg::LoadMore => load_more_browser(model).into_iter().collect(),
        BrowserMsg::SearchTimerFired(timer_id) if model.browser.search_timer == Some(timer_id) => {
            model.browser.search_timer = None;
            reload_browser(model).into_iter().collect()
        }
        BrowserMsg::LoadingIndicatorElapsed(request_id)
            if model.browser.loading_indicator_request == Some(request_id)
                && matches!(model.browser.notes.state, super::LoadState::Loading(current) if current == request_id) =>
        {
            model.browser.loading_indicator_visible = true;
            Vec::new()
        }
        BrowserMsg::SearchChanged(query) => {
            if model.browser.search_query == query {
                return Vec::new();
            }
            model.browser.search_query = query;
            let timer_id = model.next_timer_id();
            model.browser.search_timer = Some(timer_id);
            vec![Effect::ScheduleSearch { timer_id }]
        }
        BrowserMsg::SearchShortcutRequested if model.route == super::Route::Browser => {
            open_browser_search(model)
        }
        BrowserMsg::SearchOpened => open_browser_search(model),
        BrowserMsg::SearchVisibilityChanged(visible) => {
            update_browser_search_visibility(model, visible)
        }
        BrowserMsg::SearchShortcutRequested
        | BrowserMsg::SearchTimerFired(_)
        | BrowserMsg::LoadingIndicatorElapsed(_) => Vec::new(),
    }
}

fn update_browser_search_visibility(model: &mut AppModel, visible: bool) -> Vec<Effect> {
    if model.browser.search_open == visible {
        return Vec::new();
    }
    model.browser.search_open = visible;
    if visible {
        return Vec::new();
    }
    model.browser.search_timer = None;
    model.browser.search_query.clear();
    reload_browser(model).into_iter().collect()
}

fn open_browser_search(model: &mut AppModel) -> Vec<Effect> {
    if model.browser.search_open {
        return Vec::new();
    }
    model.browser.search_open = true;
    Vec::new()
}

fn restore_editor_origin(model: &mut AppModel) {
    model.route =
        if model.editor_return_route == super::Route::Base && model.bases.selected.is_some() {
            super::Route::Base
        } else {
            super::Route::Browser
        };
    model.editor_return_route = super::Route::Browser;
}

fn update_preferences(model: &mut AppModel, preference: PreferencesMsg) -> Vec<Effect> {
    match preference {
        PreferencesMsg::SetEnhancedCarveRendering(enabled) => {
            model.preferences.html_profile =
                carver_domain::rendering::HtmlProfile::from_enabled(enabled);
            model.config.editor.enhanced_carve_rendering = enabled;
        }
        PreferencesMsg::SetRemoteImages(enabled) => {
            model.preferences.load_remote_images = enabled;
            model.config.images.load_remote_automatically = enabled;
        }
        PreferencesMsg::SetEditorMode(mode) => {
            model.preferences.editor_mode = mode;
            model.config.editor.last_mode = mode;
            if let Some(document) = model.editor.as_mut() {
                document.mode = mode;
            }
        }
        PreferencesMsg::SetSourceSplitView(visible) => {
            model.preferences.source_split_view = visible;
            model.config.editor.source_split_view = visible;
        }
        PreferencesMsg::SetFormattingToolbarVisible(visible) => {
            model.preferences.show_formatting_toolbar = visible;
            model.config.editor.show_formatting_toolbar = visible;
        }
        PreferencesMsg::SetSourceLineNumbers(visible) => {
            model.preferences.source_editor.show_line_numbers = visible;
            model.config.editor.source_line_numbers = visible;
        }
        PreferencesMsg::SetSourceHighlightCurrentLine(enabled) => {
            model.preferences.source_editor.highlight_current_line = enabled;
            model.config.editor.source_highlight_current_line = enabled;
        }
        PreferencesMsg::SetSourceSyntaxStyle(style) => {
            model.preferences.source_editor.syntax_style = style;
            model.config.editor.source_syntax_style = style;
        }
        PreferencesMsg::SetSourceFont(font) => {
            model.preferences.source_editor.font = font.clone();
            model.config.editor.source_font = font;
        }
        PreferencesMsg::SetDocumentFont(font) => {
            model.preferences.document.font = font.clone();
            model.config.editor.document_font = font;
        }
        PreferencesMsg::SetDocumentLineHeightPercent(percent) => {
            let percent = percent.clamp(100, 250);
            model.preferences.document.line_height_percent = percent;
            model.config.editor.document_line_height_percent = percent;
        }
        PreferencesMsg::SetDocumentWidth(width) => {
            model.preferences.document.width = width;
            model.config.editor.document_width = width;
        }
        PreferencesMsg::SetDocumentSidebarPage(page) => {
            model.config.editor.document_sidebar_page = page;
        }
        PreferencesMsg::SetNoteOpenBehavior(behavior) => {
            model.config.editor.note_open_behavior = behavior;
        }
        PreferencesMsg::SetDocumentPropertiesEnabled(enabled) => {
            model.config.document_properties.enabled = enabled;
        }
        PreferencesMsg::SetDocumentPropertiesFloatingButton(visible) => {
            model.config.document_properties.floating_button = visible;
        }
        PreferencesMsg::SetDocumentProperties(entries) => {
            model.config.document_properties.entries = entries;
        }
        PreferencesMsg::SetDocumentPropertiesFormat(format) => {
            model.config.document_properties.format = format;
        }
    }
    persist_config_effect(model)
}

// CONTEXT: The full editor message vocabulary is intentionally centralized so source changes,
// persistence, and asynchronous asset completion share one admission point.
#[expect(clippy::too_many_lines)]
fn update_editor(model: &mut AppModel, message: EditorMsg) -> Vec<Effect> {
    match message {
        EditorMsg::CloseDeleted { session } => {
            if model.editor.as_ref().is_some_and(|document| {
                document.session == session
                    && document.external_change == Some(ExternalChange::Deleted)
                    && !matches!(document.save_state, super::EditorSaveState::Saving(_))
            }) {
                let _ = discard_editor(model, session);
                model.pending_navigation = None;
                model.route = super::Route::Trash;
                reload_trash(model).into_iter().collect()
            } else {
                Vec::new()
            }
        }
        EditorMsg::KeepExternalDraft { session } => model
            .editor
            .as_ref()
            .filter(|document| document.session == session && document.external_change.is_some())
            .map(|document| Effect::ShowExternalEdit {
                session,
                deleted: document.external_change == Some(ExternalChange::Deleted),
            })
            .into_iter()
            .collect(),
        EditorMsg::ReloadExternal { session } => {
            if model.editor.as_ref().is_some_and(|document| {
                document.session == session
                    && !matches!(document.save_state, super::EditorSaveState::Saving(_))
            }) {
                refresh_open_editor(model, true).into_iter().collect()
            } else {
                Vec::new()
            }
        }
        EditorMsg::Load {
            note_id,
            revision,
            source,
        } => update_editor_load(model, note_id, revision, source),
        EditorMsg::SourceChanged(source) => {
            let changed = model
                .editor
                .as_mut()
                .is_some_and(|document| document.source_changed(source));
            if changed {
                model.clear_notice();
                return schedule_preview(model).into_iter().collect();
            }
            Vec::new()
        }
        EditorMsg::PropertiesDialogRequested => open_properties_effect(model),
        EditorMsg::ApplyFrontmatter {
            session,
            edit,
            category,
        } => apply_frontmatter_effect(model, session, edit, category),
        EditorMsg::ApplySourceCommand { command, selection } => {
            update_source_command(model, command, selection)
        }
        EditorMsg::ApplySourceInput { input, selection } => {
            let Some(document) = model.editor.as_ref() else {
                return Vec::new();
            };
            if document.mode != carver_config::EditorMode::Source {
                return Vec::new();
            }
            match SourceEdit::plan_input(&document.source, &document.analysis, selection, input) {
                Ok(super::SourceInputOutcome::Edit(edit)) => commit_source_edit(model, &edit),
                Ok(super::SourceInputOutcome::Noop | super::SourceInputOutcome::Native)
                | Err(_) => Vec::new(),
            }
        }
        EditorMsg::ApplyRichCommand(command) => update_rich_command(model, command),
        EditorMsg::LinkDialogRequested { origin } => open_link_dialog(model, origin),
        EditorMsg::LinkDialogQueryChanged(query) => update_link_dialog_query(model, query),
        EditorMsg::LinkDialogSearchElapsed { timer_id } => {
            update_link_dialog_search(model, timer_id)
        }
        EditorMsg::LinkDialogConfirmed {
            dialog_id,
            text,
            destination,
        } => confirm_link_dialog(model, dialog_id, &text, &destination),
        EditorMsg::LinkDialogDismissed(dialog_id) => {
            dismiss_link_dialog(model, dialog_id);
            Vec::new()
        }
        EditorMsg::PreviewElapsed { session, timer_id }
            if model.preview_timer == Some((session, timer_id)) =>
        {
            model.preview_timer = None;
            if let Some(document) = model
                .editor
                .as_ref()
                .filter(|document| document.session == session)
            {
                model.editor_preview = Some(super::EditorPreview {
                    session,
                    source: document.source.clone(),
                });
            }
            Vec::new()
        }
        EditorMsg::AutosaveRequested => schedule_editor_save(model).into_iter().collect(),
        EditorMsg::AutosaveElapsed { session, timer_id } => model
            .document_for_session_mut(session)
            .filter(|document| document.is_current_timer(timer_id))
            .and_then(super::EditorDocument::begin_save)
            .map_or_else(Vec::new, save_note_effect),
        EditorMsg::RetrySave => model
            .editor
            .as_mut()
            .and_then(super::EditorDocument::begin_save)
            .map_or_else(Vec::new, save_note_effect),
        EditorMsg::BackRequested => {
            model.pending_navigation = None;
            navigate_back(model)
        }
        EditorMsg::TrashRequested => model
            .editor
            .as_ref()
            .map(|document| document.note_id)
            .map_or_else(Vec::new, |note_id| {
                update_action(model, ActionMsg::TrashNote(note_id))
            }),
        EditorMsg::ToggleFavorite => toggle_editor_favorite(model),
        EditorMsg::CopyRequested => request_editor_copy(model),
        EditorMsg::CopySelectionRequested { session, source } => {
            request_editor_selection_copy(model, session, source)
        }
        message @ (EditorMsg::ExportDialogRequested
        | EditorMsg::ExportRequested { .. }
        | EditorMsg::ExportConfirmed { .. }
        | EditorMsg::ExportCancelled { .. }
        | EditorMsg::PdfExportCompleted { .. }
        | EditorMsg::PdfExportFailed { .. }
        | EditorMsg::PdfExportCancelled { .. }
        | EditorMsg::PrintRequested) => update_editor_export(model, message),
        EditorMsg::CopyCompleted {
            request_id,
            omitted_images,
        } => complete_copy_request(model, request_id, omitted_images),
        EditorMsg::CopyFailed { request_id } => fail_copy_request(model, request_id),
        EditorMsg::PasteImage { extension, bytes } => store_editor_asset_effect(
            model,
            extension,
            bytes,
            String::from("Pasted image"),
            None,
            true,
        ),
        EditorMsg::PasteRichText {
            request_id,
            text,
            intent,
            host_initiated,
        } => {
            // Reject a reply or command for a projection that is no longer active:
            // a late paste must never overwrite newer source edits.
            let Some(document) = model
                .editor
                .as_ref()
                .filter(|document| document.mode == carver_config::EditorMode::Rich)
            else {
                return Vec::new();
            };
            let session = document.session;
            let pasted = carver_domain::import_pasted_text(&text, intent);
            vec![Effect::InsertRichSource {
                session,
                request_id,
                structured: pasted.format != carver_domain::PastedFormat::Plain,
                source: pasted.source,
                fallback: text,
                host_initiated,
            }]
        }
        EditorMsg::PasteSourceText {
            session,
            target,
            text,
            intent,
        } => {
            // The captured snapshot must still match the open document so a slow
            // clipboard read cannot paste into a later edit or another note.
            if !model.editor.as_ref().is_some_and(|document| {
                document.session == session && document.source == target.source
            }) {
                return Vec::new();
            }
            let pasted = carver_domain::import_pasted_text(&text, intent);
            update_source_command(
                model,
                super::SourceCommand::InsertText(pasted.source),
                target.selection,
            )
        }
        EditorMsg::ImportFiles { target, files } => model
            .editor
            .as_ref()
            .filter(|document| {
                document.session == target.session
                    && document.mode != carver_config::EditorMode::Rendered
            })
            .map(|document| Effect::ImportEditorFiles {
                note_id: document.note_id,
                target,
                files,
            })
            .into_iter()
            .collect(),
        EditorMsg::ImportFilesStored { target, result } => {
            complete_file_import(model, target, result)
        }
        EditorMsg::ImportImageRead { target, bytes } => {
            if !model.editor.as_ref().is_some_and(|document| {
                document.session == target.session
                    && document.mode != carver_config::EditorMode::Rendered
            }) {
                return Vec::new();
            }
            store_editor_asset_effect(
                model,
                "png".into(),
                bytes,
                "Pasted image".into(),
                target.source,
                true,
            )
        }
        EditorMsg::ImportImage {
            extension,
            bytes,
            alt,
            source_target,
        } => store_editor_asset_effect(model, extension, bytes, alt, source_target, true),
        EditorMsg::ImportFile {
            extension,
            bytes,
            name,
            source_target,
        } => store_editor_asset_effect(model, extension, bytes, name, source_target, false),
        EditorMsg::DocumentSelectionChanged {
            formatting,
            session,
            mode,
            media,
            heading,
            source,
        } => {
            if let Some(document) = model.editor.as_mut()
                && document.session == session
                && document.mode == mode
                && document.source == source.as_ref()
            {
                if let Some(formatting) = formatting
                    && mode == carver_config::EditorMode::Rich
                {
                    model.rich_selection = formatting;
                }
                document.selected_heading =
                    heading.filter(|index| *index < document.analysis.headings().len());
                document.selected_media = media.and_then(|selected| {
                    document
                        .analysis
                        .media()
                        .iter()
                        .filter(|item| item.path == selected.path)
                        .nth(selected.occurrence)
                        .map(|item| item.range.clone())
                });
            }
            Vec::new()
        }
        EditorMsg::SourceSelectionChanged { selection } => {
            if let Some(document) = model.editor.as_mut()
                && document.mode == carver_config::EditorMode::Source
            {
                document.selected_heading = document.analysis.headings().iter().position(|item| {
                    item.range.start <= selection.start
                        && selection.start <= item.range.end
                        && selection.end <= item.range.end
                });
                document.selected_media = document
                    .analysis
                    .media()
                    .iter()
                    .find(|item| {
                        item.range.start <= selection.start
                            && selection.start < item.range.end
                            && selection.end <= item.range.end
                    })
                    .map(|item| item.range.clone());
            }
            Vec::new()
        }
        EditorMsg::PreviewMedia { selection } => model
            .editor
            .as_ref()
            .and_then(|document| {
                document
                    .analysis
                    .media()
                    .iter()
                    .find(|media| media.range == selection && media.path.starts_with("assets/"))
                    .map(|media| Effect::PrepareMediaPreview {
                        session: document.session,
                        note_id: document.note_id,
                        path: media.path.clone(),
                        label: media.label.clone(),
                    })
            })
            .into_iter()
            .collect(),
        EditorMsg::MediaPreviewPrepared { session, result } => {
            if model
                .editor
                .as_ref()
                .is_none_or(|document| document.session != session)
            {
                return Vec::new();
            }
            match result {
                Ok(path) => vec![Effect::ShowMediaPreview { session, path }],
                Err(error) => {
                    model.set_notice(error);
                    Vec::new()
                }
            }
        }
        EditorMsg::MediaPreviewFailed { session } => {
            if model
                .editor
                .as_ref()
                .is_some_and(|document| document.session == session)
            {
                model.set_notice(UiError::new(
                    "Could not open this file. Install an application that can view it.",
                ));
            }
            Vec::new()
        }
        EditorMsg::DownloadMedia { selection } => model
            .editor
            .as_ref()
            .and_then(|document| {
                document
                    .analysis
                    .media()
                    .iter()
                    .find(|media| media.range == selection && media.path.starts_with("assets/"))
                    .map(|media| Effect::ShowMediaDownloadDialog {
                        session: document.session,
                        path: media.path.clone(),
                        label: media.label.clone(),
                    })
            })
            .into_iter()
            .collect(),
        EditorMsg::MediaDownloadRequested {
            session,
            path,
            target_uri,
        } => match model.document_for_session(session) {
            Some(document) if path.starts_with("assets/") => vec![Effect::WriteMediaDownload {
                session,
                note_id: document.note_id,
                path,
                target_uri,
            }],
            _ => Vec::new(),
        },
        EditorMsg::MediaDownloadFinished { session, result } => {
            if model
                .editor
                .as_ref()
                .is_some_and(|document| document.session == session)
            {
                match result {
                    Ok(()) => model.set_notice(UiError::new(gettext("File saved"))),
                    Err(error) => model.set_notice(error),
                }
            }
            Vec::new()
        }
        EditorMsg::MediaFileLoaded {
            image,
            session,
            path,
            file,
        } => {
            if let Some(document) = model.document_for_session_mut(session)
                && document.media_file_kinds.get(&path) == Some(&image)
            {
                document.media_files.insert(path, file);
            }
            Vec::new()
        }
        EditorMsg::ToggleDocumentSidebar => {
            let visible = if let Some(document) = model.editor.as_mut() {
                document.document_sidebar = document.document_sidebar.toggled();
                model.config.editor.show_document_sidebar = document.document_sidebar.is_visible();
                document.document_sidebar.is_visible()
            } else {
                return Vec::new();
            };
            let mut effects = persist_config_effect(model);
            if visible {
                effects.extend(reload_note_links(model));
            }
            effects
        }
        EditorMsg::FocusDocumentTarget {
            session,
            generation,
            target,
        } => {
            let Some(document) = model.editor.as_mut().filter(|document| {
                document.session == session && document.source_generation == generation
            }) else {
                return Vec::new();
            };
            let selection = match &target {
                carver_editor_protocol::DocumentTarget::Heading { occurrence } => {
                    let Some(heading) = document.analysis.headings().get(*occurrence) else {
                        return Vec::new();
                    };
                    document.selected_heading = Some(*occurrence);
                    document.selected_media = None;
                    heading.text_start..heading.text_start
                }
                carver_editor_protocol::DocumentTarget::Media { path, occurrence } => {
                    let Some(media) = document
                        .analysis
                        .media()
                        .iter()
                        .filter(|item| &item.path == path)
                        .nth(*occurrence)
                    else {
                        return Vec::new();
                    };
                    document.selected_media = Some(media.range.clone());
                    document.selected_heading = None;
                    media.range.clone()
                }
            };
            vec![Effect::FocusDocumentTarget {
                session,
                generation,
                selection,
                target,
            }]
        }
        #[cfg(test)]
        EditorMsg::Close(session_id) => discard_editor(model, session_id),
        EditorMsg::PreviewElapsed { .. } => Vec::new(),
        EditorMsg::ThemeChanged => {
            model.editor_theme_revision = model.editor_theme_revision.wrapping_add(1);
            Vec::new()
        }
    }
}

fn update_editor_load(
    model: &mut AppModel,
    note_id: carver_sdk::NoteId,
    revision: carver_sdk::Revision,
    source: String,
) -> Vec<Effect> {
    if model
        .editor
        .as_ref()
        .is_some_and(|document| document.note_id == note_id)
    {
        model.route = super::Route::Editor;
        return Vec::new();
    }
    let origin = current_tab_origin(model);
    let tab_id = if let Some(tab_id) = model.note_tab_for_note(note_id) {
        tab_id
    } else {
        push_note_tab(model, note_id, origin, String::new(), false, false)
    };
    // The test-only load path has no library note; a fresh id simply leaves the category row
    // unselected until a real load replaces it.
    let title = carver_domain::derive_content(&source).title;
    if model.tabs.active == Some(tab_id) {
        // Reloading the active tab in place is not a tab switch, so it is not Back history.
        model.route = super::Route::Editor;
        model.editor_preview = None;
        model.preview_timer = None;
        model.editor_link_dialog = None;
    } else {
        begin_tab_switch(model, tab_id);
    }
    let _ = open_editor(
        model,
        note_id,
        carver_sdk::CategoryId::new(),
        revision,
        false,
        source,
        super::model::DocumentOrigin::Synthetic,
    );
    if let Some(tab) = model.note_tab_mut(tab_id) {
        tab.loading = false;
        tab.title = title;
    }
    Vec::new()
}

/// Opens the unified link dialog for the active editor and presents it.
fn open_link_dialog(model: &mut AppModel, origin: super::LinkDialogOrigin) -> Vec<Effect> {
    let Some(session) = model.editor.as_ref().map(|document| document.session) else {
        return Vec::new();
    };
    let dialog_id = model.next_request_id();
    model.editor_link_dialog = Some(super::EditorLinkDialog {
        dialog_id,
        session,
        origin: origin.clone(),
        query: String::new(),
        search_timer: None,
        candidates: super::Resource::default(),
    });
    vec![Effect::ShowLinkDialog {
        dialog_id,
        session,
        origin,
    }]
}

/// Records a new link-dialog query and debounces its note search.
fn update_link_dialog_query(model: &mut AppModel, query: String) -> Vec<Effect> {
    if model.editor_link_dialog.is_none() {
        return Vec::new();
    }
    let timer_id = model.next_timer_id();
    if let Some(dialog) = model.editor_link_dialog.as_mut() {
        dialog.query = query;
        dialog.search_timer = Some(timer_id);
    }
    vec![Effect::ScheduleLinkSearch { timer_id }]
}

/// Starts or coalesces a note search for the dialog's current query.
fn search_link_candidates(model: &mut AppModel) -> Vec<Effect> {
    let request_id = model.next_request_id();
    let Some(dialog) = model.editor_link_dialog.as_mut() else {
        return Vec::new();
    };
    let query = dialog.query.clone();
    if query.trim().is_empty() {
        dialog.candidates = super::Resource::default();
        return Vec::new();
    }
    if dialog.candidates.begin_reload(request_id) {
        vec![Effect::SearchLinkCandidates { request_id, query }]
    } else {
        Vec::new()
    }
}

/// Runs the debounced note search when its timer is still current.
fn update_link_dialog_search(model: &mut AppModel, timer_id: super::TimerId) -> Vec<Effect> {
    let current = model
        .editor_link_dialog
        .as_ref()
        .is_some_and(|dialog| dialog.search_timer == Some(timer_id));
    if !current {
        return Vec::new();
    }
    if let Some(dialog) = model.editor_link_dialog.as_mut() {
        dialog.search_timer = None;
    }
    search_link_candidates(model)
}

/// Applies a confirmed link dialog edit through the originating editor surface.
fn confirm_link_dialog(
    model: &mut AppModel,
    dialog_id: super::RequestId,
    text: &str,
    destination: &str,
) -> Vec<Effect> {
    let Some(dialog) = model
        .editor_link_dialog
        .as_ref()
        .filter(|dialog| dialog.dialog_id == dialog_id)
    else {
        return Vec::new();
    };
    let origin = dialog.origin.clone();
    model.editor_link_dialog = None;
    let text = text.trim();
    let destination = destination.trim();
    if text.is_empty() || destination.is_empty() {
        return Vec::new();
    }
    match origin {
        super::LinkDialogOrigin::Rich { .. } => {
            vec![Effect::ApplyRichEditorCommand {
                command: carver_editor_protocol::EditorCommand::InsertLink(
                    carver_editor_protocol::LinkCommand {
                        text: text.to_owned(),
                        destination: destination.to_owned(),
                    },
                ),
            }]
        }
        super::LinkDialogOrigin::Source { selection, .. } => update_source_command(
            model,
            super::SourceCommand::InsertLink {
                text: text.to_owned(),
                destination: destination.to_owned(),
            },
            selection,
        ),
    }
}

/// Clears the active link dialog if it is the one being dismissed.
fn dismiss_link_dialog(model: &mut AppModel, dialog_id: super::RequestId) {
    if model
        .editor_link_dialog
        .as_ref()
        .is_some_and(|dialog| dialog.dialog_id == dialog_id)
    {
        model.editor_link_dialog = None;
    }
}

fn update_source_command(
    model: &mut AppModel,
    command: super::SourceCommand,
    selection: std::ops::Range<usize>,
) -> Vec<Effect> {
    let Some(document) = model.editor.as_mut() else {
        return Vec::new();
    };
    let edit = SourceEdit::apply(document.source.clone(), selection, command);
    commit_source_edit(model, &edit)
}

fn commit_source_edit(model: &mut AppModel, edit: &SourceEdit) -> Vec<Effect> {
    let Some(document) = model.editor.as_mut() else {
        return Vec::new();
    };
    let session = document.session;
    if !document.source_changed(edit.source().to_owned()) {
        return Vec::new();
    }
    document.source_selection = Some(edit.selection());
    model.clear_notice();
    let mut effects = [schedule_preview(model), schedule_editor_save(model)]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    effects.push(Effect::SelectEditorSource {
        session,
        selection: edit.selection(),
    });
    effects
}

fn update_rich_command(
    model: &AppModel,
    command: carver_editor_protocol::EditorCommand,
) -> Vec<Effect> {
    model
        .editor
        .is_some()
        .then_some(Effect::ApplyRichEditorCommand { command })
        .into_iter()
        .collect()
}

fn update_editor_export(model: &mut AppModel, message: EditorMsg) -> Vec<Effect> {
    match message {
        EditorMsg::ExportDialogRequested => request_editor_export_dialog(model),
        EditorMsg::ExportRequested {
            request_id,
            format,
            include_assets,
            target_uri,
        } => request_editor_export(model, request_id, format, include_assets, target_uri),
        EditorMsg::ExportConfirmed { request_id } => confirm_editor_export(model, request_id),
        EditorMsg::ExportCancelled { request_id } => cancel_editor_export(model, request_id),
        EditorMsg::PdfExportCompleted { request_id } => complete_pdf_export(model, request_id),
        EditorMsg::PdfExportFailed { request_id } => fail_pdf_export(model, request_id),
        EditorMsg::PdfExportCancelled { request_id } => cancel_pdf_export(model, request_id),
        EditorMsg::PrintRequested => request_editor_print(model),
        _ => Vec::new(),
    }
}

/// Removes a document's tab without saving it.
///
/// Used when the note no longer exists, or after its final mutation already persisted; unlike
/// [`close_tab`] this never starts a save and does not activate a neighbor.
fn discard_editor(model: &mut AppModel, session_id: super::EditorSessionId) -> Vec<Effect> {
    let tab_id = if model
        .editor
        .as_ref()
        .is_some_and(|document| document.session == session_id)
    {
        model.tabs.active
    } else {
        model
            .tabs
            .background
            .iter()
            .find(|(_, document)| document.session == session_id)
            .map(|(tab_id, _)| *tab_id)
    };
    let Some(tab_id) = tab_id else {
        return Vec::new();
    };
    let was_active = model.tabs.active == Some(tab_id);
    model.tabs.open.retain(|tab| tab.id != tab_id);
    model.tabs.background.remove(&tab_id);
    model.tabs.history.retain(|entry| *entry != Some(tab_id));
    if !was_active {
        return Vec::new();
    }
    model.tabs.active = None;
    restore_editor_origin(model);
    model.editor = None;
    model.editor_preview = None;
    model.editor_copy_request = None;
    model.editor_export_dialog_request = None;
    model.editor_export_warning_request = None;
    model.editor_export_progress = None;
    model.editor_pdf_export_request = None;
    model.editor_link_dialog = None;
    model.preview_timer = None;
    model.editor_load_request = None;
    model.editor_export_after_load = None;
    Vec::new()
}

pub(super) fn schedule_preview(model: &mut AppModel) -> Option<Effect> {
    let session = model.editor.as_ref()?.session;
    let timer_id = model.next_preview_timer_id();
    model.preview_timer = Some((session, timer_id));
    Some(Effect::SchedulePreview { session, timer_id })
}

fn persist_config_effect(model: &AppModel) -> Vec<Effect> {
    vec![Effect::PersistConfig {
        config: model.config.clone(),
    }]
}

fn create_note_effect(model: &mut AppModel) -> Vec<Effect> {
    let source = model.config.document_properties.default_source();
    create_note_with_source_effect(model, source)
}

fn create_note_with_source_effect(model: &mut AppModel, source: String) -> Vec<Effect> {
    let category_id = model.active_category_id();
    create_note_in_category_effect(model, category_id, source)
}

fn create_note_in_category_effect(
    model: &mut AppModel,
    category_id: Option<carver_sdk::CategoryId>,
    source: String,
) -> Vec<Effect> {
    category_id.map_or_else(
        || {
            model.set_notice(UiError::new("No category is available for the new note."));
            Vec::new()
        },
        |category_id| {
            vec![Effect::CreateNote {
                category_id,
                source,
            }]
        },
    )
}

/// Creates a note from clipboard text, converting it with the requested intent.
///
/// The destination category is resolved when the command is invoked, not when the
/// asynchronous clipboard read completes, so switching category mid-read cannot retarget it.
fn create_note_from_clipboard_effect(
    model: &mut AppModel,
    text: &str,
    intent: carver_domain::PasteIntent,
    category_id: Option<carver_sdk::CategoryId>,
) -> Vec<Effect> {
    if text.trim().is_empty() {
        model.set_notice(UiError::new(gettext("There is no text on the clipboard.")));
        return Vec::new();
    }
    let source = carver_domain::import_pasted_text(text, intent).source;
    let source = seed_default_properties(model, source);
    create_note_in_category_effect(model, category_id, source)
}

/// Prepends the configured default properties when a document has no frontmatter of its own.
fn seed_default_properties(model: &AppModel, source: String) -> String {
    if carver_domain::frontmatter_raw(&source).is_some() {
        return source;
    }
    let defaults = model.config.document_properties.default_source();
    if defaults.is_empty() {
        source
    } else {
        format!("{defaults}{source}")
    }
}

fn open_properties_effect(model: &AppModel) -> Vec<Effect> {
    let Some(document) = model.editor.as_ref() else {
        return Vec::new();
    };
    vec![Effect::ShowDocumentProperties {
        request: super::EditorPropertiesRequest {
            save: super::PropertiesSave::Editor {
                session: document.session,
            },
            note_id: document.note_id,
            document: carver_domain::parse_frontmatter_document(&document.source),
            raw: carver_domain::frontmatter_raw(&document.source).map(|(_, content)| content),
            heading_title: carver_domain::derive_content(&document.source).heading_title,
            defaults: model.config.document_properties.entries.clone(),
            default_format: model.config.document_properties.format,
            category_id: Some(document.category_id),
            categories: active_category_choices(model),
        },
    }]
}

/// Returns the active categories offered by the document-properties dialog.
fn active_category_choices(model: &AppModel) -> Vec<super::CategoryChoice> {
    match &model.sidebar.state {
        super::LoadState::Ready(summaries) => summaries
            .iter()
            .map(|summary| super::CategoryChoice {
                id: summary.category.id,
                name: summary.category.name.clone(),
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn apply_frontmatter_effect(
    model: &mut AppModel,
    session: super::EditorSessionId,
    edit: super::FrontmatterEdit,
    category: Option<carver_sdk::CategoryId>,
) -> Vec<Effect> {
    let Some(document) = model.editor.as_mut() else {
        return Vec::new();
    };
    // A local autosave may advance the revision while the dialog is open; only an unresolved
    // external change invalidates the dialog.
    if document.session != session || document.external_change.is_some() {
        return Vec::new();
    }
    let updated = match edit {
        super::FrontmatterEdit::Parsed(parsed) => {
            carver_domain::replace_frontmatter(&document.source, Some(&parsed))
        }
        super::FrontmatterEdit::Raw { format, content } => Ok(
            carver_domain::replace_frontmatter_raw(&document.source, format, &content),
        ),
    };
    let Ok(updated) = updated else {
        return Vec::new();
    };
    let changed = model
        .editor
        .as_mut()
        .is_some_and(|document| document.source_changed(updated));
    // A category change is applied by the next save, so mark the document dirty when it is the
    // only change (content autosave is otherwise a no-op).
    let category_changed = category.is_some_and(|category| {
        model.editor.as_mut().is_some_and(|document| {
            if document.category_id == category {
                return false;
            }
            document.pending_category = Some(category);
            if matches!(document.save_state, super::EditorSaveState::Clean) {
                document.save_state = super::EditorSaveState::Dirty;
            }
            true
        })
    });
    if !changed && !category_changed {
        return Vec::new();
    }
    let source = model
        .editor
        .as_ref()
        .map(|document| document.source.clone());
    model.clear_notice();
    let mut effects: Vec<Effect> = schedule_preview(model).into_iter().collect();
    effects.extend(schedule_editor_save(model));
    if changed && let Some(source) = source {
        // The rich projection caches the old frontmatter atom; reload it so the next rich edit
        // serializes the new block instead of reverting the change.
        effects.push(Effect::ReloadRichEditor { session, source });
    }
    effects
}

fn import_note_effect(
    model: &mut AppModel,
    format: carver_sdk::DocumentImportFormat,
    source: String,
) -> Vec<Effect> {
    let category_id = model.active_category_id();
    category_id.map_or_else(
        || {
            model.set_notice(UiError::new(
                "No category is available for the imported note.",
            ));
            Vec::new()
        },
        |category_id| {
            vec![Effect::ImportNote {
                category_id,
                format,
                source,
            }]
        },
    )
}

/// Builds an editor document for a note, without making it active.
fn new_editor_document(
    model: &mut AppModel,
    note_id: carver_sdk::NoteId,
    category_id: carver_sdk::CategoryId,
    revision: carver_sdk::Revision,
    is_favorite: bool,
    source: String,
    origin: super::model::DocumentOrigin,
) -> super::EditorDocument {
    let session = model.next_editor_session_id();
    let mut document = super::EditorDocument::new(
        session,
        note_id,
        category_id,
        revision,
        is_favorite,
        source,
        model.preferences.editor_mode,
    );
    document.origin = origin;
    if model.config.editor.show_document_sidebar {
        document.document_sidebar = super::model::DocumentSidebarVisibility::Visible;
    }
    document
}

fn open_editor(
    model: &mut AppModel,
    note_id: carver_sdk::NoteId,
    category_id: carver_sdk::CategoryId,
    revision: carver_sdk::Revision,
    is_favorite: bool,
    source: String,
    origin: super::model::DocumentOrigin,
) -> super::EditorSessionId {
    let document = new_editor_document(
        model,
        note_id,
        category_id,
        revision,
        is_favorite,
        source.clone(),
        origin,
    );
    let session = document.session;
    model.editor_refresh_request = None;
    model.editor_refresh_pending = false;
    model.editor_refresh_retry = None;
    model.editor = Some(document);
    model.editor_preview = Some(super::EditorPreview { session, source });
    model.editor_copy_request = None;
    model.editor_export_dialog_request = None;
    model.editor_export_warning_request = None;
    model.editor_export_progress = None;
    model.editor_pdf_export_request = None;
    model.editor_link_dialog = None;
    model.preview_timer = None;
    session
}

fn complete_copy_request(
    model: &mut AppModel,
    request_id: u64,
    omitted_images: usize,
) -> Vec<Effect> {
    let Some(request) = model.editor_copy_request.as_ref() else {
        return Vec::new();
    };
    if request.request_id != request_id {
        return Vec::new();
    }
    let scope = request.scope;
    model.editor_copy_request = None;
    match (scope, omitted_images) {
        (super::EditorCopyScope::Note, 0) => {
            model.set_notice(UiError::new("Note copied"));
        }
        (super::EditorCopyScope::Note, omitted) => {
            model.set_notice(UiError::new(format!(
                "Note copied; {omitted} images were omitted."
            )));
        }
        (super::EditorCopyScope::Selection, 0) => {}
        (super::EditorCopyScope::Selection, omitted) => {
            model.set_notice(UiError::new(format!(
                "Selection copied; {omitted} images were omitted."
            )));
        }
    }
    Vec::new()
}

fn request_editor_copy(model: &mut AppModel) -> Vec<Effect> {
    let Some((session, note_id, source)) = model
        .editor
        .as_ref()
        .map(|document| (document.session, document.note_id, document.source.clone()))
    else {
        return Vec::new();
    };
    let request = super::EditorCopyRequest {
        request_id: model.next_editor_copy_request_id(),
        session,
        note_id,
        source,
        scope: super::EditorCopyScope::Note,
    };
    model.editor_copy_request = Some(request.clone());
    vec![Effect::CopyEditorDocument { request }]
}

fn request_editor_selection_copy(
    model: &mut AppModel,
    session: EditorSessionId,
    source: String,
) -> Vec<Effect> {
    let Some(note_id) = model
        .editor
        .as_ref()
        .filter(|document| document.session == session)
        .map(|document| document.note_id)
    else {
        return Vec::new();
    };
    let request = super::EditorCopyRequest {
        request_id: model.next_editor_copy_request_id(),
        session,
        note_id,
        source,
        scope: super::EditorCopyScope::Selection,
    };
    model.editor_copy_request = Some(request.clone());
    vec![Effect::CopyEditorDocument { request }]
}

fn fail_copy_request(model: &mut AppModel, request_id: u64) -> Vec<Effect> {
    let Some(request) = model.editor_copy_request.as_ref() else {
        return Vec::new();
    };
    if request.request_id != request_id {
        return Vec::new();
    }
    let scope = request.scope;
    model.editor_copy_request = None;
    let message = match scope {
        super::EditorCopyScope::Note => "Could not copy the note.",
        super::EditorCopyScope::Selection => "Could not copy the selection.",
    };
    model.set_notice(UiError::new(message));
    Vec::new()
}

fn request_editor_export_dialog(model: &mut AppModel) -> Vec<Effect> {
    let Some((session, note_id, source)) = model
        .editor
        .as_ref()
        .map(|document| (document.session, document.note_id, document.source.clone()))
    else {
        return Vec::new();
    };
    let request = super::EditorExportDialogRequest {
        html_profile: model.preferences.html_profile,
        request_id: model.next_editor_export_request_id(),
        session,
        note_id,
        source: source.clone(),
        filename_stem: carver_export::sanitized_filename_stem(
            &carver_domain::derive_content(&source).title,
        ),
    };
    model.editor_export_dialog_request = Some(request.clone());
    vec![Effect::ShowEditorExportDialog { request }]
}

fn request_editor_export(
    model: &mut AppModel,
    request_id: u64,
    format: super::EditorExportFormat,
    include_assets: bool,
    target_uri: String,
) -> Vec<Effect> {
    let Some(request) = model
        .editor_export_dialog_request
        .take_if(|request| request.request_id == request_id)
    else {
        return Vec::new();
    };
    if matches!(format, super::EditorExportFormat::Pdf) {
        let pdf_request = super::EditorPdfExportRequest {
            html_profile: request.html_profile,
            request_id,
            session: request.session,
            note_id: request.note_id,
            source: request.source,
            target_uri,
            print_dialog: false,
        };
        model.editor_pdf_export_request = Some(pdf_request.clone());
        return vec![Effect::ExportEditorPdf {
            request: pdf_request,
        }];
    }
    model.editor_export_progress = Some(super::EditorExportProgress {
        request_id,
        session: request.session,
    });
    vec![Effect::PrepareEditorExport {
        html_profile: request.html_profile,
        request_id,
        session: request.session,
        note_id: request.note_id,
        source: request.source,
        filename_stem: request.filename_stem,
        format,
        include_assets,
        target_uri,
    }]
}

fn confirm_editor_export(model: &mut AppModel, request_id: u64) -> Vec<Effect> {
    let Some(request) = model.editor_export_warning_request.take() else {
        return Vec::new();
    };
    if request.request_id != request_id {
        model.editor_export_warning_request = Some(request);
        return Vec::new();
    }
    vec![Effect::WriteEditorExport { request_id }]
}

fn cancel_editor_export(model: &mut AppModel, request_id: u64) -> Vec<Effect> {
    let Some(request) = model.editor_export_warning_request.take() else {
        return Vec::new();
    };
    if request.request_id != request_id {
        model.editor_export_warning_request = Some(request);
        return Vec::new();
    }
    model.editor_export_progress = None;
    vec![Effect::DiscardEditorExport { request_id }]
}

fn complete_pdf_export(model: &mut AppModel, request_id: u64) -> Vec<Effect> {
    if model
        .editor_pdf_export_request
        .as_ref()
        .is_none_or(|request| request.request_id != request_id)
    {
        return Vec::new();
    }
    model.editor_pdf_export_request = None;
    model.set_notice(UiError::new("Note exported as PDF"));
    Vec::new()
}

fn fail_pdf_export(model: &mut AppModel, request_id: u64) -> Vec<Effect> {
    if model
        .editor_pdf_export_request
        .as_ref()
        .is_none_or(|request| request.request_id != request_id)
    {
        return Vec::new();
    }
    model.editor_pdf_export_request = None;
    model.set_notice(UiError::new("Could not export the note as PDF."));
    Vec::new()
}

fn cancel_pdf_export(model: &mut AppModel, request_id: u64) -> Vec<Effect> {
    if model
        .editor_pdf_export_request
        .as_ref()
        .is_none_or(|request| request.request_id != request_id)
    {
        return Vec::new();
    }
    model.editor_pdf_export_request = None;
    Vec::new()
}

fn request_editor_print(model: &mut AppModel) -> Vec<Effect> {
    let Some((session, note_id, source)) = model
        .editor
        .as_ref()
        .map(|document| (document.session, document.note_id, document.source.clone()))
    else {
        return Vec::new();
    };
    let pdf_request = super::EditorPdfExportRequest {
        html_profile: model.preferences.html_profile,
        request_id: model.next_editor_export_request_id(),
        session,
        note_id,
        source,
        target_uri: String::new(),
        print_dialog: true,
    };
    model.editor_pdf_export_request = Some(pdf_request.clone());
    vec![Effect::ExportEditorPdf {
        request: pdf_request,
    }]
}

fn update_action(model: &mut AppModel, action: ActionMsg) -> Vec<Effect> {
    if let ActionMsg::TrashNote(note_id) = action
        && let Some(document) = model.editor.as_ref().filter(|document| {
            document.note_id == note_id && document.external_change == Some(ExternalChange::Deleted)
        })
    {
        return vec![Effect::ShowExternalEdit {
            session: document.session,
            deleted: true,
        }];
    }
    if matches!(action, ActionMsg::UndoMove) {
        return update_undo_move(model);
    }
    let Some(key) = action.key() else {
        return Vec::new();
    };
    if !model.begin_action(key) {
        return Vec::new();
    }
    if let ActionMsg::TrashNote(note_id) = action
        && let Some(session) = model
            .document_for_note(note_id)
            .map(|document| document.session)
    {
        // The note is going to the trash, so drop its tab without saving the draft. This covers a
        // tab in the background too, which would otherwise keep a stale, document-less page.
        let _ = discard_editor(model, session);
    }
    let effect = match action {
        ActionMsg::CreateCategory(name) => {
            category_name_effect(&name, |name| Effect::CreateCategory { name })
        }
        ActionMsg::CreateCategoryWithAppearance {
            name,
            appearance,
            template_id,
        } => category_name_effect(&name, |name| Effect::CreateCategoryWithAppearance {
            name,
            appearance,
            template_id,
        }),
        ActionMsg::CreateCategoryAndMoveNote { name, note_id, .. } => {
            category_name_effect(&name, |name| Effect::CreateCategoryAndMoveNote {
                action: key,
                name,
                note_id,
            })
        }
        ActionMsg::RenameCategory { category_id, name } => {
            category_name_effect(&name, |name| Effect::RenameCategory { category_id, name })
        }
        ActionMsg::UpdateCategory {
            category_id,
            name,
            appearance,
        } => category_name_effect(&name, |name| Effect::UpdateCategory {
            category_id,
            name,
            appearance,
        }),
        ActionMsg::TrashCategory(category_id) => Some(Effect::TrashCategory { category_id }),
        ActionMsg::MoveNote {
            note_id,
            source_category_id: _,
            category_id,
        } => Some(Effect::MoveNote {
            action: key,
            note_id,
            category_id,
        }),
        ActionMsg::TrashNote(note_id) => Some(Effect::TrashNote { note_id }),
        ActionMsg::SetNoteFavorite {
            note_id,
            revision,
            is_favorite,
        } => Some(Effect::SetNoteFavorite {
            action: key,
            note_id,
            revision,
            is_favorite,
        }),
        ActionMsg::UndoMove => None,
    };
    if let Some(effect) = effect {
        vec![effect]
    } else {
        model.finish_action(key);
        model.set_notice(UiError::new("Category names cannot be empty."));
        Vec::new()
    }
}

fn update_undo_move(model: &mut AppModel) -> Vec<Effect> {
    let Some(MoveUndo {
        note_id,
        source_category_id,
    }) = model.undo_move
    else {
        return Vec::new();
    };
    let action = ActionKey::UndoMove(note_id);
    if !model.begin_action(action) {
        return Vec::new();
    }
    vec![Effect::MoveNote {
        action,
        note_id,
        category_id: source_category_id,
    }]
}

fn category_name_effect(name: &str, effect: impl FnOnce(String) -> Effect) -> Option<Effect> {
    let name = name.trim().to_owned();
    (!name.is_empty()).then(|| effect(name))
}

#[expect(
    clippy::too_many_lines,
    reason = "Library completions stay centralized so every async reply follows one reducer path"
)]
fn update_library(model: &mut AppModel, reply: LibraryReply) -> Vec<Effect> {
    match reply {
        LibraryReply::BasesLoaded { request_id, result } => {
            update_bases_loaded(model, request_id, result)
        }
        LibraryReply::PropertyDescriptorsLoaded { request_id, result } => {
            update_property_descriptors_loaded(model, request_id, result)
        }
        LibraryReply::BaseRowsLoaded {
            request_id,
            base_id,
            result,
        } => update_base_rows_loaded(model, request_id, base_id, result),
        LibraryReply::BaseRowsAppended {
            request_id,
            base_id,
            result,
        } => update_base_rows_appended(model, request_id, base_id, result),
        LibraryReply::BaseCellEdited {
            request_id,
            moved,
            move_error,
            result,
            ..
        } => update_base_cell_edited(model, request_id, moved, move_error, result),
        LibraryReply::BasePropertiesLoaded { request_id, result } => {
            if model.bases.base_properties_request != Some(request_id) {
                return Vec::new();
            }
            model.bases.base_properties_request = None;
            match result {
                Ok(request) => vec![Effect::ShowDocumentProperties { request }],
                Err(error) => {
                    model.set_notice(error);
                    Vec::new()
                }
            }
        }
        LibraryReply::BaseCreated { result } => update_base_created(model, result),
        LibraryReply::BaseUpdated { result } => update_base_updated(model, result),
        LibraryReply::BaseConfigurationLoaded {
            request_id,
            definition,
            result,
        } => {
            if model.bases.configuration_request != Some(request_id) {
                return Vec::new();
            }
            model.bases.configuration_request = None;
            if model.route != super::Route::Base || model.bases.selected != Some(definition.id) {
                return Vec::new();
            }
            match result {
                Ok(()) => {
                    let descriptors = match &model.bases.property_descriptors.state {
                        super::LoadState::Ready(items) => items.clone(),
                        _ => Vec::new(),
                    };
                    let default_properties = model.config.document_properties.entries.clone();
                    present_base_configuration_dialog(model, request_id);
                    vec![Effect::ShowBaseConfiguration {
                        dialog_id: request_id,
                        definition,
                        descriptors,
                        default_properties,
                    }]
                }
                Err(error) => {
                    model.set_notice(error);
                    Vec::new()
                }
            }
        }
        LibraryReply::NewBaseConfigurationLoaded { request_id, result } => {
            if model.bases.configuration_request != Some(request_id) {
                return Vec::new();
            }
            model.bases.configuration_request = None;
            match result {
                Ok(()) => {
                    let descriptors = match &model.bases.property_descriptors.state {
                        super::LoadState::Ready(items) => items.clone(),
                        _ => Vec::new(),
                    };
                    let default_properties = model.config.document_properties.entries.clone();
                    present_base_configuration_dialog(model, request_id);
                    vec![Effect::ShowNewBaseConfiguration {
                        dialog_id: request_id,
                        descriptors,
                        default_properties,
                    }]
                }
                Err(error) => {
                    model.set_notice(error);
                    Vec::new()
                }
            }
        }
        LibraryReply::BasePreviewCount {
            dialog_id,
            request_id,
            result,
        } => {
            if model.bases.configuration_dialog != Some(dialog_id)
                || model.bases.configuration_preview_request != Some((dialog_id, request_id))
            {
                return Vec::new();
            }
            model.bases.configuration_preview_request = None;
            match result {
                Ok(count) => vec![Effect::UpdateBaseConfigurationPreview { count, dialog_id }],
                Err(error) => {
                    model.set_notice(error);
                    Vec::new()
                }
            }
        }
        LibraryReply::BaseDeleted { base_id, result } => {
            update_base_deleted(model, base_id, result)
        }
        LibraryReply::LibraryRevisionLoaded { request_id, result } => {
            update_library_revision(model, request_id, result)
        }
        LibraryReply::ConfigPersisted { result } => update_config_persisted(model, result),
        LibraryReply::DefaultCategoryEnsured { result } => update_default_category(model, result),
        LibraryReply::NoteCreated { result } => update_created_note(model, result),
        LibraryReply::ActionFinished { action, result } => {
            model.finish_action(action);
            match result {
                Ok(()) => {
                    update_undo_state(model, action);
                    if let ActionKey::TrashNote(note_id) = action {
                        model.undo_trash_note = Some(note_id);
                    }
                    if let ActionKey::TrashCategory(category_id) = action
                        && model.selected_category == Some(category_id)
                    {
                        model.selected_category = None;
                    }
                    model.clear_notice();
                    let mut effects = reload_after_local_mutation(model);
                    if matches!(action, ActionKey::TrashCategory(_)) {
                        effects.push(Effect::EnsureDefaultCategory);
                    }
                    effects
                }
                Err(error) => {
                    model.set_notice(error);
                    Vec::new()
                }
            }
        }
        LibraryReply::NoteMoved { action, result } => update_note_moved(model, action, result),
        LibraryReply::SidebarLoaded { request_id, result } => {
            let mut effects = reload_sidebar_after(model.sidebar.finish(request_id, result), model);
            if matches!(&model.sidebar.state, super::LoadState::Ready(categories) if categories.iter().any(|c| c.category.default_template_id.is_some()))
            {
                effects.extend(super::templates::refresh_catalog(model));
            }
            effects
        }
        LibraryReply::BrowserLoaded {
            request_id,
            result,
            favorites,
        } => update_browser_loaded(model, request_id, result, favorites),
        LibraryReply::BrowserAppended { request_id, result } => {
            update_browser_appended(model, request_id, result)
        }
        LibraryReply::FavoriteChanged { action, result } => {
            update_favorite_changed(model, action, result)
        }
        LibraryReply::EditorRefreshed {
            request_id,
            session,
            snapshot,
            discard_local,
            result,
        } => update_editor_refresh(model, request_id, session, &snapshot, discard_local, result),
        LibraryReply::EditorLoaded {
            request_id,
            tab_id,
            result,
        } => update_editor_loaded(model, request_id, tab_id, result),
        LibraryReply::NoteLinksLoaded {
            request_id,
            note_id,
            result,
        } => update_note_links_loaded(model, request_id, note_id, result),
        LibraryReply::LinkCandidatesLoaded { request_id, result } => {
            let reload = model
                .editor_link_dialog
                .as_mut()
                .is_some_and(|dialog| dialog.candidates.finish(request_id, result));
            if reload {
                search_link_candidates(model)
            } else {
                Vec::new()
            }
        }
        LibraryReply::EditorAssetStored {
            image,
            session,
            alt,
            source_target,
            result,
        } => update_editor_asset_stored(model, session, &alt, source_target, result, image),
        LibraryReply::EditorExportPrepared {
            request_id,
            session,
            result,
        } => update_editor_export_prepared(model, request_id, session, result),
        LibraryReply::EditorExportWritten { request_id, result } => {
            update_editor_export_written(model, request_id, result)
        }
        LibraryReply::TrashLoaded { request_id, result } => {
            reload_trash_after(model.trash.finish(request_id, result), model)
        }
        LibraryReply::TrashMutationFinished { result } => match result {
            Ok(_) => {
                model.clear_notice();
                reload_after_local_mutation(model)
            }
            Err(error) => {
                model.set_notice(error);
                Vec::new()
            }
        },
        LibraryReply::EditorSaved {
            request,
            move_error,
            result,
        } => {
            let session = request.session;
            let effects = update_editor_save(model, &request, move_error, result);
            // A closed tab's document is dropped once its final save succeeds. A failed save has
            // no editor left to show it, so surface the error instead of losing it silently.
            let failed = match model
                .tabs
                .closing
                .get(&session)
                .map(|document| &document.save_state)
            {
                Some(super::EditorSaveState::Failed(error)) => Some(error.clone()),
                _ => None,
            };
            let clean = matches!(
                model
                    .tabs
                    .closing
                    .get(&session)
                    .map(|document| &document.save_state),
                Some(super::EditorSaveState::Clean)
            );
            if let Some(error) = failed {
                // Reopen the draft as a tab so the unsaved source stays reachable and retryable
                // instead of being dropped on exit.
                if let Some(document) = model.tabs.closing.remove(&session) {
                    let title = document.analysis.title().to_owned();
                    let tab_id = push_note_tab(
                        model,
                        document.note_id,
                        super::TabOrigin::Browser,
                        title,
                        document.is_favorite,
                        false,
                    );
                    model.tabs.background.insert(tab_id, document);
                }
                model.set_notice(UiError::new(tr_fmt!(
                    gettext("Could not save note: {error}"),
                    error = error.message
                )));
            } else if clean {
                model.tabs.closing.remove(&session);
            }
            effects
        }
    }
}

fn update_base_deleted(
    model: &mut AppModel,
    base_id: carver_sdk::BaseId,
    result: Result<(), UiError>,
) -> Vec<Effect> {
    if !model.bases.deleting.remove(&base_id) {
        return Vec::new();
    }
    if let Err(error) = result {
        model.set_notice(error);
        return Vec::new();
    }
    clear_missing_base_selection(model, base_id);
    model.clear_notice();
    reload_bases(model).into_iter().collect()
}

fn update_bases_loaded(
    model: &mut AppModel,
    request_id: super::RequestId,
    result: Result<Vec<carver_sdk::BaseDefinition>, UiError>,
) -> Vec<Effect> {
    let reload = model.bases.definitions.finish(request_id, result);
    let (missing_selection, missing_pending_navigation) = if reload {
        (None, None)
    } else {
        match &model.bases.definitions.state {
            super::LoadState::Ready(definitions) => {
                let is_missing = |base_id| {
                    !definitions
                        .iter()
                        .any(|definition| definition.id == base_id)
                };
                let missing_selection = model.bases.selected.filter(|base_id| is_missing(*base_id));
                let missing_pending_navigation = match model.pending_navigation {
                    Some(PendingNavigation::Base(base_id)) if is_missing(base_id) => Some(base_id),
                    _ => None,
                };
                (missing_selection, missing_pending_navigation)
            }
            _ => (None, None),
        }
    };
    if let Some(base_id) = missing_selection {
        clear_missing_base_selection(model, base_id);
    }
    if let Some(base_id) = missing_pending_navigation {
        clear_missing_base_pending_navigation(model, base_id);
    }
    reload
        .then(|| reload_bases(model))
        .flatten()
        .into_iter()
        .collect()
}

fn clear_missing_base_selection(model: &mut AppModel, base_id: carver_sdk::BaseId) {
    if model.bases.selected != Some(base_id) {
        return;
    }
    model.bases.selected = None;
    if model.route == super::Route::Base {
        model.route = super::Route::Browser;
    }
    if model.editor_return_route == super::Route::Base {
        model.editor_return_route = super::Route::Browser;
    }
    clear_missing_base_pending_navigation(model, base_id);
}

fn clear_missing_base_pending_navigation(model: &mut AppModel, base_id: carver_sdk::BaseId) {
    if model.pending_navigation == Some(PendingNavigation::Base(base_id)) {
        model.pending_navigation = Some(super::model::PendingNavigation::Browser(
            model.selected_category,
        ));
    }
}

fn update_property_descriptors_loaded(
    model: &mut AppModel,
    request_id: super::RequestId,
    result: Result<Vec<carver_sdk::PropertyDescriptor>, UiError>,
) -> Vec<Effect> {
    let reload = model.bases.property_descriptors.finish(request_id, result);
    if reload {
        return reload_property_descriptors(model).into_iter().collect();
    }
    resume_pending_base_configuration(model)
}

fn resume_pending_base_configuration(model: &mut AppModel) -> Vec<Effect> {
    let Some(target) = model.bases.pending_configuration.take() else {
        return Vec::new();
    };
    let request_id = model.bases.configuration_request.take();
    let Some(request_id) = request_id else {
        return Vec::new();
    };
    let default_properties = model.config.document_properties.entries.clone();
    match &model.bases.property_descriptors.state {
        super::LoadState::Ready(descriptors) => match target {
            PendingBaseConfiguration::New => {
                let descriptors = descriptors.clone();
                present_base_configuration_dialog(model, request_id);
                vec![Effect::ShowNewBaseConfiguration {
                    dialog_id: request_id,
                    descriptors,
                    default_properties,
                }]
            }
            PendingBaseConfiguration::Existing(definition)
                if model.route == super::Route::Base
                    && model.bases.selected == Some(definition.id) =>
            {
                let descriptors = descriptors.clone();
                present_base_configuration_dialog(model, request_id);
                vec![Effect::ShowBaseConfiguration {
                    dialog_id: request_id,
                    definition,
                    descriptors,
                    default_properties,
                }]
            }
            PendingBaseConfiguration::Existing(_) => Vec::new(),
        },
        super::LoadState::Failed(error) => {
            model.set_notice(error.clone());
            Vec::new()
        }
        super::LoadState::Idle | super::LoadState::Loading(_) => {
            model.bases.configuration_request = Some(request_id);
            model.bases.pending_configuration = Some(target);
            Vec::new()
        }
    }
}

fn update_base_rows_loaded(
    model: &mut AppModel,
    request_id: super::RequestId,
    _base_id: carver_sdk::BaseId,
    result: Result<carver_sdk::Page<carver_sdk::BaseRow>, UiError>,
) -> Vec<Effect> {
    let current = matches!(
        model.bases.rows.state,
        super::LoadState::Loading(current) if current == request_id
    );
    let result = result.map(|page| {
        if current {
            model.bases.rows_next_offset = page.items.len();
            model.bases.rows_has_more = page.has_more;
            model.bases.rows_append_request = None;
            model.bases.rows_append_error = None;
        }
        page.items
    });
    let reload = model.bases.rows.finish(request_id, result);
    if reload {
        return model
            .bases
            .selected
            .and_then(|selected| reload_base_rows(model, selected))
            .into_iter()
            .collect();
    }
    Vec::new()
}

fn update_base_rows_appended(
    model: &mut AppModel,
    request_id: super::RequestId,
    base_id: carver_sdk::BaseId,
    result: Result<carver_sdk::Page<carver_sdk::BaseRow>, UiError>,
) -> Vec<Effect> {
    if model.bases.selected != Some(base_id) || model.bases.rows_append_request != Some(request_id)
    {
        return Vec::new();
    }
    model.bases.rows_append_request = None;
    match result {
        Ok(page) => {
            let Some(rows) = (match &mut model.bases.rows.state {
                super::LoadState::Ready(rows) => Some(rows),
                _ => None,
            }) else {
                return Vec::new();
            };
            model.bases.rows_next_offset += page.items.len();
            model.bases.rows_has_more = page.has_more;
            model.bases.rows_append_error = None;
            rows.extend(page.items);
        }
        Err(error) => model.bases.rows_append_error = Some(error),
    }
    Vec::new()
}

fn update_base_created(
    model: &mut AppModel,
    result: Result<carver_sdk::BaseDefinition, UiError>,
) -> Vec<Effect> {
    let was_saving = std::mem::take(&mut model.bases.saving_configuration);
    let completion: Vec<_> = was_saving
        .then_some(Effect::FinishBaseConfiguration {
            success: result.is_ok(),
        })
        .into_iter()
        .collect();
    match result {
        Ok(base) => {
            model.clear_notice();
            let mut effects = completion;
            effects.extend(reload_bases(model));
            effects.extend(update_bases(model, BasesMsg::Open(base.id)));
            effects
        }
        Err(error) => {
            model.set_notice(error);
            completion
        }
    }
}

fn update_base_updated(
    model: &mut AppModel,
    result: Result<carver_sdk::BaseDefinition, UiError>,
) -> Vec<Effect> {
    let was_saving = std::mem::take(&mut model.bases.saving_configuration);
    let completion: Vec<_> = was_saving
        .then_some(Effect::FinishBaseConfiguration {
            success: result.is_ok(),
        })
        .into_iter()
        .collect();
    match result {
        Ok(base) => {
            model.clear_notice();
            let mut effects = completion;
            effects.extend(reload_bases(model));
            if model.route == super::Route::Base && model.bases.selected == Some(base.id) {
                effects.extend(reload_base_rows(model, base.id));
            }
            effects
        }
        Err(error) => {
            model.set_notice(error);
            completion
        }
    }
}

fn update_favorite_changed(
    model: &mut AppModel,
    action: ActionKey,
    result: Result<carver_sdk::Note, UiError>,
) -> Vec<Effect> {
    model.finish_action(action);
    match result {
        Ok(note) => {
            model.clear_notice();
            let (rebased_save, pending_favorite, close_requested, favorite_session) =
                if let Some(document) = model.document_for_note_mut(note.id) {
                    let save_needs_rebase = matches!(
                        &document.save_state,
                        super::EditorSaveState::Saving(request)
                            if request.expected_revision != note.revision
                    );
                    document.revision = note.revision;
                    document.is_favorite = note.is_favorite;
                    document.favorite_mutation_in_flight = false;
                    let pending_favorite = document
                        .pending_favorite
                        .filter(|is_favorite| *is_favorite != note.is_favorite);
                    if pending_favorite.is_none() {
                        document.pending_favorite = None;
                    }
                    let rebased_save = if save_needs_rebase {
                        document.save_state = super::EditorSaveState::Dirty;
                        document.begin_save()
                    } else {
                        None
                    };
                    (
                        rebased_save,
                        pending_favorite,
                        document.close_is_requested(),
                        Some(document.session),
                    )
                } else {
                    (None, None, false, None)
                };
            let save_was_rebased = rebased_save.is_some();
            let mut effects = rebased_save.map_or_else(Vec::new, save_note_effect);
            if !save_was_rebased {
                effects.extend(pending_favorite.map_or_else(Vec::new, |is_favorite| {
                    favorite_session.map_or_else(Vec::new, |session| {
                        set_document_favorite(model, session, is_favorite)
                    })
                }));
                if pending_favorite.is_none() && close_requested {
                    let session = model.editor.as_ref().map(|document| document.session);
                    effects.extend(
                        session.map_or_else(Vec::new, |session| discard_editor(model, session)),
                    );
                    let pending_effects = complete_pending_navigation(model);
                    if pending_effects.is_empty() {
                        effects.extend(reload_browser(model));
                    } else {
                        effects.extend(pending_effects);
                    }
                }
            }
            effects.extend(reload_after_local_mutation(model));
            effects
        }
        Err(error) => {
            if let ActionKey::SetNoteFavorite(note_id) = action
                && let Some(document) = model.document_for_note_mut(note_id)
            {
                document.favorite_mutation_in_flight = false;
                document.pending_favorite = None;
            }
            model.set_notice(error);
            Vec::new()
        }
    }
}

fn update_note_moved(
    model: &mut AppModel,
    action: ActionKey,
    result: Result<carver_sdk::Note, UiError>,
) -> Vec<Effect> {
    model.finish_action(action);
    match result {
        Ok(note) => {
            model.clear_notice();
            update_undo_state(model, action);
            // A move keeps the content but advances the revision, so reconcile any open
            // editor document before its next autosave would conflict.
            let rebased_save = reconcile_moved_document(model, &note);
            let mut effects = rebased_save.map_or_else(Vec::new, save_note_effect);
            effects.extend(reload_after_local_mutation(model));
            effects
        }
        Err(error) => {
            model.set_notice(error);
            Vec::new()
        }
    }
}

/// Adopts a moved note's revision and category in an open editor document.
fn reconcile_moved_document(
    model: &mut AppModel,
    note: &carver_sdk::Note,
) -> Option<EditorSaveRequest> {
    let document = model.document_for_note_mut(note.id)?;
    let save_needs_rebase = matches!(
        &document.save_state,
        super::EditorSaveState::Saving(request) if request.expected_revision != note.revision
    );
    document.revision = note.revision;
    document.category_id = note.category_id;
    if save_needs_rebase {
        document.save_state = super::EditorSaveState::Dirty;
        return document.begin_save();
    }
    None
}

fn update_editor_asset_stored(
    model: &mut AppModel,
    session: super::EditorSessionId,
    alt: &str,
    source_target: Option<super::SourceImageTarget>,
    result: Result<String, UiError>,
    image: bool,
) -> Vec<Effect> {
    let Some(document) = model.document_for_session_mut(session) else {
        return Vec::new();
    };
    document.pending_assets = document.pending_assets.saturating_sub(1);
    match result {
        Ok(path) => {
            let source = if image {
                image_source(&document.source, alt, &path, source_target)
            } else {
                attachment_source(&document.source, alt, &path, source_target)
            };
            if !document.source_changed(source) {
                return Vec::new();
            }
            let source = document.source.clone();
            // Asset storage can complete after the user leaves the tab; schedule the save on the
            // document that owns the asset, and only preview the active one.
            let mut effects = Vec::new();
            if model
                .editor
                .as_ref()
                .is_some_and(|document| document.session == session)
            {
                effects.extend(schedule_preview(model));
            }
            effects.extend(schedule_editor_save_for(model, session));
            effects.push(Effect::ReloadRichEditor { session, source });
            effects
        }
        Err(error) => {
            model.set_notice(error);
            Vec::new()
        }
    }
}

fn update_browser_loaded(
    model: &mut AppModel,
    request_id: super::RequestId,
    result: Result<carver_sdk::Page<carver_sdk::NoteSummary>, UiError>,
    favorites: Result<Vec<carver_sdk::NoteSummary>, UiError>,
) -> Vec<Effect> {
    let is_current = matches!(
        model.browser.notes.state,
        super::LoadState::Loading(current) if current == request_id
    );
    let result = result.map(|page| {
        if is_current {
            model.browser.next_offset = page.items.len();
            model.browser.has_more = page.has_more;
            model.browser.append_request = None;
            model.browser.append_error = None;
        }
        page.items
    });
    let reload = model.browser.notes.finish(request_id, result);
    if is_current {
        if !reload {
            if let super::LoadState::Ready(notes) = &model.browser.notes.state {
                model.browser.last_ready_notes = Some(notes.clone());
            }
            model.browser.favorites.state = match favorites {
                Ok(notes) => super::LoadState::Ready(notes),
                Err(error) => super::LoadState::Failed(error),
            };
        }
        model.browser.loading_indicator_request = None;
        model.browser.loading_indicator_visible = false;
    }
    reload_browser_after(reload, model)
}

fn update_browser_appended(
    model: &mut AppModel,
    request_id: super::RequestId,
    result: Result<carver_sdk::Page<carver_sdk::NoteSummary>, UiError>,
) -> Vec<Effect> {
    if model.browser.append_request != Some(request_id) {
        return Vec::new();
    }
    model.browser.append_request = None;
    match result {
        Ok(page) => {
            let Some(notes) = (match &mut model.browser.notes.state {
                super::LoadState::Ready(notes) => Some(notes),
                _ => None,
            }) else {
                return Vec::new();
            };
            model.browser.next_offset += page.items.len();
            model.browser.has_more = page.has_more;
            model.browser.append_error = None;
            notes.extend(page.items);
        }
        Err(error) => model.browser.append_error = Some(error),
    }
    Vec::new()
}

fn update_editor_loaded(
    model: &mut AppModel,
    request_id: super::RequestId,
    tab_id: super::TabId,
    result: Result<carver_sdk::Note, UiError>,
) -> Vec<Effect> {
    if model.note_tab(tab_id).is_none() {
        return Vec::new();
    }
    if model.editor_load_request == Some(request_id) {
        model.editor_load_request = None;
    }
    let export_after_load = model.editor_export_after_load == Some(request_id);
    if export_after_load {
        model.editor_export_after_load = None;
    }
    match result {
        Ok(note) => {
            if let Some(tab) = model.note_tab_mut(tab_id) {
                tab.title.clone_from(&note.title);
                tab.is_favorite = note.is_favorite;
                tab.loading = false;
            }
            let is_active = model.tabs.active == Some(tab_id);
            // A Base-triggered reload is asynchronous. If the user edited the note before the
            // reply arrived, keep the local draft instead of replacing it with the snapshot.
            if model.document_for_note(note.id).is_some_and(|document| {
                matches!(
                    document.save_state,
                    super::EditorSaveState::Dirty
                        | super::EditorSaveState::Saving(_)
                        | super::EditorSaveState::Failed(_)
                )
            }) {
                return Vec::new();
            }
            if !is_active {
                let document = new_editor_document(
                    model,
                    note.id,
                    note.category_id,
                    note.revision,
                    note.is_favorite,
                    note.source,
                    super::model::DocumentOrigin::Library,
                );
                model.tabs.background.insert(tab_id, document);
                return Vec::new();
            }
            let session = open_editor(
                model,
                note.id,
                note.category_id,
                note.revision,
                note.is_favorite,
                note.source,
                super::model::DocumentOrigin::Library,
            );
            model.route = super::Route::Editor;
            let mut effects = if export_after_load {
                request_editor_export_dialog(model)
            } else {
                vec![Effect::FocusEditor { session }]
            };
            effects.extend(reload_note_links(model));
            effects
        }
        Err(error) => {
            model.set_notice(error);
            remove_note_tab(model, tab_id)
        }
    }
}

fn update_note_links_loaded(
    model: &mut AppModel,
    request_id: super::RequestId,
    note_id: carver_sdk::NoteId,
    result: Result<carver_sdk::NoteLinks, UiError>,
) -> Vec<Effect> {
    let reload = model
        .document_for_note_mut(note_id)
        .is_some_and(|document| document.links.finish(request_id, result));
    if reload {
        reload_note_links_for(model, note_id).into_iter().collect()
    } else {
        Vec::new()
    }
}

/// Requests the active note's link index, coalescing reloads while one is in flight.
fn reload_note_links(model: &mut AppModel) -> Option<Effect> {
    let note_id = model.editor.as_ref()?.note_id;
    reload_note_links_for(model, note_id)
}

/// Requests one note's link index, active or background.
fn reload_note_links_for(model: &mut AppModel, note_id: carver_sdk::NoteId) -> Option<Effect> {
    let request_id = model.next_request_id();
    let document = model.document_for_note_mut(note_id)?;
    document
        .links
        .begin_reload(request_id)
        .then_some(Effect::LoadNoteLinks {
            request_id,
            note_id,
        })
}

fn update_editor_export_prepared(
    model: &mut AppModel,
    request_id: u64,
    session: super::EditorSessionId,
    result: Result<Vec<String>, UiError>,
) -> Vec<Effect> {
    if model.editor_export_progress
        != Some(super::EditorExportProgress {
            request_id,
            session,
        })
        || model
            .editor
            .as_ref()
            .is_none_or(|document| document.session != session)
    {
        return vec![Effect::DiscardEditorExport { request_id }];
    }
    match result {
        Ok(warnings) if warnings.is_empty() => vec![Effect::WriteEditorExport { request_id }],
        Ok(warnings) => {
            let warning_request = super::EditorExportWarningRequest {
                request_id,
                session,
                warnings,
            };
            model.editor_export_warning_request = Some(warning_request.clone());
            vec![Effect::ShowEditorExportWarning {
                request: warning_request,
            }]
        }
        Err(error) => {
            model.editor_export_progress = None;
            model.set_notice(error);
            Vec::new()
        }
    }
}

fn update_editor_export_written(
    model: &mut AppModel,
    request_id: u64,
    result: Result<(), UiError>,
) -> Vec<Effect> {
    if model
        .editor_export_progress
        .as_ref()
        .is_none_or(|request| request.request_id != request_id)
    {
        return Vec::new();
    }
    model.editor_export_warning_request = None;
    model.editor_export_progress = None;
    match result {
        Ok(()) => model.set_notice(UiError::new("Note exported")),
        Err(error) => model.set_notice(error),
    }
    Vec::new()
}

fn update_created_note(
    model: &mut AppModel,
    result: Result<carver_sdk::Note, UiError>,
) -> Vec<Effect> {
    match result {
        Ok(note) => {
            let session = open_note_as_active_tab(model, &note);
            let mut effects = vec![Effect::FocusEditor { session }];
            effects.extend(reload_note_links(model));
            effects.extend(reload_after_local_mutation(model));
            effects
        }
        Err(error) => {
            model.set_notice(error);
            Vec::new()
        }
    }
}

fn update_config_persisted(model: &mut AppModel, result: Result<(), UiError>) -> Vec<Effect> {
    if let Err(error) = result {
        model.set_notice(error);
    }
    Vec::new()
}

fn update_default_category(model: &mut AppModel, result: Result<(), UiError>) -> Vec<Effect> {
    match result {
        Ok(()) => {
            let mut effects = [
                reload_sidebar(model),
                reload_bases(model),
                reload_property_descriptors(model),
                reload_browser(model),
            ]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();
            effects.extend(request_library_revision(
                model,
                LibraryRevisionCheckReason::InitialLoad,
            ));
            effects
        }
        Err(error) => {
            model.set_notice(error);
            Vec::new()
        }
    }
}

fn store_editor_asset_effect(
    model: &mut AppModel,
    extension: String,
    bytes: Vec<u8>,
    alt: String,
    source_target: Option<super::SourceImageTarget>,
    image: bool,
) -> Vec<Effect> {
    let Some(document) = model
        .editor
        .as_mut()
        .filter(|document| document.mode != carver_config::EditorMode::Rendered)
    else {
        return Vec::new();
    };
    document.pending_assets += 1;
    vec![Effect::StoreEditorAsset {
        image,
        session: document.session,
        note_id: document.note_id,
        extension,
        bytes,
        alt,
        source_target,
    }]
}

fn image_source(
    source: &str,
    alt: &str,
    path: &str,
    source_target: Option<super::SourceImageTarget>,
) -> String {
    let markup = format!("![{alt}]({path})");
    insert_media_markup(source, &markup, source_target)
}

fn insert_media_markup(
    source: &str,
    markup: &str,
    source_target: Option<super::SourceImageTarget>,
) -> String {
    let Some(target) = source_target.filter(|target| target.source == source) else {
        return append_image_source(source, markup);
    };
    let start = character_byte_offset(source, target.selection.start);
    let end = character_byte_offset(source, target.selection.end.max(target.selection.start));
    let mut inserted = source.to_owned();
    inserted.replace_range(start..end, markup);
    inserted
}

fn append_image_source(source: &str, markup: &str) -> String {
    let mut source = source.to_owned();
    if !source.is_empty() && !source.ends_with('\n') {
        source.push('\n');
    }
    source.push_str(markup);
    source.push('\n');
    source
}

fn attachment_source(
    source: &str,
    name: &str,
    path: &str,
    source_target: Option<super::SourceImageTarget>,
) -> String {
    let label = carve_link_label(name);
    let markup = format!("[{label}]({path})");
    let Some(target) = source_target.filter(|target| target.source == source) else {
        return append_image_source(source, &markup);
    };
    let start = character_byte_offset(source, target.selection.start);
    let end = character_byte_offset(source, target.selection.end.max(target.selection.start));
    let mut inserted = source.to_owned();
    inserted.replace_range(start..end, &markup);
    inserted
}

/// Produces link text that remains literal inside canonical Carve markup.
fn carve_link_label(label: &str) -> String {
    label.chars().fold(String::new(), |mut escaped, character| {
        match character {
            '\\' => escaped.push_str("\\\\"),
            '[' | ']' | '\n' | '\r' => {}
            _ => escaped.push(character),
        }
        escaped
    })
}

fn character_byte_offset(source: &str, offset: usize) -> usize {
    source
        .char_indices()
        .nth(offset)
        .map_or(source.len(), |(index, _)| index)
}

pub(super) fn schedule_editor_save(model: &mut AppModel) -> Option<Effect> {
    let session = model.editor.as_ref()?.session;
    schedule_editor_save_for(model, session)
}

/// Schedules a debounced save for one document, active or background.
fn schedule_editor_save_for(
    model: &mut AppModel,
    session: super::EditorSessionId,
) -> Option<Effect> {
    let timer_id = model.next_timer_id();
    let delay_ms = model.preferences.autosave_delay_ms;
    let document = model.document_for_session_mut(session)?;
    if matches!(document.save_state, super::EditorSaveState::Saving(_)) {
        return None;
    }
    document.schedule_save(timer_id);
    Some(Effect::ScheduleEditorSave {
        session,
        timer_id,
        delay_ms,
    })
}

fn save_note_effect(request: EditorSaveRequest) -> Vec<Effect> {
    vec![Effect::SaveNote { request }]
}

fn update_editor_save(
    model: &mut AppModel,
    request: &EditorSaveRequest,
    mut move_error: Option<UiError>,
    result: Result<carver_sdk::Revision, UiError>,
) -> Vec<Effect> {
    let (close_requested, pending_favorite, move_notice, moved) = {
        let Some(document) = model.document_for_session_mut(request.session) else {
            return Vec::new();
        };
        if document.note_id != request.note_id
            || document.revision != request.expected_revision
            || document.save_state != super::EditorSaveState::Saving(request.clone())
        {
            return Vec::new();
        }
        match result {
            Ok(revision) => {
                // The content save is authoritative. A failed move is reported but does not keep
                // the note on a stale revision, so the editor stays consistent.
                let mut moved = false;
                let move_notice = match request.move_to {
                    Some(category) if move_error.is_none() => {
                        document.category_id = category;
                        moved = true;
                        None
                    }
                    Some(_) => move_error.take(),
                    None => None,
                };
                document.revision = revision;
                // Re-save when the source changed or a category change arrived while this save was
                // in flight, so the queued move is not dropped.
                if document.source == request.source && document.pending_category.is_none() {
                    document.save_state = super::EditorSaveState::Clean;
                    (
                        document.close_is_requested(),
                        (!document.favorite_mutation_in_flight)
                            .then_some(document.pending_favorite)
                            .flatten(),
                        move_notice,
                        moved,
                    )
                } else {
                    document.save_state = super::EditorSaveState::Dirty;
                    return document
                        .begin_save()
                        .map_or_else(Vec::new, save_note_effect);
                }
            }
            Err(error) if document.source == request.source => {
                // Keep the requested move so a retry still applies it.
                document.pending_category = document.pending_category.or(request.move_to);
                document.save_state = super::EditorSaveState::Failed(error);
                return Vec::new();
            }
            Err(_) => {
                document.pending_category = document.pending_category.or(request.move_to);
                document.save_state = super::EditorSaveState::Dirty;
                return document
                    .begin_save()
                    .map_or_else(Vec::new, save_note_effect);
            }
        }
    };
    if let Some(notice) = move_notice {
        model.set_notice(notice);
    }
    let mut effects = if let Some(is_favorite) = pending_favorite {
        set_document_favorite(model, request.session, is_favorite)
    } else {
        Vec::new()
    };
    if moved {
        // A move changes which categories own the note, so refresh the sidebar counts and the
        // browser list even though the save came from the editor rather than an action.
        effects.extend(reload_sidebar(model));
        effects.extend(reload_browser(model));
    }
    if close_requested {
        restore_editor_origin(model);
        model.editor = None;
        model.editor_preview = None;
        model.preview_timer = None;
    }
    if close_requested {
        let pending_effects = complete_pending_navigation(model);
        if pending_effects.is_empty() {
            effects.extend(reload_return_surface(model));
        } else {
            effects.extend(pending_effects);
        }
    }
    // Refresh the visible note's links after any save: a background tab may have added or removed
    // a backlink to the note currently on screen.
    if !close_requested {
        effects.extend(reload_note_links(model));
    }
    effects.extend(request_library_revision(
        model,
        LibraryRevisionCheckReason::LocalMutation,
    ));
    effects
}

fn reload_return_surface(model: &mut AppModel) -> Vec<Effect> {
    match (model.route, model.bases.selected) {
        (super::Route::Base, Some(base_id)) => {
            [reload_base_rows(model, base_id), reload_browser(model)]
                .into_iter()
                .flatten()
                .collect()
        }
        _ => reload_browser(model).into_iter().collect(),
    }
}

fn update_undo_state(model: &mut AppModel, action: ActionKey) {
    match action {
        ActionKey::MoveNote {
            note_id,
            source_category_id,
        } => {
            model.undo_move = Some(MoveUndo {
                note_id,
                source_category_id,
            });
        }
        ActionKey::UndoMove(_) => model.undo_move = None,
        ActionKey::CreateCategory
        | ActionKey::RenameCategory(_)
        | ActionKey::UpdateCategory(_)
        | ActionKey::TrashCategory(_)
        | ActionKey::TrashNote(_)
        | ActionKey::SetNoteFavorite(_) => {}
    }
}

fn reload_sidebar_after(reload: bool, model: &mut AppModel) -> Vec<Effect> {
    reload
        .then(|| reload_sidebar(model))
        .flatten()
        .into_iter()
        .collect()
}

fn reload_browser_after(reload: bool, model: &mut AppModel) -> Vec<Effect> {
    reload
        .then(|| reload_browser(model))
        .flatten()
        .into_iter()
        .collect()
}

fn reload_trash_after(reload: bool, model: &mut AppModel) -> Vec<Effect> {
    reload
        .then(|| reload_trash(model))
        .flatten()
        .into_iter()
        .collect()
}

fn reload_all_resources(model: &mut AppModel) -> Vec<Effect> {
    let mut effects: Vec<_> = [
        reload_sidebar(model),
        reload_bases(model),
        reload_property_descriptors(model),
        reload_browser(model),
        reload_trash(model),
    ]
    .into_iter()
    .flatten()
    .collect();
    if let Some(base_id) = model.bases.selected {
        effects.extend(reload_base_rows(model, base_id));
    }
    effects
}

fn reload_after_local_mutation(model: &mut AppModel) -> Vec<Effect> {
    let mut effects = reload_all_resources(model);
    effects.extend(request_library_revision(
        model,
        LibraryRevisionCheckReason::LocalMutation,
    ));
    effects
}

pub(super) fn request_library_revision(
    model: &mut AppModel,
    reason: LibraryRevisionCheckReason,
) -> Option<Effect> {
    if model.library_revision_request.is_some() {
        model.library_revision_pending = true;
        return None;
    }
    let request_id = model.next_request_id();
    model.library_revision_request = Some(LibraryRevisionRequest { request_id, reason });
    Some(Effect::LoadLibraryRevision { request_id })
}

fn update_library_revision(
    model: &mut AppModel,
    request_id: super::RequestId,
    result: Result<carver_sdk::LibraryRevision, UiError>,
) -> Vec<Effect> {
    let Some(request) = model.library_revision_request else {
        return Vec::new();
    };
    if request.request_id != request_id {
        return Vec::new();
    }
    model.library_revision_request = None;
    let mut effects = match result {
        Ok(revision) => {
            let changed = model
                .library_revision
                .is_none_or(|current| current != revision);
            model.library_revision = Some(revision);
            let retry = model.editor_refresh_retry.is_some_and(|session| {
                model
                    .editor
                    .as_ref()
                    .is_some_and(|document| document.session == session)
            });
            if (changed && request.reason != LibraryRevisionCheckReason::LocalTemplateMutation)
                || retry
            {
                let mut effects =
                    if changed && request.reason == LibraryRevisionCheckReason::ExternalWakeup {
                        reload_all_resources(model)
                    } else {
                        Vec::new()
                    };
                effects.extend(refresh_open_editor(model, false));
                // A library change by another writer can add or drop backlinks without changing
                // the open note's own revision, so refresh its links too.
                effects.extend(reload_note_links(model));
                effects
            } else {
                Vec::new()
            }
        }
        Err(error) => {
            model.set_notice(error);
            Vec::new()
        }
    };
    if std::mem::take(&mut model.library_revision_pending) {
        effects.extend(request_library_revision(
            model,
            LibraryRevisionCheckReason::ExternalWakeup,
        ));
    }
    effects
}

fn refresh_open_editor(model: &mut AppModel, discard_local: bool) -> Option<Effect> {
    let document = model.editor.as_ref()?;
    if !discard_local
        && (model.editor_refresh_request.is_some()
            || matches!(document.save_state, super::EditorSaveState::Saving(_))
            || document.favorite_mutation_in_flight)
    {
        model.editor_refresh_pending = true;
        return None;
    }
    let request_id = model.next_request_id();
    let document = model.editor.as_ref()?;
    model.editor_refresh_retry = None;
    model.editor_refresh_request = Some(request_id);
    Some(Effect::RefreshEditorNote {
        request_id,
        session: document.session,
        snapshot: EditorSaveRequest {
            session: document.session,
            note_id: document.note_id,
            expected_revision: document.revision,
            source: document.source.clone(),
            move_to: document.pending_category,
        },
        discard_local,
    })
}

fn update_editor_refresh(
    model: &mut AppModel,
    request_id: super::RequestId,
    session: super::EditorSessionId,
    snapshot: &EditorSaveRequest,
    discard_local: bool,
    result: Result<Option<carver_sdk::Note>, UiError>,
) -> Vec<Effect> {
    if model.editor_refresh_request != Some(request_id) {
        return Vec::new();
    }
    model.editor_refresh_request = None;
    let Some(document) = model
        .editor
        .as_mut()
        .filter(|document| document.session == session && document.note_id == snapshot.note_id)
    else {
        return Vec::new();
    };
    if document.revision != snapshot.expected_revision
        || matches!(document.save_state, super::EditorSaveState::Saving(_))
        || document.favorite_mutation_in_flight
    {
        model.editor_refresh_pending = true;
        return Vec::new();
    }
    let note = match result {
        Ok(note) => note,
        Err(error) => {
            model.editor_refresh_retry = Some(session);
            model.set_notice(error);
            return Vec::new();
        }
    };
    let Some(note) = note.filter(|note| note.trashed_at.is_none()) else {
        if document.external_change.replace(ExternalChange::Deleted)
            == Some(ExternalChange::Deleted)
        {
            return Vec::new();
        }
        return vec![Effect::ShowExternalEdit {
            session,
            deleted: true,
        }];
    };
    if note.revision == document.revision && document.external_change.is_none() {
        return Vec::new();
    }
    let safe = document.source == snapshot.source
        && (discard_local || document.save_state == super::EditorSaveState::Clean);
    if !safe {
        if matches!(
            document
                .external_change
                .replace(ExternalChange::Edited(note.revision)),
            Some(ExternalChange::Edited(_))
        ) {
            return Vec::new();
        }
        return vec![Effect::ShowExternalEdit {
            session,
            deleted: false,
        }];
    }
    document.accept_external_note(&note);
    model.editor_preview = Some(super::EditorPreview {
        session,
        source: note.source.clone(),
    });
    model.preview_timer = None;
    vec![Effect::ReloadRichEditor {
        session,
        source: note.source,
    }]
}

pub(super) fn reload_sidebar(model: &mut AppModel) -> Option<Effect> {
    let request_id = model.next_request_id();
    model
        .sidebar
        .begin_reload(request_id)
        .then_some(Effect::LoadSidebar { request_id })
}

fn reload_bases(model: &mut AppModel) -> Option<Effect> {
    let request_id = model.next_request_id();
    model
        .bases
        .definitions
        .begin_reload(request_id)
        .then_some(Effect::LoadBases { request_id })
}

fn reload_property_descriptors(model: &mut AppModel) -> Option<Effect> {
    let request_id = model.next_request_id();
    model
        .bases
        .property_descriptors
        .begin_reload(request_id)
        .then_some(Effect::LoadPropertyDescriptors { request_id })
}

fn reload_base_rows(model: &mut AppModel, base_id: carver_sdk::BaseId) -> Option<Effect> {
    let request_id = model.next_request_id();
    let started = model
        .bases
        .rows
        .begin_reload(request_id)
        .then_some(Effect::LoadBaseRows {
            request_id,
            base_id,
            query: model.bases.search_query.clone(),
        });
    if started.is_some() {
        model.bases.rows_next_offset = 0;
        model.bases.rows_has_more = false;
        model.bases.rows_append_request = None;
        model.bases.rows_append_error = None;
    }
    started
}

fn load_more_base_rows(model: &mut AppModel) -> Option<Effect> {
    let base_id = model.bases.selected?;
    if !model.bases.rows_has_more
        || model.bases.rows_append_request.is_some()
        || !matches!(model.bases.rows.state, super::LoadState::Ready(_))
    {
        return None;
    }
    let request_id = model.next_request_id();
    model.bases.rows_append_request = Some(request_id);
    model.bases.rows_append_error = None;
    Some(Effect::LoadMoreBaseRows {
        request_id,
        base_id,
        query: model.bases.search_query.clone(),
        offset: model.bases.rows_next_offset,
    })
}

/// Admits one inline Base cell edit and returns the effect that persists it.
fn commit_base_cell_edit(
    model: &mut AppModel,
    note_id: carver_sdk::NoteId,
    path: String,
    revision: carver_sdk::Revision,
    value: Option<serde_json::Value>,
) -> Vec<Effect> {
    if model.route != super::Route::Base || model.bases.selected.is_none() {
        return Vec::new();
    }
    // Only one edit per note may be in flight: a second edit would carry the pre-save revision and
    // conflict once the first bumps it. Queue it so the change is applied once the note reloads
    // instead of being dropped.
    if model
        .bases
        .cell_edits
        .iter()
        .any(|edit| edit.note_id == note_id)
    {
        model
            .bases
            .pending_cell_edits
            .push(super::PendingBaseCellEdit {
                note_id,
                path,
                value,
            });
        return Vec::new();
    }
    admit_base_cell_edit(model, note_id, path, revision, value)
}

/// Dispatches one inline Base cell edit and records it as in flight.
fn admit_base_cell_edit(
    model: &mut AppModel,
    note_id: carver_sdk::NoteId,
    path: String,
    revision: carver_sdk::Revision,
    value: Option<serde_json::Value>,
) -> Vec<Effect> {
    let request_id = model.next_request_id();
    model.bases.cell_edits.push(super::BaseCellEdit {
        note_id,
        path: path.clone(),
        revision,
        request_id,
    });
    vec![Effect::EditBaseCell {
        request_id,
        note_id,
        revision,
        format: model.config.document_properties.format,
        path,
        value,
    }]
}

/// Retires a finished inline cell edit and reloads the grid so moved rows re-project.
fn update_base_cell_edited(
    model: &mut AppModel,
    request_id: super::RequestId,
    moved: bool,
    move_error: Option<UiError>,
    result: Result<carver_sdk::Revision, UiError>,
) -> Vec<Effect> {
    let Some(position) = model
        .bases
        .cell_edits
        .iter()
        .position(|edit| edit.request_id == request_id)
    else {
        return Vec::new();
    };
    let finished = model.bases.cell_edits.remove(position);
    let next_revision = match &result {
        Ok(revision) => *revision,
        Err(_) => finished.revision,
    };
    let mut effects = match result {
        Ok(_) => {
            let mut effects: Vec<Effect> = match (model.route, model.bases.selected) {
                (super::Route::Base, Some(base_id)) => {
                    reload_base_rows(model, base_id).into_iter().collect()
                }
                _ => Vec::new(),
            };
            if moved {
                // A move changes which categories own the note, so refresh the sidebar counts and
                // the browser list as the editor move path does.
                effects.extend(reload_sidebar(model));
                effects.extend(reload_browser(model));
            }
            // Reload an open editor tab for the edited note so its frontmatter is not stale.
            if let Some(tab_id) = model.note_tab_for_note(finished.note_id) {
                let clean = model
                    .document_for_note(finished.note_id)
                    .is_none_or(|document| {
                        matches!(document.save_state, super::EditorSaveState::Clean)
                    });
                if clean {
                    let request_id = model.next_request_id();
                    effects.push(Effect::LoadEditorNote {
                        request_id,
                        tab_id,
                        note_id: finished.note_id,
                    });
                }
            }
            if let Some(error) = move_error {
                // Content saved but the move failed; the grid reloads and the user is told.
                model.set_notice(error);
            }
            effects
        }
        Err(error) => {
            model.set_notice(error);
            // A failed edit left the always-visible control showing a value that was not
            // persisted; reload so it reflects the stored row again.
            match (model.route, model.bases.selected) {
                (super::Route::Base, Some(base_id)) => {
                    reload_base_rows(model, base_id).into_iter().collect()
                }
                _ => Vec::new(),
            }
        }
    };
    // Apply the next queued edit for this note against the revision the first save produced.
    if let Some(index) = model
        .bases
        .pending_cell_edits
        .iter()
        .position(|pending| pending.note_id == finished.note_id)
    {
        let pending = model.bases.pending_cell_edits.remove(index);
        effects.extend(admit_base_cell_edit(
            model,
            pending.note_id,
            pending.path,
            next_revision,
            pending.value,
        ));
    }
    effects
}

pub(super) fn reload_browser(model: &mut AppModel) -> Option<Effect> {
    let request_id = model.next_request_id();
    let started = model.browser.notes.begin_reload(request_id);
    if started {
        model.browser.loading_indicator_request = Some(request_id);
        model.browser.loading_indicator_visible = false;
        model.browser.next_offset = 0;
        model.browser.has_more = false;
        model.browser.append_request = None;
        model.browser.append_error = None;
    }
    started.then_some(Effect::LoadBrowser {
        request_id,
        category_id: model.selected_category,
        query: model.browser.search_query.clone(),
    })
}

fn load_more_browser(model: &mut AppModel) -> Option<Effect> {
    if !model.browser.has_more
        || model.browser.append_request.is_some()
        || !matches!(model.browser.notes.state, super::LoadState::Ready(_))
    {
        return None;
    }
    let request_id = model.next_request_id();
    model.browser.append_request = Some(request_id);
    model.browser.append_error = None;
    Some(Effect::LoadMoreBrowser {
        request_id,
        category_id: model.selected_category,
        query: model.browser.search_query.clone(),
        offset: model.browser.next_offset,
    })
}

fn toggle_editor_favorite(model: &mut AppModel) -> Vec<Effect> {
    let Some(document) = model.editor.as_mut() else {
        return Vec::new();
    };
    let is_favorite = !document.pending_favorite.unwrap_or(document.is_favorite);
    document.pending_favorite = Some(is_favorite);
    if matches!(document.save_state, super::EditorSaveState::Clean) {
        return set_editor_favorite(model, is_favorite);
    }
    document
        .begin_save()
        .map_or_else(Vec::new, save_note_effect)
}

fn set_editor_favorite(model: &mut AppModel, is_favorite: bool) -> Vec<Effect> {
    let Some(session) = model.editor.as_ref().map(|document| document.session) else {
        return Vec::new();
    };
    set_document_favorite(model, session, is_favorite)
}

/// Applies a favorite change to the document owning `session`.
fn set_document_favorite(
    model: &mut AppModel,
    session: super::EditorSessionId,
    is_favorite: bool,
) -> Vec<Effect> {
    let Some((note_id, revision, current, in_flight)) =
        model.document_for_session(session).map(|document| {
            (
                document.note_id,
                document.revision,
                document.is_favorite,
                document.favorite_mutation_in_flight,
            )
        })
    else {
        return Vec::new();
    };
    if in_flight || current == is_favorite {
        if !in_flight && let Some(document) = model.document_for_session_mut(session) {
            document.pending_favorite = None;
        }
        return Vec::new();
    }
    let effects = update_action(
        model,
        ActionMsg::SetNoteFavorite {
            note_id,
            revision,
            is_favorite,
        },
    );
    if !effects.is_empty()
        && let Some(document) = model.document_for_session_mut(session)
        && document.note_id == note_id
    {
        document.favorite_mutation_in_flight = true;
    }
    effects
}

fn reload_trash(model: &mut AppModel) -> Option<Effect> {
    let request_id = model.next_request_id();
    model
        .trash
        .begin_reload(request_id)
        .then_some(Effect::LoadTrash { request_id })
}

fn complete_file_import(
    model: &mut AppModel,
    target: super::ImportTarget,
    result: Result<Vec<super::StoredMedia>, UiError>,
) -> Vec<Effect> {
    let Some(document) = model.document_for_session_mut(target.session) else {
        return Vec::new();
    };
    let files = match result {
        Ok(files) => files,
        Err(error) => {
            model.set_notice(error);
            return Vec::new();
        }
    };
    if files.is_empty() {
        return Vec::new();
    }
    let markup = files
        .iter()
        .map(|file| {
            let label = carve_link_label(&file.label);
            format!(
                "{}[{label}]({})",
                if file.image { "!" } else { "" },
                file.path
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let source = insert_media_markup(&document.source, &markup, target.source);
    document.source_changed(source);
    let source = document.source.clone();
    // The import can finish after the user leaves the tab; schedule the save on the document that
    // owns the import, and only preview it while it is active.
    let mut effects = Vec::new();
    if model
        .editor
        .as_ref()
        .is_some_and(|document| document.session == target.session)
    {
        effects.extend(schedule_preview(model));
    }
    effects.extend(schedule_editor_save_for(model, target.session));
    effects.push(Effect::ReloadRichEditor {
        session: target.session,
        source,
    });
    effects
}
