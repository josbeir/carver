//! Imperative GTK rendering adapters for MVU model snapshots.

#[cfg(test)]
mod tests;

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk::prelude::*;
use libadwaita as adw;
use time::{Date, OffsetDateTime};

use crate::mvu::{
    AppDispatcher, AppModel, AppMsg, BrowserModel, EditorSaveState, Effect, LoadState, MoveUndo,
    Route,
};
use crate::ui::browser::{BrowserFeedContext, BrowserFeedItem};

type SidebarRenderer = Box<dyn Fn(&AppModel)>;
#[derive(Clone, Debug, Eq, PartialEq)]
struct SidebarSnapshot {
    categories: Vec<carver_sdk::CategorySummary>,
    bases: LoadState<Vec<carver_sdk::BaseDefinition>>,
    selected_category: Option<carver_sdk::CategoryId>,
    selection: crate::mvu::SidebarSelection,
}

impl SidebarSnapshot {
    fn from_model(model: &AppModel) -> Option<Self> {
        let LoadState::Ready(categories) = &model.sidebar.state else {
            return None;
        };
        Some(Self {
            categories: categories.clone(),
            bases: model.bases.definitions.state.clone(),
            selected_category: model.selected_category,
            selection: model.sidebar_selection(),
        })
    }
}
struct BrowserProjectionSnapshot {
    browser: BrowserModel,
    selected_category: Option<carver_sdk::CategoryId>,
    sidebar: LoadState<Vec<carver_sdk::CategorySummary>>,
    route: Route,
    today: Date,
}

impl BrowserProjectionSnapshot {
    fn matches(&self, model: &AppModel, today: Date) -> bool {
        self.browser == model.browser
            && self.selected_category == model.selected_category
            && self.sidebar == model.sidebar.state
            && self.route == model.route
            && self.today == today
    }
}

struct BrowserContentRefs<'a> {
    list: &'a gtk::ListView,
    feed_store: &'a gtk::gio::ListStore,
    pages: &'a gtk::Stack,
    empty_new_note: &'a gtk::Button,
}

/// GTK references used to render the high-level MVU resources.
///
/// This type intentionally owns widgets only. Application state lives in [`AppModel`].
pub struct ViewRefs {
    route_stack: gtk::Stack,
    browser_list: Option<gtk::ListView>,
    browser_feed_store: Option<gtk::gio::ListStore>,
    browser_feed_context: Option<Rc<RefCell<BrowserFeedContext>>>,
    browser_rendered_rows: RefCell<Vec<(carver_sdk::NoteId, carver_sdk::Revision)>>,
    browser_rendered_context: RefCell<Option<BrowserFeedContext>>,
    browser_pages: Option<gtk::Stack>,
    browser_search_bar: Option<gtk::SearchBar>,
    browser_search_entry: Option<gtk::SearchEntry>,
    browser_search_toggle: Option<gtk::ToggleButton>,
    browser_empty_new_note_button: Option<gtk::Button>,
    browser_status: adw::StatusPage,
    base: Option<crate::ui::bases::BaseViewRefs>,
    trash_list: Option<gtk::ListBox>,
    trash_pages: Option<gtk::Stack>,
    empty_trash_button: Option<gtk::Button>,
    trash_status: adw::StatusPage,
    toast_overlay: Option<adw::ToastOverlay>,
    dispatcher: Option<AppDispatcher>,
    last_notice: RefCell<Option<String>>,
    external_change_toast: RefCell<Option<(crate::mvu::EditorSessionId, adw::Toast)>>,
    last_editor_save_error: RefCell<Option<String>>,
    last_undo_move: RefCell<Option<MoveUndo>>,
    last_undo_trash_note: Cell<Option<carver_sdk::NoteId>>,
    last_browser_search_open: Cell<bool>,
    last_browser_snapshot: RefCell<Option<BrowserProjectionSnapshot>>,
    last_trash_snapshot: RefCell<Option<LoadState<carver_sdk::TrashContents>>>,
    sidebar_renderer: Option<SidebarRenderer>,
    editor: Option<crate::ui::editor::EditorViewRefs>,
    last_sidebar_snapshot: RefCell<Option<SidebarSnapshot>>,
    rendering: Cell<bool>,
}

