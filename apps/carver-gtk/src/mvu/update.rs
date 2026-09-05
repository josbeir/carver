//! Pure state transitions for the application model.

use super::model::{LibraryRevisionCheckReason, LibraryRevisionRequest};
use super::{
    ActionKey, ActionMsg, AppModel, AppMsg, BrowserMsg, EditorMsg, EditorSaveRequest, Effect,
    LibraryReply, MoveUndo, NavigationMsg, PreferencesMsg, SidebarMsg, TrashMsg, UiError,
    WindowMsg,
};

/// Applies one message and returns the work a runtime must perform afterwards.
#[must_use]
pub fn update(model: &mut AppModel, message: AppMsg) -> Vec<Effect> {
    match message {
        AppMsg::Navigation(NavigationMsg::Started) => {
            vec![Effect::EnsureDefaultCategory]
        }
        AppMsg::Navigation(NavigationMsg::SelectCategory(category_id)) => {
            model.route = super::Route::Browser;
            model.selected_category = category_id;
            reload_browser(model).into_iter().collect()
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
    }
}

fn request_editor_load(
    model: &mut AppModel,
    note_id: carver_sdk::NoteId,
    export_after_load: bool,
) -> Vec<Effect> {
    let request_id = model.next_request_id();
    model.editor_load_request = Some(request_id);
    model.editor_export_after_load = export_after_load.then_some(request_id);
    vec![Effect::LoadEditorNote {
        request_id,
        note_id,
    }]
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
    }
    persist_config_effect(model)
}

fn update_editor(model: &mut AppModel, message: EditorMsg) -> Vec<Effect> {
    match message {
        EditorMsg::Load {
            note_id,
            revision,
            source,
        } => {
            model.route = super::Route::Editor;
            if model
                .editor
                .as_ref()
                .is_some_and(|document| document.note_id == note_id)
            {
                return Vec::new();
            }
            open_editor(model, note_id, revision, source);
            Vec::new()
        }
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
        EditorMsg::BackRequested => request_editor_close(model),
        EditorMsg::TrashRequested => model
            .editor
            .as_ref()
            .map(|document| document.note_id)
            .map_or_else(Vec::new, |note_id| {
                update_action(model, ActionMsg::TrashNote(note_id))
            }),
        EditorMsg::CopyRequested => request_editor_copy(model),
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
        EditorMsg::PasteImage { extension, bytes } => {
            store_editor_asset_effect(model, extension, bytes, String::from("Pasted image"), None)
        }
        EditorMsg::ImportImage {
            extension,
            bytes,
            alt,
            source_selection,
        } => store_editor_asset_effect(model, extension, bytes, alt, source_selection),
        EditorMsg::Close(session_id) => close_editor(model, session_id),
        EditorMsg::PreviewElapsed { .. } => Vec::new(),
        EditorMsg::ThemeChanged => {
            model.editor_theme_revision = model.editor_theme_revision.wrapping_add(1);
            Vec::new()
        }
    }
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
    model.route = super::Route::Browser;
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
    source: String,
) {
    let session = model.next_editor_session_id();
    model.editor = Some(super::EditorDocument::new(
        session,
        note_id,
        revision,
        source.clone(),
        model.preferences.editor_mode,
    ));
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
    model.editor_copy_request = None;
    let message = if omitted_images == 0 {
        String::from("Note copied")
    } else {
        format!("Note copied; {omitted_images} images were omitted.")
    };
    model.notice = Some(UiError::new(message));
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
    model.editor_copy_request = Some(super::EditorCopyRequest {
        request_id: model.next_editor_copy_request_id(),
        session,
        source,
    });
    Vec::new()
}

