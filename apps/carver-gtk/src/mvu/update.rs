//! Pure state transitions for the application model.

use super::model::{
    ExternalChange, LibraryRevisionCheckReason, LibraryRevisionRequest, PendingNavigation,
};
use super::{
    ActionKey, ActionMsg, AppModel, AppMsg, BasesMsg, BrowserMsg, EditorMsg, EditorSaveRequest,
    EditorSessionId, Effect, LibraryReply, MoveUndo, NavigationMsg, PreferencesMsg, SidebarMsg,
    SourceEdit, TrashMsg, UiError, WindowMsg,
};

/// Applies one message and returns the work a runtime must perform afterwards.
#[must_use]
pub fn update(model: &mut AppModel, message: AppMsg) -> Vec<Effect> {
    let mut effects = match message {
        AppMsg::Navigation(NavigationMsg::Started) => {
            vec![Effect::EnsureDefaultCategory]
        }
        AppMsg::Navigation(NavigationMsg::SelectCategory(category_id)) => {
            select_category(model, category_id)
        }
        AppMsg::Navigation(NavigationMsg::OpenNote(note_id)) => {
            request_editor_load(model, note_id, false)
        }
        AppMsg::Navigation(NavigationMsg::ExportNote(note_id)) => {
            request_editor_load(model, note_id, true)
        }
        AppMsg::Navigation(NavigationMsg::CreateNote) => create_note_effect(model),
        AppMsg::Navigation(NavigationMsg::ImportNote { format, source }) => {
            import_note_effect(model, format, source)
        }
        AppMsg::Navigation(NavigationMsg::ImportFailed(message)) => {
            model.notice = Some(UiError::new(message));
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

fn update_bases(model: &mut AppModel, message: BasesMsg) -> Vec<Effect> {
    match message {
        BasesMsg::Open(base_id) => {
            if model.route == super::Route::Editor {
                model.pending_navigation = Some(PendingNavigation::Base(base_id));
                let effects = request_editor_close(model);
                return if model.editor.is_none() {
                    complete_pending_navigation(model)
                } else {
                    effects
                };
            }
            open_base(model, base_id)
        }
        BasesMsg::Create { name, columns } => {
            let name = name.trim().to_owned();
            if name.is_empty() {
                model.notice = Some(UiError::new("Base names cannot be empty."));
                Vec::new()
            } else {
                vec![Effect::CreateBase { name, columns }]
            }
        }
        BasesMsg::Reload => reload_bases(model).into_iter().collect(),
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

fn request_editor_load(
    model: &mut AppModel,
    note_id: carver_sdk::NoteId,
    export_after_load: bool,
) -> Vec<Effect> {
    if model.route != super::Route::Editor {
        model.editor_return_route = model.route;
    }
    let request_id = model.next_request_id();
    model.editor_load_request = Some(request_id);
    model.editor_export_after_load = export_after_load.then_some(request_id);
    vec![Effect::LoadEditorNote {
        request_id,
        note_id,
    }]
}

fn select_category(
    model: &mut AppModel,
    category_id: Option<carver_sdk::CategoryId>,
) -> Vec<Effect> {
    if model.route == super::Route::Editor {
        model.pending_navigation = Some(PendingNavigation::Browser(category_id));
        let effects = request_editor_close(model);
        return if model.editor.is_none() {
            complete_pending_navigation(model)
        } else {
            effects
        };
    }
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
    model.route = super::Route::Base;
    model.bases.selected = Some(base_id);
    reload_base_rows(model, base_id).into_iter().collect()
}

fn update_browser(model: &mut AppModel, message: BrowserMsg) -> Vec<Effect> {
    match message {
        BrowserMsg::Reload => reload_browser(model).into_iter().collect(),
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
                let _ = close_editor(model, session);
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
                model.notice = None;
                return schedule_preview(model).into_iter().collect();
            }
            Vec::new()
        }
        EditorMsg::ApplySourceCommand { command, selection } => {
            update_source_command(model, command, selection)
        }
        EditorMsg::ApplyRichCommand(command) => update_rich_command(model, command),
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
            .editor
            .as_mut()
            .filter(|document| document.session == session && document.is_current_timer(timer_id))
            .and_then(super::EditorDocument::begin_save)
            .map_or_else(Vec::new, save_note_effect),
        EditorMsg::RetrySave => model
            .editor
            .as_mut()
            .and_then(super::EditorDocument::begin_save)
            .map_or_else(Vec::new, save_note_effect),
        EditorMsg::BackRequested => {
            model.pending_navigation = None;
            request_editor_close(model)
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
                    model.notice = Some(error);
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
                model.notice = Some(UiError::new(
                    "Could not open this file. Install an application that can view it.",
                ));
            }
            Vec::new()
        }
        EditorMsg::MediaFileLoaded {
            image,
            session,
            path,
            file,
        } => {
            if let Some(document) = model.editor.as_mut()
                && document.session == session
                && document.media_file_kinds.get(&path) == Some(&image)
            {
                document.media_files.insert(path, file);
            }
            Vec::new()
        }
        EditorMsg::ToggleDocumentSidebar => {
            if let Some(document) = model.editor.as_mut() {
                document.document_sidebar = document.document_sidebar.toggled();
                model.config.editor.show_document_sidebar = document.document_sidebar.is_visible();
                return persist_config_effect(model);
            }
            Vec::new()
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
        EditorMsg::Close(session_id) => close_editor(model, session_id),
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
    model.route = super::Route::Editor;
    if model
        .editor
        .as_ref()
        .is_some_and(|document| document.note_id == note_id)
    {
        return Vec::new();
    }
    open_editor(model, note_id, revision, false, source);
    Vec::new()
}

fn update_source_command(
    model: &mut AppModel,
    command: super::SourceCommand,
    selection: std::ops::Range<usize>,
) -> Vec<Effect> {
    let Some(document) = model.editor.as_mut() else {
        return Vec::new();
    };
    let session = document.session;
    let edit = SourceEdit::apply(document.source.clone(), selection, command);
    if !document.source_changed(edit.source().to_owned()) {
        return Vec::new();
    }
    model.notice = None;
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

fn close_editor(model: &mut AppModel, session_id: super::EditorSessionId) -> Vec<Effect> {
    if model
        .editor
        .as_ref()
        .is_none_or(|document| document.session != session_id)
    {
        return Vec::new();
    }
    restore_editor_origin(model);
    model.editor = None;
    model.editor_preview = None;
    model.editor_copy_request = None;
    model.editor_export_dialog_request = None;
    model.editor_export_warning_request = None;
    model.editor_export_progress = None;
    model.editor_pdf_export_request = None;
    model.preview_timer = None;
    model.editor_load_request = None;
    model.editor_export_after_load = None;
    Vec::new()
}

fn schedule_preview(model: &mut AppModel) -> Option<Effect> {
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
    let category_id = model
        .selected_category
        .or_else(|| match &model.sidebar.state {
            super::LoadState::Ready(categories) => {
                categories.first().map(|category| category.category.id)
            }
            _ => None,
        });
    category_id.map_or_else(
        || {
            model.notice = Some(UiError::new("No category is available for the new note."));
            Vec::new()
        },
        |category_id| vec![Effect::CreateNote { category_id }],
    )
}

fn import_note_effect(
    model: &mut AppModel,
    format: carver_sdk::DocumentImportFormat,
    source: String,
) -> Vec<Effect> {
    let category_id = model
        .selected_category
        .or_else(|| match &model.sidebar.state {
            super::LoadState::Ready(categories) => {
                categories.first().map(|category| category.category.id)
            }
            _ => None,
        });
    category_id.map_or_else(
        || {
            model.notice = Some(UiError::new(
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

fn open_editor(
    model: &mut AppModel,
    note_id: carver_sdk::NoteId,
    revision: carver_sdk::Revision,
    is_favorite: bool,
    source: String,
) {
    let session = model.next_editor_session_id();
    let mut document = super::EditorDocument::new(
        session,
        note_id,
        revision,
        is_favorite,
        source.clone(),
        model.preferences.editor_mode,
    );
    if model.config.editor.show_document_sidebar {
        document.document_sidebar = super::model::DocumentSidebarVisibility::Visible;
    }
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
    model.preview_timer = None;
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
            model.notice = Some(UiError::new("Note copied"));
        }
        (super::EditorCopyScope::Note, omitted) => {
            model.notice = Some(UiError::new(format!(
                "Note copied; {omitted} images were omitted."
            )));
        }
        (super::EditorCopyScope::Selection, 0) => {}
        (super::EditorCopyScope::Selection, omitted) => {
            model.notice = Some(UiError::new(format!(
                "Selection copied; {omitted} images were omitted."
            )));
        }
    }
    Vec::new()
}

fn request_editor_copy(model: &mut AppModel) -> Vec<Effect> {
    let Some((session, source)) = model
        .editor
        .as_ref()
        .map(|document| (document.session, document.source.clone()))
    else {
        return Vec::new();
    };
    let request = super::EditorCopyRequest {
        request_id: model.next_editor_copy_request_id(),
        session,
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
    if !matches!(model.editor.as_ref(), Some(document) if document.session == session) {
        return Vec::new();
    }
    let request = super::EditorCopyRequest {
        request_id: model.next_editor_copy_request_id(),
        session,
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
    model.notice = Some(UiError::new(message));
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
            request_id,
            session: request.session,
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
    model.notice = Some(UiError::new("Note exported as PDF"));
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
    model.notice = Some(UiError::new("Could not export the note as PDF."));
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
    let Some((session, source)) = model
        .editor
        .as_ref()
        .map(|document| (document.session, document.source.clone()))
    else {
        return Vec::new();
    };
    let pdf_request = super::EditorPdfExportRequest {
        request_id: model.next_editor_export_request_id(),
        session,
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
        && model
            .editor
            .as_ref()
            .is_some_and(|document| document.note_id == note_id)
    {
        model.route = super::Route::Browser;
        model.editor = None;
        model.editor_preview = None;
        model.editor_copy_request = None;
        model.editor_export_dialog_request = None;
        model.editor_export_warning_request = None;
        model.editor_export_progress = None;
        model.editor_pdf_export_request = None;
        model.preview_timer = None;
        model.editor_export_after_load = None;
    }
    let effect = match action {
        ActionMsg::CreateCategory(name) => {
            category_name_effect(&name, |name| Effect::CreateCategory { name })
        }
        ActionMsg::CreateCategoryWithAppearance { name, appearance } => {
            category_name_effect(&name, |name| Effect::CreateCategoryWithAppearance {
                name,
                appearance,
            })
        }
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
        model.notice = Some(UiError::new("Category names cannot be empty."));
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

fn update_library(model: &mut AppModel, reply: LibraryReply) -> Vec<Effect> {
    match reply {
        LibraryReply::BasesLoaded { request_id, result } => {
            update_bases_loaded(model, request_id, result)
        }
        LibraryReply::BaseRowsLoaded {
            request_id,
            base_id,
            result,
        } => update_base_rows_loaded(model, request_id, base_id, result),
        LibraryReply::BaseCreated { result } => update_base_created(model, result),
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
                    model.notice = None;
                    let mut effects = reload_after_local_mutation(model);
                    if matches!(action, ActionKey::TrashCategory(_)) {
                        effects.push(Effect::EnsureDefaultCategory);
                    }
                    effects
                }
                Err(error) => {
                    model.notice = Some(error);
                    Vec::new()
                }
            }
        }
        LibraryReply::SidebarLoaded { request_id, result } => {
            reload_sidebar_after(model.sidebar.finish(request_id, result), model)
        }
        LibraryReply::BrowserLoaded {
            request_id,
            result,
            favorites,
        } => update_browser_loaded(model, request_id, result, favorites),
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
        LibraryReply::EditorLoaded { request_id, result } => {
            update_editor_loaded(model, request_id, result)
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
                model.notice = None;
                reload_after_local_mutation(model)
            }
            Err(error) => {
                model.notice = Some(error);
                Vec::new()
            }
        },
        LibraryReply::EditorSaved { request, result } => {
            update_editor_save(model, &request, result)
        }
    }
}

fn update_bases_loaded(
    model: &mut AppModel,
    request_id: super::RequestId,
    result: Result<Vec<carver_sdk::BaseDefinition>, UiError>,
) -> Vec<Effect> {
    let reload = model.bases.definitions.finish(request_id, result);
    reload
        .then(|| reload_bases(model))
        .flatten()
        .into_iter()
        .collect()
}

fn update_base_rows_loaded(
    model: &mut AppModel,
    request_id: super::RequestId,
    _base_id: carver_sdk::BaseId,
    result: Result<Vec<carver_sdk::BaseRow>, UiError>,
) -> Vec<Effect> {
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

fn update_base_created(
    model: &mut AppModel,
    result: Result<carver_sdk::BaseDefinition, UiError>,
) -> Vec<Effect> {
    match result {
        Ok(base) => {
            model.notice = None;
            model.route = super::Route::Base;
            model.bases.selected = Some(base.id);
            [reload_bases(model), reload_base_rows(model, base.id)]
                .into_iter()
                .flatten()
                .collect()
        }
        Err(error) => {
            model.notice = Some(error);
            Vec::new()
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
            model.notice = None;
            let (rebased_save, pending_favorite, close_requested) = if let Some(document) = model
                .editor
                .as_mut()
                .filter(|document| document.note_id == note.id)
            {
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
                )
            } else {
                (None, None, false)
            };
            let save_was_rebased = rebased_save.is_some();
            let mut effects = rebased_save.map_or_else(Vec::new, save_note_effect);
            if !save_was_rebased {
                effects.extend(pending_favorite.map_or_else(Vec::new, |is_favorite| {
                    set_editor_favorite(model, is_favorite)
                }));
                if pending_favorite.is_none() && close_requested {
                    let session = model.editor.as_ref().map(|document| document.session);
                    effects.extend(
                        session.map_or_else(Vec::new, |session| close_editor(model, session)),
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
                && let Some(document) = model
                    .editor
                    .as_mut()
                    .filter(|document| document.note_id == note_id)
            {
                document.favorite_mutation_in_flight = false;
                document.pending_favorite = None;
            }
            model.notice = Some(error);
            Vec::new()
        }
    }
}

fn update_editor_asset_stored(
    model: &mut AppModel,
    session: super::EditorSessionId,
    alt: &str,
    source_target: Option<super::SourceImageTarget>,
    result: Result<String, UiError>,
    image: bool,
) -> Vec<Effect> {
    let Some(document) = model
        .editor
        .as_mut()
        .filter(|document| document.session == session)
    else {
        return Vec::new();
    };
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
            [
                schedule_preview(model),
                schedule_editor_save(model),
                Some(Effect::ReloadRichEditor { session, source }),
            ]
            .into_iter()
            .flatten()
            .collect()
        }
        Err(error) => {
            model.notice = Some(error);
            Vec::new()
        }
    }
}

fn update_browser_loaded(
    model: &mut AppModel,
    request_id: super::RequestId,
    result: Result<Vec<carver_sdk::NoteSummary>, UiError>,
    favorites: Result<Vec<carver_sdk::NoteSummary>, UiError>,
) -> Vec<Effect> {
    let is_current = matches!(
        model.browser.notes.state,
        super::LoadState::Loading(current) if current == request_id
    );
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

fn update_editor_loaded(
    model: &mut AppModel,
    request_id: super::RequestId,
    result: Result<carver_sdk::Note, UiError>,
) -> Vec<Effect> {
    if model.editor_load_request != Some(request_id) {
        return Vec::new();
    }
    model.editor_load_request = None;
    let export_after_load = model.editor_export_after_load == Some(request_id);
    if export_after_load {
        model.editor_export_after_load = None;
    }
    match result {
        Ok(note) => {
            open_editor(model, note.id, note.revision, note.is_favorite, note.source);
            model.route = super::Route::Editor;
            if export_after_load {
                request_editor_export_dialog(model)
            } else {
                Vec::new()
            }
        }
        Err(error) => {
            model.notice = Some(error);
            Vec::new()
        }
    }
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
            model.notice = Some(error);
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
        Ok(()) => model.notice = Some(UiError::new("Note exported")),
        Err(error) => model.notice = Some(error),
    }
    Vec::new()
}

fn update_created_note(
    model: &mut AppModel,
    result: Result<carver_sdk::Note, UiError>,
) -> Vec<Effect> {
    match result {
        Ok(note) => {
            open_editor(model, note.id, note.revision, note.is_favorite, note.source);
            model.route = super::Route::Editor;
            reload_after_local_mutation(model)
        }
        Err(error) => {
            model.notice = Some(error);
            Vec::new()
        }
    }
}

fn update_config_persisted(model: &mut AppModel, result: Result<(), UiError>) -> Vec<Effect> {
    if let Err(error) = result {
        model.notice = Some(error);
    }
    Vec::new()
}

fn update_default_category(model: &mut AppModel, result: Result<(), UiError>) -> Vec<Effect> {
    match result {
        Ok(()) => {
            let mut effects = [
                reload_sidebar(model),
                reload_bases(model),
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
            model.notice = Some(error);
            Vec::new()
        }
    }
}

fn store_editor_asset_effect(
    model: &AppModel,
    extension: String,
    bytes: Vec<u8>,
    alt: String,
    source_target: Option<super::SourceImageTarget>,
    image: bool,
) -> Vec<Effect> {
    model
        .editor
        .as_ref()
        .filter(|document| document.mode != carver_config::EditorMode::Rendered)
        .map(|document| Effect::StoreEditorAsset {
            image,
            session: document.session,
            note_id: document.note_id,
            extension,
            bytes,
            alt,
            source_target,
        })
        .into_iter()
        .collect()
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

fn schedule_editor_save(model: &mut AppModel) -> Option<Effect> {
    let timer_id = model.next_timer_id();
    let delay_ms = model.preferences.autosave_delay_ms;
    let document = model.editor.as_mut()?;
    if matches!(document.save_state, super::EditorSaveState::Saving(_)) {
        return None;
    }
    document.schedule_save(timer_id);
    Some(Effect::ScheduleEditorSave {
        session: document.session,
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
    result: Result<carver_sdk::Revision, UiError>,
) -> Vec<Effect> {
    let (close_requested, pending_favorite) = {
        let Some(document) = model.editor.as_mut() else {
            return Vec::new();
        };
        if document.session != request.session
            || document.note_id != request.note_id
            || document.revision != request.expected_revision
            || document.save_state != super::EditorSaveState::Saving(request.clone())
        {
            return Vec::new();
        }
        match result {
            Ok(revision) => {
                document.revision = revision;
                if document.source == request.source {
                    document.save_state = super::EditorSaveState::Clean;
                    (
                        document.close_is_requested(),
                        (!document.favorite_mutation_in_flight)
                            .then_some(document.pending_favorite)
                            .flatten(),
                    )
                } else {
                    document.save_state = super::EditorSaveState::Dirty;
                    return document
                        .begin_save()
                        .map_or_else(Vec::new, save_note_effect);
                }
            }
            Err(error) if document.source == request.source => {
                document.save_state = super::EditorSaveState::Failed(error);
                return Vec::new();
            }
            Err(_) => {
                document.save_state = super::EditorSaveState::Dirty;
                return document
                    .begin_save()
                    .map_or_else(Vec::new, save_note_effect);
            }
        }
    };
    let mut effects = pending_favorite.map_or_else(Vec::new, |is_favorite| {
        set_editor_favorite(model, is_favorite)
    });
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
    effects.extend(request_library_revision(
        model,
        LibraryRevisionCheckReason::LocalMutation,
    ));
    effects
}

fn request_editor_close(model: &mut AppModel) -> Vec<Effect> {
    let Some(document) = model.editor.as_mut() else {
        return Vec::new();
    };
    if document.external_change.is_some() {
        return vec![Effect::ShowExternalEdit {
            session: document.session,
            deleted: document.external_change == Some(ExternalChange::Deleted),
        }];
    }
    document.request_close();
    if matches!(&document.save_state, super::EditorSaveState::Clean)
        && !document.favorite_mutation_in_flight
    {
        restore_editor_origin(model);
        model.editor = None;
        model.editor_preview = None;
        model.preview_timer = None;
        return if model.pending_navigation.is_none() {
            reload_return_surface(model)
        } else {
            Vec::new()
        };
    }
    document
        .begin_save()
        .map_or_else(Vec::new, save_note_effect)
}

fn reload_return_surface(model: &mut AppModel) -> Vec<Effect> {
    match (model.route, model.bases.selected) {
        (super::Route::Base, Some(base_id)) => {
            reload_base_rows(model, base_id).into_iter().collect()
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

fn request_library_revision(
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
            if changed || retry {
                let mut effects =
                    if changed && request.reason == LibraryRevisionCheckReason::ExternalWakeup {
                        reload_all_resources(model)
                    } else {
                        Vec::new()
                    };
                effects.extend(refresh_open_editor(model, false));
                effects
            } else {
                Vec::new()
            }
        }
        Err(error) => {
            model.notice = Some(error);
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
            model.notice = Some(error);
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

fn reload_sidebar(model: &mut AppModel) -> Option<Effect> {
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

fn reload_base_rows(model: &mut AppModel, base_id: carver_sdk::BaseId) -> Option<Effect> {
    let request_id = model.next_request_id();
    model
        .bases
        .rows
        .begin_reload(request_id)
        .then_some(Effect::LoadBaseRows {
            request_id,
            base_id,
        })
}

fn reload_browser(model: &mut AppModel) -> Option<Effect> {
    let request_id = model.next_request_id();
    let started = model.browser.notes.begin_reload(request_id);
    if started {
        model.browser.loading_indicator_request = Some(request_id);
        model.browser.loading_indicator_visible = false;
    }
    started.then_some(Effect::LoadBrowser {
        request_id,
        category_id: model.selected_category,
        query: model.browser.search_query.clone(),
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
    let Some((note_id, revision)) = model.editor.as_ref().and_then(|document| {
        (!document.favorite_mutation_in_flight && document.is_favorite != is_favorite)
            .then_some((document.note_id, document.revision))
    }) else {
        if let Some(document) = model.editor.as_mut()
            && !document.favorite_mutation_in_flight
        {
            document.pending_favorite = None;
        }
        return Vec::new();
    };
    let effects = update_action(
        model,
        ActionMsg::SetNoteFavorite {
            note_id,
            revision,
            is_favorite,
        },
    );
    if !effects.is_empty()
        && let Some(document) = model
            .editor
            .as_mut()
            .filter(|document| document.note_id == note_id)
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
    let Some(document) = model
        .editor
        .as_mut()
        .filter(|document| document.session == target.session)
    else {
        return Vec::new();
    };
    let files = match result {
        Ok(files) => files,
        Err(error) => {
            model.notice = Some(error);
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
    [
        schedule_preview(model),
        schedule_editor_save(model),
        Some(Effect::ReloadRichEditor {
            session: target.session,
            source,
        }),
    ]
    .into_iter()
    .flatten()
    .collect()
}