impl ViewRefs {
    /// Collects view references after a window has composed its GTK widget tree.
    #[must_use]
    pub fn new(
        route_stack: gtk::Stack,
        browser_status: adw::StatusPage,
        trash_status: adw::StatusPage,
    ) -> Self {
        Self {
            route_stack,
            browser_list: None,
            browser_feed_store: None,
            browser_feed_context: None,
            browser_rendered_rows: RefCell::new(Vec::new()),
            browser_rendered_context: RefCell::new(None),
            browser_pages: None,
            browser_search_bar: None,
            browser_search_entry: None,
            browser_search_toggle: None,
            browser_empty_new_note_button: None,
            browser_status,
            base: None,
            trash_list: None,
            trash_pages: None,
            empty_trash_button: None,
            trash_status,
            toast_overlay: None,
            dispatcher: None,
            last_notice: RefCell::new(None),
            external_change_toast: RefCell::new(None),
            last_editor_save_error: RefCell::new(None),
            last_undo_move: RefCell::new(None),
            last_undo_trash_note: Cell::new(None),
            last_browser_search_open: Cell::new(false),
            last_browser_snapshot: RefCell::new(None),
            last_trash_snapshot: RefCell::new(None),
            sidebar_renderer: None,
            editor: None,
            last_sidebar_snapshot: RefCell::new(None),
            rendering: Cell::new(false),
        }
    }

    /// Adds the trash widgets created by the window composition shell.
    #[must_use]
    pub fn with_trash(
        mut self,
        trash_list: gtk::ListBox,
        trash_pages: gtk::Stack,
        empty_trash_button: gtk::Button,
    ) -> Self {
        self.trash_list = Some(trash_list);
        self.trash_pages = Some(trash_pages);
        self.empty_trash_button = Some(empty_trash_button);
        self
    }

    /// Adds the window toast overlay used for mutation error feedback.
    #[must_use]
    pub fn with_toast_overlay(mut self, toast_overlay: adw::ToastOverlay) -> Self {
        self.toast_overlay = Some(toast_overlay);
        self
    }

    /// Adds the window-local message dispatcher used by rendered contextual actions.
    #[must_use]
    pub fn with_dispatcher(mut self, dispatcher: AppDispatcher) -> Self {
        self.dispatcher = Some(dispatcher);
        self
    }

    /// Adds the browser widgets created by the window composition.
    #[must_use]
    pub(crate) fn with_browser(mut self, browser: crate::ui::browser::BrowserViewRefs) -> Self {
        self.browser_list = Some(browser.list);
        self.browser_feed_store = Some(browser.feed_store);
        self.browser_feed_context = Some(browser.feed_context);
        self.browser_pages = Some(browser.pages);
        self.browser_search_bar = Some(browser.search_bar);
        self.browser_search_entry = Some(browser.search_entry);
        self.browser_search_toggle = Some(browser.search_toggle);
        self.browser_empty_new_note_button = Some(browser.empty_new_note_button);
        self
    }

    /// Adds the native saved-base grid.
    #[must_use]
    pub(crate) fn with_base(mut self, base: crate::ui::bases::BaseViewRefs) -> Self {
        self.base = Some(base);
        self
    }

    /// Uses the complete category-row renderer for changed MVU snapshots.
    #[must_use]
    pub fn with_sidebar_renderer(mut self, renderer: impl Fn(&AppModel) + 'static) -> Self {
        self.sidebar_renderer = Some(Box::new(renderer));
        self
    }

    /// Adds the editor projections created by the composition shell.
    #[must_use]
    pub(crate) fn with_editor(mut self, editor: crate::ui::editor::EditorViewRefs) -> Self {
        self.editor = Some(editor);
        self
    }

    /// Renders one immutable model snapshot without invoking application actions.
    pub fn render(&self, model: &AppModel) {
        self.rendering.set(true);
        self.route_stack.set_visible_child_name(match model.route {
            Route::Browser => "browser",
            Route::Base => "base",
            Route::Trash => "trash",
            Route::Editor => "editor",
        });
        self.render_sidebar(model);
        self.render_browser(model);
        self.render_base(model);
        self.render_trash(model);
        self.render_editor(model);
        self.clear_resolved_external_notice(model);
        self.render_notice(model);
        self.render_editor_save_error(model);
        self.render_undo_move(model);
        self.render_undo_trash_note(model);
        self.rendering.set(false);
    }