fn fail_copy_request(model: &mut AppModel, request_id: u64) -> Vec<Effect> {
    let Some(request) = model.editor_copy_request.as_ref() else {
        return Vec::new();
    };
    if request.request_id != request_id {
        return Vec::new();
    }
    model.editor_copy_request = None;
    model.notice = Some(UiError::new("Could not copy the note."));
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
    model.editor_export_dialog_request = Some(request);
    Vec::new()
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
        .take()
        .filter(|request| request.request_id == request_id)
    else {
        return Vec::new();
    };
    if matches!(format, super::EditorExportFormat::Pdf) {
        model.editor_pdf_export_request = Some(super::EditorPdfExportRequest {
            request_id,
            session: request.session,
            source: request.source,
            target_uri,
            print_dialog: false,
        });
        return Vec::new();
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
    model.editor_pdf_export_request = Some(super::EditorPdfExportRequest {
        request_id: model.next_editor_export_request_id(),
        session,
        source,
        target_uri: String::new(),
        print_dialog: true,
    });
    Vec::new()
}

fn update_action(model: &mut AppModel, action: ActionMsg) -> Vec<Effect> {
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
        LibraryReply::BrowserLoaded { request_id, result } => {
            update_browser_loaded(model, request_id, result)
        }
        LibraryReply::EditorLoaded { request_id, result } => {
            update_editor_loaded(model, request_id, result)
        }
        LibraryReply::EditorAssetStored {
            session,
            alt,
            source_selection,
            result,
        } => {
            let Some(document) = model
                .editor
                .as_mut()
                .filter(|document| document.session == session)
            else {
                return Vec::new();
            };
            match result {
                Ok(path) => {
                    let source = image_source(&document.source, &alt, &path, source_selection);
                    if document.source_changed(source) {
                        [schedule_preview(model), schedule_editor_save(model)]
                            .into_iter()
                            .flatten()
                            .collect()
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

fn update_browser_loaded(
    model: &mut AppModel,
    request_id: super::RequestId,
    result: Result<Vec<carver_sdk::NoteSummary>, UiError>,
) -> Vec<Effect> {
    let is_current = matches!(
        model.browser.notes.state,
        super::LoadState::Loading(current) if current == request_id
    );
    let reload = model.browser.notes.finish(request_id, result);
    if is_current {
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
            model.selected_category = Some(note.category_id);
            open_editor(model, note.id, note.revision, note.source);
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
            model.editor_export_warning_request = Some(super::EditorExportWarningRequest {
                request_id,
                session,
                warnings,
            });
            Vec::new()
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
            open_editor(model, note.id, note.revision, note.source);
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
            let mut effects = [reload_sidebar(model), reload_browser(model)]
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
    source_selection: Option<std::ops::Range<usize>>,
) -> Vec<Effect> {
    model
        .editor
        .as_ref()
        .map(|document| Effect::StoreEditorAsset {
            session: document.session,
            note_id: document.note_id,
            extension,
            bytes,
            alt,
            source_selection,
        })
        .into_iter()
        .collect()
}

fn image_source(
    source: &str,
    alt: &str,
    path: &str,
    source_selection: Option<std::ops::Range<usize>>,
) -> String {
    let markup = format!("![{alt}]({path})");
    let Some(selection) = source_selection else {
        let mut source = source.to_owned();
        if !source.is_empty() && !source.ends_with('\n') {
            source.push('\n');
        }
        source.push_str(&markup);
        source.push('\n');
        return source;
    };
    let start = character_byte_offset(source, selection.start);
    let end = character_byte_offset(source, selection.end.max(selection.start));
    let mut inserted = source.to_owned();
    inserted.replace_range(start..end, &markup);
    inserted
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
    let close_after_save = {
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
                    document.closes_after_save()
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
    if close_after_save {
        model.route = super::Route::Browser;
        model.editor = None;
        model.editor_preview = None;
        model.preview_timer = None;
    }
    let mut effects = reload_browser(model).into_iter().collect::<Vec<_>>();
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
    document.request_close();
    if matches!(&document.save_state, super::EditorSaveState::Clean) {
        model.route = super::Route::Browser;
        model.editor = None;
        model.editor_preview = None;
        model.preview_timer = None;
        return Vec::new();
    }
    document
        .begin_save()
        .map_or_else(Vec::new, save_note_effect)
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
        | ActionKey::TrashNote(_) => {}
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
    [
        reload_sidebar(model),
        reload_browser(model),
        reload_trash(model),
    ]
    .into_iter()
    .flatten()
    .collect()
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
    match result {
        Ok(revision) => {
            let changed = model
                .library_revision
                .is_some_and(|current| current != revision);
            model.library_revision = Some(revision);
            if request.reason == LibraryRevisionCheckReason::ExternalWakeup && changed {
                reload_all_resources(model)
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

fn reload_sidebar(model: &mut AppModel) -> Option<Effect> {
    let request_id = model.next_request_id();
    model
        .sidebar
        .begin_reload(request_id)
        .then_some(Effect::LoadSidebar { request_id })
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

fn reload_trash(model: &mut AppModel) -> Option<Effect> {
    let request_id = model.next_request_id();
    model
        .trash
        .begin_reload(request_id)
        .then_some(Effect::LoadTrash { request_id })
}
