//! Imperative GTK rendering adapters for MVU model snapshots.

#[cfg(test)]
mod tests;

use std::cell::{Cell, RefCell};

use gtk::prelude::*;
use libadwaita as adw;
use time::OffsetDateTime;

use crate::{
    dialogs::show_move_note_dialog,
    mvu::{
        ActionMsg, AppDispatcher, AppModel, AppMsg, EditorSaveState, LoadState, MoveUndo, Route,
    },
};

type SidebarRenderer = Box<dyn Fn(&AppModel)>;
type SidebarSnapshot = (
    LoadState<Vec<carver_sdk::CategorySummary>>,
    Option<carver_sdk::CategoryId>,
);

struct BrowserContentRefs<'a> {
    list: &'a gtk::ListBox,
    pages: &'a gtk::Stack,
    search_empty: &'a gtk::Box,
    category_empty: &'a gtk::Box,
    empty_new_note: &'a gtk::Button,
    category_empty_new_note: &'a gtk::Button,
}

/// GTK references used to render the high-level MVU resources.
///
/// This type intentionally owns widgets only. Application state lives in [`AppModel`].
pub struct ViewRefs {
    route_stack: gtk::Stack,
    browser_list: Option<gtk::ListBox>,
    browser_pages: Option<gtk::Stack>,
    browser_search_bar: Option<gtk::SearchBar>,
    browser_search_entry: Option<gtk::SearchEntry>,
    browser_search_toggle: Option<gtk::ToggleButton>,
    browser_search_empty_card: Option<gtk::Box>,
    browser_category_empty_card: Option<gtk::Box>,
    browser_empty_new_note_button: Option<gtk::Button>,
    browser_category_empty_new_note_button: Option<gtk::Button>,
    browser_category_hero: Option<gtk::Box>,
    browser_status: adw::StatusPage,
    trash_list: Option<gtk::ListBox>,
    trash_pages: Option<gtk::Stack>,
    empty_trash_button: Option<gtk::Button>,
    trash_status: adw::StatusPage,
    toast_overlay: Option<adw::ToastOverlay>,
    dispatcher: Option<AppDispatcher>,
    last_notice: RefCell<Option<String>>,
    last_editor_save_error: RefCell<Option<String>>,
    last_undo_move: RefCell<Option<MoveUndo>>,
    last_undo_trash_note: Cell<Option<carver_sdk::NoteId>>,
    last_browser_search_open: Cell<bool>,
    has_browser_snapshot: Cell<bool>,
    sidebar_renderer: Option<SidebarRenderer>,
    editor: Option<crate::editor::EditorViewRefs>,
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
            browser_pages: None,
            browser_search_bar: None,
            browser_search_entry: None,
            browser_search_toggle: None,
            browser_search_empty_card: None,
            browser_category_empty_card: None,
            browser_empty_new_note_button: None,
            browser_category_empty_new_note_button: None,
            browser_category_hero: None,
            browser_status,
            trash_list: None,
            trash_pages: None,
            empty_trash_button: None,
            trash_status,
            toast_overlay: None,
            dispatcher: None,
            last_notice: RefCell::new(None),
            last_editor_save_error: RefCell::new(None),
            last_undo_move: RefCell::new(None),
            last_undo_trash_note: Cell::new(None),
            last_browser_search_open: Cell::new(false),
            has_browser_snapshot: Cell::new(false),
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
    pub(crate) fn with_browser(mut self, browser: crate::browser::BrowserViewRefs) -> Self {
        self.browser_list = Some(browser.list);
        self.browser_pages = Some(browser.pages);
        self.browser_search_bar = Some(browser.search_bar);
        self.browser_search_entry = Some(browser.search_entry);
        self.browser_search_toggle = Some(browser.search_toggle);
        self.browser_search_empty_card = Some(browser.search_empty_card);
        self.browser_category_empty_card = Some(browser.category_empty_card);
        self.browser_empty_new_note_button = Some(browser.empty_new_note_button);
        self.browser_category_empty_new_note_button = Some(browser.category_empty_new_note_button);
        self.browser_category_hero = Some(browser.category_hero);
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
    pub(crate) fn with_editor(mut self, editor: crate::editor::EditorViewRefs) -> Self {
        self.editor = Some(editor);
        self
    }

    /// Renders one immutable model snapshot without invoking application actions.
    pub fn render(&self, model: &AppModel) {
        self.rendering.set(true);
        self.route_stack.set_visible_child_name(match model.route {
            Route::Browser => "browser",
            Route::Trash => "trash",
            Route::Editor => "editor",
        });
        self.render_sidebar(model);
        self.render_browser(model);
        self.render_trash(model);
        self.render_editor(model);
        self.render_notice(model);
        self.render_editor_save_error(model);
        self.render_undo_move(model);
        self.render_undo_trash_note(model);
        self.rendering.set(false);
    }

    /// Reports whether a widget signal was caused by a programmatic render.
    #[must_use]
    pub fn is_rendering(&self) -> bool {
        self.rendering.get()
    }

    fn render_sidebar(&self, model: &AppModel) {
        if let Some(renderer) = &self.sidebar_renderer {
            let snapshot = (model.sidebar.state.clone(), model.selected_category);
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
        let Some(BrowserContentRefs {
            list,
            pages,
            search_empty,
            category_empty,
            empty_new_note,
            category_empty_new_note,
        }) = self.browser_content_refs()
        else {
            return;
        };
        if let Some(hero) = &self.browser_category_hero {
            crate::browser::render_category_hero(
                hero,
                &model.sidebar.state,
                model.selected_category,
                self.dispatcher.as_ref(),
            );
        }
        if matches!(model.browser.notes.state, LoadState::Ready(_)) {
            self.has_browser_snapshot.set(true);
        }
        match &model.browser.notes.state {
            LoadState::Ready(notes)
                if notes.is_empty() && !model.browser.search_query.trim().is_empty() =>
            {
                clear_list(list);
                search_empty.set_visible(true);
                category_empty.set_visible(false);
                empty_new_note.set_visible(false);
                category_empty_new_note.set_visible(false);
                pages.set_visible_child_name("contents");
            }
            LoadState::Ready(notes) if notes.is_empty() && model.selected_category.is_some() => {
                render_empty_category(
                    list,
                    pages,
                    search_empty,
                    category_empty,
                    empty_new_note,
                    category_empty_new_note,
                );
            }
            LoadState::Ready(notes) if notes.is_empty() => {
                clear_list(list);
                search_empty.set_visible(false);
                category_empty.set_visible(false);
                empty_new_note.set_visible(true);
                category_empty_new_note.set_visible(false);
                self.browser_status.set_title("No notes yet");
                self.browser_status
                    .set_description(Some("Create a note to get started."));
                pages.set_visible_child_name("empty");
            }
            LoadState::Ready(notes) => {
                clear_list(list);
                search_empty.set_visible(false);
                category_empty.set_visible(false);
                empty_new_note.set_visible(true);
                category_empty_new_note.set_visible(false);
                pages.set_visible_child_name("contents");
                let show_category = model.selected_category.is_none();
                let show_date_groups = model.browser.search_query.trim().is_empty();
                let now = OffsetDateTime::now_utc();
                let mut previous_group = None;
                for note in notes {
                    if show_date_groups {
                        let group = crate::browser::note_date_group(note.updated_at, now);
                        if previous_group != Some(group) {
                            append_note_date_group_heading(list, group);
                            previous_group = Some(group);
                        }
                    }
                    list.append(&browser_row(
                        note,
                        show_category,
                        &model.sidebar.state,
                        self.dispatcher.as_ref(),
                    ));
                }
            }
            // CONTEXT: Fast reloads retain the last rendered snapshot rather than flashing a
            // loading status between category selections.
            LoadState::Loading(_)
                if self.has_browser_snapshot.get() && !model.browser.loading_indicator_visible => {}
            state => {
                clear_list(list);
                search_empty.set_visible(false);
                category_empty.set_visible(false);
                empty_new_note.set_visible(false);
                category_empty_new_note.set_visible(false);
                self.browser_status
                    .set_title(resource_label(state, "No notes yet"));
                if let LoadState::Failed(error) = state {
                    self.browser_status.set_description(Some(&error.message));
                }
                pages.set_visible_child_name("empty");
            }
        }
    }

    fn browser_content_refs(&self) -> Option<BrowserContentRefs<'_>> {
        Some(BrowserContentRefs {
            list: self.browser_list.as_ref()?,
            pages: self.browser_pages.as_ref()?,
            search_empty: self.browser_search_empty_card.as_ref()?,
            category_empty: self.browser_category_empty_card.as_ref()?,
            empty_new_note: self.browser_empty_new_note_button.as_ref()?,
            category_empty_new_note: self.browser_category_empty_new_note_button.as_ref()?,
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

fn render_empty_category(
    list: &gtk::ListBox,
    pages: &gtk::Stack,
    search_empty: &gtk::Box,
    category_empty: &gtk::Box,
    empty_new_note: &gtk::Button,
    category_empty_new_note: &gtk::Button,
) {
    clear_list(list);
    search_empty.set_visible(false);
    category_empty.set_visible(true);
    empty_new_note.set_visible(false);
    category_empty_new_note.set_visible(true);
    pages.set_visible_child_name("contents");
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

fn clear_list(list: &gtk::ListBox) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }
}

fn browser_row(
    note: &carver_sdk::NoteSummary,
    show_category: bool,
    sidebar: &LoadState<Vec<carver_sdk::CategorySummary>>,
    dispatcher: Option<&AppDispatcher>,
) -> gtk::ListBoxRow {
    let row = gtk::ListBoxRow::new();
    row.set_widget_name(&format!("note:{}", note.id));
    row.add_css_class("card");
    row.add_css_class("activatable");
    row.add_css_class("note-card");
    let content = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    content.set_margin_start(12);
    content.set_margin_end(8);
    content.set_margin_top(10);
    content.set_margin_bottom(10);
    let category_color = show_category
        .then(|| note_category_color(note, sidebar))
        .flatten();
    content.append(&crate::browser::note_card_details(
        note,
        show_category,
        category_color,
    ));
    if let (LoadState::Ready(categories), Some(dispatcher)) = (sidebar, dispatcher) {
        content.append(&note_actions(note, categories, dispatcher));
    }
    row.set_child(Some(&content));
    row
}

fn note_category_color(
    note: &carver_sdk::NoteSummary,
    sidebar: &LoadState<Vec<carver_sdk::CategorySummary>>,
) -> Option<carver_sdk::CategoryColor> {
    let LoadState::Ready(categories) = sidebar else {
        return None;
    };
    categories
        .iter()
        .find(|summary| summary.category.id == note.category_id)
        .map(|summary| {
            summary
                .category
                .appearance
                .color
                .resolved_for(note.category_id)
        })
}

fn note_actions(
    note: &carver_sdk::NoteSummary,
    categories: &[carver_sdk::CategorySummary],
    dispatcher: &AppDispatcher,
) -> gtk::MenuButton {
    let menu = gtk::MenuButton::new();
    menu.set_widget_name(&format!("note-menu:{}", note.id));
    menu.set_icon_name("view-more-symbolic");
    menu.set_tooltip_text(Some("Note actions"));
    menu.add_css_class("flat");
    let popover = gtk::Popover::new();
    let actions = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let move_button = gtk::Button::with_label("Move…");
    move_button.set_widget_name(&format!("move-note-button:{}", note.id));
    move_button.add_css_class("flat");
    let dispatcher_for_move = dispatcher.clone();
    let note_id = note.id;
    let source_category_id = note.category_id;
    let note_title = note.title.clone();
    let categories = categories.to_vec();
    let popover_for_move = popover.clone();
    move_button.connect_clicked(move |button| {
        popover_for_move.popdown();
        let parent = button
            .root()
            .and_then(|root| root.downcast::<gtk::Window>().ok());
        show_move_note_dialog(
            parent.as_ref(),
            &dispatcher_for_move,
            note_id,
            source_category_id,
            &note_title,
            &categories,
        );
    });
    actions.append(&move_button);
    let export_button = gtk::Button::with_label("Export note…");
    export_button.set_widget_name(&format!("export-note-button:{}", note.id));
    export_button.add_css_class("flat");
    let dispatcher_for_export = dispatcher.clone();
    let note_id = note.id;
    let popover_for_export = popover.clone();
    export_button.connect_clicked(move |_| {
        popover_for_export.popdown();
        let _ = dispatcher_for_export.dispatch(AppMsg::Navigation(
            crate::mvu::NavigationMsg::ExportNote(note_id),
        ));
    });
    actions.append(&export_button);
    let trash_button = gtk::Button::with_label("Move to Trash");
    trash_button.add_css_class("flat");
    trash_button.add_css_class("destructive-action");
    let dispatcher = dispatcher.clone();
    let note_id = note.id;
    trash_button.connect_clicked(move |_| {
        let _ = dispatcher.dispatch(AppMsg::Action(ActionMsg::TrashNote(note_id)));
    });
    actions.append(&trash_button);
    popover.set_child(Some(&actions));
    menu.set_popover(Some(&popover));
    menu
}

fn append_section_heading(list: &gtk::ListBox, text: &str) {
    let row = gtk::ListBoxRow::new();
    row.set_selectable(false);
    row.add_css_class("date-heading");
    let label = gtk::Label::new(Some(text));
    label.set_xalign(0.0);
    label.add_css_class("date-heading-label");
    row.set_child(Some(&label));
    list.append(&row);
}

fn append_note_date_group_heading(list: &gtk::ListBox, group: crate::browser::NoteDateGroup) {
    let row = gtk::ListBoxRow::new();
    row.set_widget_name(&format!("note-group:{}", group.identifier()));
    row.set_selectable(false);
    row.add_css_class("date-heading");
    let label = gtk::Label::new(Some(&group.label()));
    label.set_xalign(0.0);
    label.add_css_class("date-heading-label");
    row.set_child(Some(&label));
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
    let excerpt_text = crate::browser::compact_note_excerpt(&note.title, &note.excerpt);
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