    fn render_base(&self, model: &AppModel) {
        if model.route != Route::Base {
            return;
        }
        let (Some(refs), Some(base_id), Some(dispatcher)) =
            (&self.base, model.bases.selected, &self.dispatcher)
        else {
            return;
        };
        crate::ui::bases::render_base_search(
            refs,
            model.bases.search_open,
            &model.bases.search_query,
        );
        let LoadState::Ready(definitions) = &model.bases.definitions.state else {
            crate::ui::bases::actions::render_delete(&refs.delete, None, dispatcher);
            crate::ui::bases::actions::render_configure(&refs.configure, None, dispatcher);
            refs.grid.set_sensitive(false);
            if let LoadState::Failed(error) = &model.bases.definitions.state {
                crate::ui::bases::render_base_status(refs, "Couldn’t load base", &error.message);
                return;
            }
            if !matches!(model.bases.definitions.state, LoadState::Loading(id)
                if model.bases.definitions_loading_elapsed == Some(id))
            {
                return;
            }
            crate::ui::bases::render_base_status(refs, "Loading base…", "Loading its definition.");
            return;
        };
        let definition = definitions.iter().find(|base| base.id == base_id);
        crate::ui::bases::actions::render_delete(
            &refs.delete,
            definition.filter(|base| !model.bases.deleting.contains(&base.id)),
            dispatcher,
        );
        let rows = match &model.bases.rows.state {
            LoadState::Ready(rows) => rows,
            LoadState::Failed(error) => {
                crate::ui::bases::render_base_status(refs, "Couldn’t load rows", &error.message);
                return;
            }
            LoadState::Idle | LoadState::Loading(_) => {
                refs.grid.set_sensitive(false);
                if !matches!(model.bases.rows.state, LoadState::Loading(id)
                    if model.bases.rows_loading_elapsed == Some(id))
                {
                    return;
                }
                crate::ui::bases::render_base_status(
                    refs,
                    "Loading rows…",
                    "Refreshing this base.",
                );
                return;
            }
        };
        if let Some(definition) = definitions.iter().find(|base| base.id == base_id) {
            crate::ui::bases::actions::render_configure(
                &refs.configure,
                Some(definition),
                dispatcher,
            );
            crate::ui::bases::render_base(refs, definition, rows, dispatcher);
            refs.grid.set_sensitive(!model.bases.saving_configuration);
            let loading = model.bases.rows_append_request.is_some();
            refs.load_more
                .set_visible(model.bases.rows_has_more || model.bases.rows_append_error.is_some());
            refs.load_more.set_sensitive(!loading);
            refs.load_more.set_label(if loading {
                "Loading more rows…"
            } else if model.bases.rows_append_error.is_some() {
                "Retry loading rows"
            } else {
                "Load more rows"
            });
        }
    }

    /// Executes a native GTK adapter effect after the runtime has rendered its model snapshot.
    ///
    /// These effects deliberately live outside `render`: rendering remains a projection of the
    /// model and cannot repeat clipboard, dialog, or print work on a later redraw.
    // CONTEXT: Native dialog effects stay in one visible dispatch table so GTK callbacks never
    // bypass the MVU runtime or apply work to a stale dialog session.
    #[expect(
        clippy::too_many_lines,
        reason = "the native effect boundary keeps dialog session routing explicit"
    )]
    pub(crate) fn run_editor_effect(&self, effect: Effect) {
        if let Effect::ShowNewBaseConfiguration {
            dialog_id,
            descriptors,
        } = &effect
        {
            if let Some(dispatcher) = &self.dispatcher {
                use adw::prelude::*;
                if let Some(parent) = self.route_stack.root().and_downcast::<gtk::Window>() {
                    let dialog = crate::ui::bases::actions::show_new_configuration_dialog(
                        &parent,
                        dispatcher,
                        *dialog_id,
                        descriptors,
                    );
                    if let Some(refs) = &self.base {
                        refs.configuration.replace(Some((*dialog_id, dialog)));
                    }
                }
            }
            return;
        }
        if let Effect::ShowBaseConfiguration {
            dialog_id,
            definition,
            descriptors,
        } = &effect
        {
            if let (Some(refs), Some(dispatcher)) = (&self.base, &self.dispatcher) {
                use adw::prelude::*;
                if let Some(parent) = refs.configure.root().and_downcast::<gtk::Window>() {
                    let existing = refs.configuration.borrow().clone();
                    if let Some((_, dialog)) = existing.filter(|(_, dialog)| dialog.is_mapped()) {
                        dialog.grab_focus();
                        return;
                    }
                    let dialog = crate::ui::bases::actions::show_configuration_dialog(
                        &parent,
                        dispatcher,
                        *dialog_id,
                        definition,
                        descriptors,
                    );
                    refs.configuration.replace(Some((*dialog_id, dialog)));
                }
            }
            return;
        }
        if let Effect::FinishBaseConfiguration { success } = effect {
            if let Some(refs) = &self.base {
                let dialog = if success {
                    refs.configuration.take().map(|(_, dialog)| dialog)
                } else {
                    refs.configuration
                        .borrow()
                        .clone()
                        .map(|(_, dialog)| dialog)
                };
                if let Some(dialog) = dialog {
                    crate::ui::bases::actions::finish_configuration(&dialog, success);
                }
            }
            return;
        }
        if let Effect::UpdateBaseConfigurationPreview { dialog_id, count } = effect {
            if let Some(dialog) = self
                .base
                .as_ref()
                .and_then(|refs| refs.configuration.borrow().clone())
                .filter(|(current_dialog_id, _)| current_dialog_id == &dialog_id)
                .map(|(_, dialog)| dialog)
            {
                crate::ui::bases::actions::render_preview(&dialog, count);
            }
            return;
        }
        if let Effect::ShowExternalEdit { session, deleted } = effect {
            self.show_external_edit(session, deleted);
            return;
        }
        let Some(editor) = &self.editor else {
            return;
        };
        match effect {
            Effect::ApplyRichEditorCommand { command } => editor.apply_rich_command(&command),
            Effect::ReloadRichEditor { session, source } => {
                editor.reload_rich_source(session, &source);
            }
            Effect::InsertRichSource {
                session,
                request_id,
                source,
                structured,
                fallback,
                host_initiated,
            } => {
                editor.insert_rich_source(
                    session,
                    request_id,
                    &source,
                    structured,
                    &fallback,
                    host_initiated,
                );
            }
            Effect::SelectEditorSource { session, selection } => {
                editor.select_source_range(session, selection);
            }
            Effect::FocusEditor { session } => editor.focus(session),
            Effect::FocusDocumentTarget {
                session,
                selection,
                generation,
                target,
            } => {
                editor.focus_document_target(session, generation, selection, &target);
            }
            Effect::ShowMediaPreview { session, path } => editor.preview_media(session, &path),
            Effect::CopyEditorDocument { request } => editor.copy_document(&request),
            Effect::ShowEditorExportDialog { request } => editor.show_export_dialog(request),
            Effect::ShowEditorExportWarning { request } => editor.show_export_warning(&request),
            Effect::ExportEditorPdf { request } => editor.export_pdf(&request),
            _ => {}
        }
    }

    fn show_external_edit(&self, session: crate::mvu::EditorSessionId, deleted: bool) {
        use adw::prelude::*;
        let (Some(overlay), Some(dispatcher)) = (&self.toast_overlay, &self.dispatcher) else {
            return;
        };
        let toast = adw::Toast::builder()
            .title(if deleted {
                "This note was deleted elsewhere. Your open text is preserved."
            } else {
                "This note changed elsewhere. Your unsaved edits are preserved."
            })
            .button_label(if deleted {
                "Close Note…"
            } else {
                "Reload…"
            })
            .timeout(0)
            .build();
        let parent = self.route_stack.clone();
        let dispatcher = dispatcher.clone();
        toast.connect_button_clicked(move |_| {
            let dialog = adw::AlertDialog::builder()
                .heading(if deleted { "Close deleted note?" } else { "Reload note?" })
                .body(if deleted { "Closing discards your local draft and opens Trash, where you can restore the saved note. Cancel to keep or export your draft first." }
                    else { "Reloading replaces your unsaved edits with the latest saved note." })
                .default_response("cancel")
                .close_response("cancel")
                .build();
            dialog.add_responses(&[("cancel", "Cancel"), ("reload", if deleted { "Discard Draft and Open Trash" } else { "Discard Edits and Reload" })]);
            dialog.set_response_appearance("reload", adw::ResponseAppearance::Destructive);
            let dispatcher = dispatcher.clone();
            dialog.connect_response(None, move |_, response| {
                if response == "reload" {
                    let _ = dispatcher.dispatch(AppMsg::Editor(
                        if deleted { crate::mvu::EditorMsg::CloseDeleted { session } }
                        else { crate::mvu::EditorMsg::ReloadExternal { session } },
                    ));
                } else {
                    let _ = dispatcher.dispatch(AppMsg::Editor(
                        crate::mvu::EditorMsg::KeepExternalDraft { session },
                    ));
                }
            });
            dialog.present(Some(&parent));
        });
        let previous = self
            .external_change_toast
            .replace(Some((session, toast.clone())));
        if let Some((_, previous)) = previous {
            previous.dismiss();
        }
        overlay.add_toast(toast);
    }

    fn clear_resolved_external_notice(&self, model: &AppModel) {
        let resolved = self
            .external_change_toast
            .borrow()
            .as_ref()
            .is_some_and(|(session, _)| {
                model.editor.as_ref().is_none_or(|document| {
                    document.session != *session || document.external_change.is_none()
                })
            });
        if resolved {
            let previous = self.external_change_toast.take();
            if let Some((_, toast)) = previous {
                toast.dismiss();
            }
        }
    }

    /// Reports whether a widget signal was caused by a programmatic render.
    #[must_use]
    pub fn is_rendering(&self) -> bool {
        self.rendering.get()
    }

    fn render_sidebar(&self, model: &AppModel) {
        if let Some(renderer) = &self.sidebar_renderer {
            let Some(snapshot) = SidebarSnapshot::from_model(model) else {
                if matches!(model.sidebar.state, LoadState::Loading(_)) {
                    return;
                }
                if self.last_sidebar_snapshot.borrow_mut().take().is_some() {
                    renderer(model);
                }
                return;
            };
            let changed = self.last_sidebar_snapshot.borrow().as_ref() != Some(&snapshot);
            if changed {
                self.last_sidebar_snapshot.replace(Some(snapshot));
                renderer(model);
            }
        }
    }

    fn render_editor(&self, model: &AppModel) {
        if let Some(editor) = &self.editor {
            editor.render(model);
        }
    }

    fn render_browser(&self, model: &AppModel) {
        self.render_browser_search(model);
        // Keep the complete previous browser visible until both lists are ready.
        if matches!(model.browser.notes.state, LoadState::Loading(_))
            && !model.browser.loading_indicator_visible
            && model.browser.last_ready_notes.is_some()
        {
            return;
        }
        if !self.browser_projection_changed(model) {
            return;
        }
        let Some(BrowserContentRefs {
            list,
            feed_store,
            pages,
            empty_new_note,
        }) = self.browser_content_refs()
        else {
            return;
        };
        match &model.browser.notes.state {
            LoadState::Ready(notes)
                if notes.is_empty() && !model.browser.search_query.trim().is_empty() =>
            {
                self.render_browser_special(feed_store, model, BrowserFeedItem::SearchEmpty);
                empty_new_note.set_visible(false);
                pages.set_visible_child_name("contents");
            }
            LoadState::Ready(notes) if notes.is_empty() && model.selected_category.is_some() => {
                self.render_browser_special(feed_store, model, BrowserFeedItem::CategoryEmpty);
                empty_new_note.set_visible(false);
                pages.set_visible_child_name("contents");
            }
            LoadState::Ready(notes) if notes.is_empty() => {
                feed_store.remove_all();
                self.browser_rendered_rows.borrow_mut().clear();
                empty_new_note.set_visible(true);
                self.browser_status.set_title("No notes yet");
                self.browser_status
                    .set_description(Some("Create a note to get started."));
                pages.set_visible_child_name("empty");
            }
            LoadState::Ready(notes) => {
                self.render_browser_notes(list, feed_store, pages, empty_new_note, notes, model);
            }
            state => {
                feed_store.remove_all();
                self.browser_rendered_rows.borrow_mut().clear();
                empty_new_note.set_visible(false);
                self.browser_status
                    .set_title(resource_label(state, "No notes yet"));
                if let LoadState::Failed(error) = state {
                    self.browser_status.set_description(Some(&error.message));
                }
                pages.set_visible_child_name("empty");
            }
        }
    }

    fn browser_projection_changed(&self, model: &AppModel) -> bool {
        let today = crate::ui::browser::local_day(OffsetDateTime::now_utc());
        if self
            .last_browser_snapshot
            .borrow()
            .as_ref()
            .is_some_and(|snapshot| snapshot.matches(model, today))
        {
            return false;
        }
        self.last_browser_snapshot
            .replace(Some(browser_projection_snapshot(model, today)));
        true
    }

    fn browser_content_refs(&self) -> Option<BrowserContentRefs<'_>> {
        Some(BrowserContentRefs {
            list: self.browser_list.as_ref()?,
            feed_store: self.browser_feed_store.as_ref()?,
            pages: self.browser_pages.as_ref()?,
            empty_new_note: self.browser_empty_new_note_button.as_ref()?,
        })
    }

    fn render_browser_search(&self, model: &AppModel) {
        let (Some(bar), Some(entry), Some(toggle)) = (
            self.browser_search_bar.as_ref(),
            self.browser_search_entry.as_ref(),
            self.browser_search_toggle.as_ref(),
        ) else {
            return;
        };
        let was_open = self
            .last_browser_search_open
            .replace(model.browser.search_open);
        let opening = model.browser.search_open && !was_open;
        let closing = was_open && !model.browser.search_open;
        if bar.is_search_mode() != model.browser.search_open {
            bar.set_search_mode(model.browser.search_open);
        }
        if toggle.is_active() != model.browser.search_open {
            toggle.set_active(model.browser.search_open);
        }
        if entry.text().as_str() != model.browser.search_query {
            entry.set_text(&model.browser.search_query);
        }
        if opening {
            let bar = bar.clone();
            let entry = entry.clone();
            glib::idle_add_local_once(move || {
                if bar.is_search_mode() {
                    entry.grab_focus();
                    entry.select_region(0, -1);
                }
            });
        }
        if closing && let Some(list) = &self.browser_list {
            list.grab_focus();
        }
    }

    fn render_trash(&self, model: &AppModel) {
        if self.last_trash_snapshot.borrow().as_ref() == Some(&model.trash.state) {
            return;
        }
        self.last_trash_snapshot
            .replace(Some(model.trash.state.clone()));
        let (Some(list), Some(pages), Some(empty_button)) = (
            self.trash_list.as_ref(),
            self.trash_pages.as_ref(),
            self.empty_trash_button.as_ref(),
        ) else {
            render_resource(&self.trash_status, &model.trash.state, "Trash is empty");
            return;
        };
        while let Some(child) = list.first_child() {
            list.remove(&child);
        }
        match &model.trash.state {
            LoadState::Ready(contents) if contents.is_empty() => {
                self.trash_status.set_title("Trash is empty");
                self.trash_status
                    .set_description(Some("Deleted notes and categories can be restored here."));
                empty_button.set_sensitive(false);
                pages.set_visible_child_name("empty");
            }
            LoadState::Ready(contents) => {
                empty_button.set_sensitive(true);
                if !contents.categories.is_empty() {
                    append_section_heading(list, "Categories");
                    for category in &contents.categories {
                        list.append(&trashed_category_row(category));
                    }
                }
                if !contents.notes.is_empty() {
                    append_section_heading(list, "Notes");
                    for note in &contents.notes {
                        list.append(&trashed_note_row(note));
                    }
                }
                pages.set_visible_child_name("contents");
            }
            state => {
                empty_button.set_sensitive(false);
                self.trash_status
                    .set_title(resource_label(state, "Trash is empty"));
                if let LoadState::Failed(error) = state {
                    self.trash_status.set_description(Some(&error.message));
                }
                pages.set_visible_child_name("empty");
            }
        }
    }

    fn render_notice(&self, model: &AppModel) {
        let Some(error) = &model.notice else {
            self.last_notice.replace(None);
            return;
        };
        if self.last_notice.borrow().as_deref() == Some(error.message.as_str()) {
            return;
        }
        if let Some(toast_overlay) = &self.toast_overlay {
            toast_overlay.add_toast(adw::Toast::new(&error.message));
            self.last_notice.replace(Some(error.message.clone()));
        }
    }

    fn render_editor_save_error(&self, model: &AppModel) {
        let error = model.editor.as_ref().and_then(|document| {
            if let EditorSaveState::Failed(error) = &document.save_state {
                Some(error)
            } else {
                None
            }
        });
        let Some(error) = error else {
            self.last_editor_save_error.replace(None);
            return;
        };
        if self.last_editor_save_error.borrow().as_deref() == Some(error.message.as_str()) {
            return;
        }
        if let Some(toast_overlay) = &self.toast_overlay {
            let toast = adw::Toast::new(&format!("Could not save note: {}", error.message));
            toast.set_button_label(Some("Retry"));
            toast.set_action_name(Some("mvu.retry-save"));
            toast_overlay.add_toast(toast);
            self.last_editor_save_error
                .replace(Some(error.message.clone()));
        }
    }

    fn render_undo_move(&self, model: &AppModel) {
        if *self.last_undo_move.borrow() == model.undo_move {
            return;
        }
        self.last_undo_move.replace(model.undo_move);
        if model.undo_move.is_some()
            && let Some(toast_overlay) = &self.toast_overlay
        {
            let toast = adw::Toast::new("Moved note");
            toast.set_button_label(Some("Undo"));
            toast.set_action_name(Some("mvu.undo-move"));
            toast_overlay.add_toast(toast);
        }
    }

    fn render_undo_trash_note(&self, model: &AppModel) {
        if self.last_undo_trash_note.get() == model.undo_trash_note {
            return;
        }
        self.last_undo_trash_note.set(model.undo_trash_note);
        if model.undo_trash_note.is_some()
            && let Some(toast_overlay) = &self.toast_overlay
        {
            let toast = adw::Toast::new("Moved note to Trash");
            toast.set_button_label(Some("Undo"));
            toast.set_action_name(Some("mvu.undo-trash-note"));
            toast_overlay.add_toast(toast);
        }
    }
}

fn browser_projection_snapshot(model: &AppModel, today: Date) -> BrowserProjectionSnapshot {
    BrowserProjectionSnapshot {
        browser: model.browser.clone(),
        selected_category: model.selected_category,
        sidebar: model.sidebar.state.clone(),
        route: model.route,
        today,
    }
}

impl ViewRefs {
    fn render_browser_special(
        &self,
        feed_store: &gtk::gio::ListStore,
        model: &AppModel,
        item: BrowserFeedItem,
    ) {
        let context = browser_feed_context(model);
        if let Some(feed_context) = &self.browser_feed_context {
            feed_context.replace(context.clone());
        }
        feed_store.remove_all();
        feed_store.append(&glib::BoxedAnyObject::new(BrowserFeedItem::Hero));
        feed_store.append(&glib::BoxedAnyObject::new(item));
        self.browser_rendered_rows.borrow_mut().clear();
        self.browser_rendered_context.replace(Some(context));
    }

    fn render_browser_notes(
        &self,
        _list: &gtk::ListView,
        feed_store: &gtk::gio::ListStore,
        pages: &gtk::Stack,
        empty_new_note: &gtk::Button,
        notes: &[carver_sdk::NoteSummary],
        model: &AppModel,
    ) {
        empty_new_note.set_visible(true);
        pages.set_visible_child_name("contents");
        let context = browser_feed_context(model);
        if let Some(feed_context) = &self.browser_feed_context {
            feed_context.replace(context.clone());
        }
        let show_date_groups = model.browser.search_query.trim().is_empty();
        let existing_context = self.browser_rendered_context.borrow().clone();
        let incoming: Vec<_> = notes.iter().map(|note| (note.id, note.revision)).collect();
        let mut rendered = self.browser_rendered_rows.borrow_mut();
        let can_append = existing_context.as_ref() == Some(&context)
            && notes.len() >= rendered.len()
            && incoming
                .get(..rendered.len())
                .is_some_and(|prefix| prefix == rendered.as_slice());
        if can_append {
            remove_browser_footer(feed_store);
        } else {
            feed_store.remove_all();
            rendered.clear();
            append_browser_prelude(feed_store, model);
        }
        let now = OffsetDateTime::now_utc();
        let mut previous_group = rendered.last().and_then(|(id, _)| {
            notes
                .iter()
                .find(|note| note.id == *id)
                .map(|note| crate::ui::browser::note_date_group(note.updated_at, now))
        });
        for note in &notes[rendered.len()..] {
            if show_date_groups {
                let group = crate::ui::browser::note_date_group(note.updated_at, now);
                if previous_group != Some(group) {
                    feed_store.append(&glib::BoxedAnyObject::new(BrowserFeedItem::Heading(group)));
                    previous_group = Some(group);
                }
            }
            feed_store.append(&glib::BoxedAnyObject::new(BrowserFeedItem::Note(
                note.clone(),
            )));
        }
        append_browser_footer(feed_store, model);
        *rendered = incoming;
        self.browser_rendered_context.replace(Some(context));
    }
}

fn browser_feed_context(model: &AppModel) -> BrowserFeedContext {
    let favorites = match &model.browser.favorites.state {
        LoadState::Ready(notes) if model.browser.search_query.trim().is_empty() => {
            notes.iter().map(|note| (note.id, note.revision)).collect()
        }
        _ => Vec::new(),
    };
    BrowserFeedContext {
        show_category: model.selected_category.is_none(),
        sidebar: model.sidebar.state.clone(),
        selected_category: model.selected_category,
        favorites,
    }
}

fn append_browser_prelude(feed_store: &gtk::gio::ListStore, model: &AppModel) {
    feed_store.append(&glib::BoxedAnyObject::new(BrowserFeedItem::Hero));
    if model.browser.search_query.trim().is_empty()
        && let LoadState::Ready(notes) = &model.browser.favorites.state
        && !notes.is_empty()
    {
        feed_store.append(&glib::BoxedAnyObject::new(
            BrowserFeedItem::FavoritesHeading,
        ));
        for note in notes {
            feed_store.append(&glib::BoxedAnyObject::new(BrowserFeedItem::Favorite(
                note.clone(),
            )));
        }
    }
}

fn append_browser_footer(feed_store: &gtk::gio::ListStore, model: &AppModel) {
    let loading = model.browser.append_request.is_some();
    if model.browser.has_more || model.browser.append_error.is_some() {
        let label = if loading {
            "Loading more notes…"
        } else if model.browser.append_error.is_some() {
            "Retry loading notes"
        } else {
            "Load more notes"
        };
        feed_store.append(&glib::BoxedAnyObject::new(BrowserFeedItem::LoadMore {
            label: label.to_string(),
            sensitive: !loading,
        }));
    }
}

fn remove_browser_footer(feed_store: &gtk::gio::ListStore) {
    let Some(position) = feed_store.n_items().checked_sub(1) else {
        return;
    };
    let Some(item) = feed_store
        .item(position)
        .and_downcast::<glib::BoxedAnyObject>()
    else {
        return;
    };
    if matches!(
        &*item.borrow::<BrowserFeedItem>(),
        BrowserFeedItem::LoadMore { .. }
    ) {
        feed_store.remove(position);
    }
}

fn render_resource<T>(status: &adw::StatusPage, resource: &LoadState<T>, empty_title: &str) {
    match resource {
        LoadState::Idle | LoadState::Ready(_) => status.set_title(empty_title),
        LoadState::Loading(_) => status.set_title("Loading…"),
        LoadState::Failed(error) => {
            status.set_title("Could not load content");
            status.set_description(Some(&error.message));
        }
    }
}

fn resource_label<T>(resource: &LoadState<T>, empty: &'static str) -> &'static str {
    match resource {
        LoadState::Idle | LoadState::Ready(_) => empty,
        LoadState::Loading(_) => "Loading…",
        LoadState::Failed(_) => "Could not load content",
    }
}

fn append_section_heading(list: &gtk::ListBox, text: &str) {
    let label = gtk::Label::new(Some(text));
    label.set_xalign(0.0);
    label.add_css_class("date-heading-label");
    append_section_heading_widget(list, &label);
}

fn append_section_heading_widget(list: &gtk::ListBox, child: &impl IsA<gtk::Widget>) {
    let row = gtk::ListBoxRow::new();
    row.set_selectable(false);
    row.add_css_class("date-heading");
    row.set_child(Some(child));
    list.append(&row);
}

fn trashed_category_row(category: &carver_sdk::TrashedCategorySummary) -> gtk::ListBoxRow {
    let row = gtk::ListBoxRow::new();
    row.set_widget_name(&format!("trashed-category:{}", category.category.id));
    row.add_css_class("card");
    row.add_css_class("note-card");
    let content = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    content.set_margin_start(12);
    content.set_margin_end(12);
    content.set_margin_top(10);
    content.set_margin_bottom(10);
    let details = gtk::Box::new(gtk::Orientation::Vertical, 4);
    details.set_hexpand(true);
    let title = gtk::Label::new(Some(&category.category.name));
    title.set_xalign(0.0);
    title.add_css_class("note-card-title");
    details.append(&title);
    let recovery_count = gtk::Label::new(Some(&format!(
        "{} recoverable notes",
        category.recoverable_note_count
    )));
    recovery_count.set_xalign(0.0);
    recovery_count.set_ellipsize(gtk::pango::EllipsizeMode::End);
    recovery_count.set_single_line_mode(true);
    recovery_count.add_css_class("note-card-updated");
    details.append(&recovery_count);
    content.append(&details);
    content.append(&restore_button(
        &format!("restore-category:{}", category.category.id),
        "trash.restore-category",
        &category.category.id.to_string(),
    ));
    row.set_child(Some(&content));
    row
}

fn trashed_note_row(note: &carver_sdk::TrashedNoteSummary) -> gtk::ListBoxRow {
    let row = gtk::ListBoxRow::new();
    row.set_widget_name(&format!("trashed-note:{}", note.id));
    row.add_css_class("card");
    row.add_css_class("note-card");
    let content = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    content.set_margin_start(12);
    content.set_margin_end(12);
    content.set_margin_top(10);
    content.set_margin_bottom(10);
    let details = gtk::Box::new(gtk::Orientation::Vertical, 4);
    details.set_hexpand(true);
    let title = gtk::Label::new(Some(&note.title));
    title.set_xalign(0.0);
    title.add_css_class("note-card-title");
    title.set_ellipsize(gtk::pango::EllipsizeMode::End);
    title.set_single_line_mode(true);
    details.append(&title);
    let excerpt_text = crate::ui::browser::compact_note_excerpt(&note.title, &note.excerpt);
    if !excerpt_text.is_empty() {
        let excerpt = gtk::Label::new(Some(&excerpt_text));
        excerpt.set_widget_name(&format!("trashed-note-excerpt:{}", note.id));
        excerpt.set_xalign(0.0);
        excerpt.set_ellipsize(gtk::pango::EllipsizeMode::End);
        excerpt.set_single_line_mode(true);
        excerpt.add_css_class("note-card-excerpt");
        details.append(&excerpt);
    }
    content.append(&details);
    content.append(&restore_button(
        &format!("restore-note:{}", note.id),
        "trash.restore-note",
        &note.id.to_string(),
    ));
    row.set_child(Some(&content));
    row
}

fn restore_button(name: &str, action: &str, target: &str) -> gtk::Button {
    let restore = gtk::Button::from_icon_name("edit-undo-symbolic");
    restore.set_widget_name(name);
    restore.add_css_class("flat");
    restore.set_tooltip_text(Some("Restore"));
    restore.update_property(&[gtk::accessible::Property::Label("Restore")]);
    restore.set_valign(gtk::Align::Center);
    restore.set_action_name(Some(action));
    restore.set_action_target_value(Some(&target.to_variant()));
    restore
}
