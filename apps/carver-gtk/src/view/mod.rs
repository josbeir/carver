//! Imperative GTK rendering adapters for MVU model snapshots.

use std::cell::{Cell, RefCell};

use carver_sdk::{CategoryId, CategorySummary};
use gtk::prelude::*;
use libadwaita as adw;

use crate::mvu::{AppModel, EditorSaveState, LoadState, MoveUndo, Route};

type SidebarRenderer = Box<dyn Fn(&AppModel)>;
type SidebarSnapshot = (LoadState<Vec<CategorySummary>>, Option<CategoryId>);

/// GTK references used to render the high-level MVU resources.
///
/// This type intentionally owns widgets only. Application state lives in [`AppModel`].
pub struct ViewRefs {
    route_stack: gtk::Stack,
    sidebar_list: Option<gtk::ListBox>,
    browser_list: Option<gtk::ListBox>,
    browser_pages: Option<gtk::Stack>,
    browser_search_empty_card: Option<gtk::Box>,
    browser_empty_new_note_button: Option<gtk::Button>,
    browser_title: Option<adw::WindowTitle>,
    browser_status: adw::StatusPage,
    trash_list: Option<gtk::ListBox>,
    trash_pages: Option<gtk::Stack>,
    empty_trash_button: Option<gtk::Button>,
    trash_status: adw::StatusPage,
    toast_overlay: Option<adw::ToastOverlay>,
    last_notice: RefCell<Option<String>>,
    last_editor_save_error: RefCell<Option<String>>,
    last_undo_move: RefCell<Option<MoveUndo>>,
    sidebar_renderer: Option<SidebarRenderer>,
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
            sidebar_list: None,
            browser_list: None,
            browser_pages: None,
            browser_search_empty_card: None,
            browser_empty_new_note_button: None,
            browser_title: None,
            browser_status,
            trash_list: None,
            trash_pages: None,
            empty_trash_button: None,
            trash_status,
            toast_overlay: None,
            last_notice: RefCell::new(None),
            last_editor_save_error: RefCell::new(None),
            last_undo_move: RefCell::new(None),
            sidebar_renderer: None,
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

    /// Adds the browser and sidebar widgets created by the legacy composition shell.
    #[must_use]
    pub fn with_browser_and_sidebar(
        mut self,
        sidebar_list: gtk::ListBox,
        browser_list: gtk::ListBox,
        browser_pages: gtk::Stack,
        browser_search_empty_card: gtk::Box,
        browser_empty_new_note_button: gtk::Button,
        browser_title: adw::WindowTitle,
    ) -> Self {
        self.sidebar_list = Some(sidebar_list);
        self.browser_list = Some(browser_list);
        self.browser_pages = Some(browser_pages);
        self.browser_search_empty_card = Some(browser_search_empty_card);
        self.browser_empty_new_note_button = Some(browser_empty_new_note_button);
        self.browser_title = Some(browser_title);
        self
    }

    /// Uses the composition shell's full category-row renderer for changed MVU snapshots.
    #[must_use]
    pub fn with_sidebar_renderer(mut self, renderer: impl Fn(&AppModel) + 'static) -> Self {
        self.sidebar_renderer = Some(Box::new(renderer));
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
        self.render_notice(model);
        self.render_editor_save_error(model);
        self.render_undo_move(model);
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
            let mut last_snapshot = self.last_sidebar_snapshot.borrow_mut();
            if last_snapshot.as_ref() != Some(&snapshot) {
                renderer(model);
                *last_snapshot = Some(snapshot);
            }
            return;
        }
        let Some(list) = self.sidebar_list.as_ref() else {
            return;
        };
        while let Some(child) = list.first_child() {
            list.remove(&child);
        }
        let LoadState::Ready(categories) = &model.sidebar.state else {
            let row = gtk::ListBoxRow::new();
            row.set_selectable(false);
            let label = gtk::Label::new(Some(resource_label(
                &model.sidebar.state,
                "No categories yet",
            )));
            label.set_margin_top(12);
            label.set_margin_bottom(12);
            row.set_child(Some(&label));
            list.append(&row);
            return;
        };
        let all_count = categories
            .iter()
            .map(|summary| summary.note_count)
            .sum::<usize>();
        list.append(&sidebar_row("All notes", all_count, None));
        for summary in categories {
            list.append(&sidebar_row(
                &summary.category.name,
                summary.note_count,
                Some(summary.category.id),
            ));
        }
    }

    fn render_browser(&self, model: &AppModel) {
        let (Some(list), Some(pages), Some(search_empty), Some(empty_new_note)) = (
            self.browser_list.as_ref(),
            self.browser_pages.as_ref(),
            self.browser_search_empty_card.as_ref(),
            self.browser_empty_new_note_button.as_ref(),
        ) else {
            return;
        };
        if let Some(title) = &self.browser_title {
            let selected_name = match (&model.sidebar.state, model.selected_category) {
                (LoadState::Ready(categories), Some(category_id)) => categories
                    .iter()
                    .find(|summary| summary.category.id == category_id)
                    .map(|summary| summary.category.name.as_str()),
                _ => None,
            };
            title.set_title(selected_name.unwrap_or("Home"));
            title.set_subtitle(if selected_name.is_some() {
                "Recently edited"
            } else {
                "All recent notes"
            });
        }
        match &model.browser.notes.state {
            LoadState::Ready(notes)
                if notes.is_empty() && !model.browser.search_query.trim().is_empty() =>
            {
                clear_list(list);
                search_empty.set_visible(true);
                empty_new_note.set_visible(false);
                pages.set_visible_child_name("contents");
            }
            LoadState::Ready(notes) if notes.is_empty() => {
                clear_list(list);
                search_empty.set_visible(false);
                empty_new_note.set_visible(true);
                self.browser_status.set_title("No notes yet");
                self.browser_status
                    .set_description(Some("Create a note to get started."));
                pages.set_visible_child_name("empty");
            }
            LoadState::Ready(notes) => {
                clear_list(list);
                search_empty.set_visible(false);
                empty_new_note.set_visible(true);
                pages.set_visible_child_name("contents");
                let show_category = model.selected_category.is_none();
                for note in notes {
                    list.append(&browser_row(note, show_category));
                }
            }
            LoadState::Loading(_) if list.first_child().is_some() => {
                search_empty.set_visible(false);
                empty_new_note.set_visible(true);
                pages.set_visible_child_name("contents");
            }
            state => {
                clear_list(list);
                search_empty.set_visible(false);
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

fn sidebar_row(
    name: &str,
    count: usize,
    category_id: Option<carver_sdk::CategoryId>,
) -> gtk::ListBoxRow {
    let row = gtk::ListBoxRow::new();
    row.set_selectable(true);
    if let Some(category_id) = category_id {
        row.set_widget_name(&format!("category:{category_id}"));
    }
    let content = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    content.set_margin_start(16);
    content.set_margin_end(16);
    content.set_margin_top(6);
    content.set_margin_bottom(6);
    let label = gtk::Label::new(Some(name));
    label.set_xalign(0.0);
    label.set_hexpand(true);
    content.append(&label);
    content.append(&gtk::Label::new(Some(&format!("{count}"))));
    row.set_child(Some(&content));
    row
}

fn clear_list(list: &gtk::ListBox) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }
}

fn browser_row(note: &carver_sdk::NoteSummary, show_category: bool) -> gtk::ListBoxRow {
    let row = gtk::ListBoxRow::new();
    row.set_widget_name(&format!("note:{}", note.id));
    row.add_css_class("note-card");
    let content = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    content.set_margin_start(12);
    content.set_margin_end(8);
    content.set_margin_top(10);
    content.set_margin_bottom(10);
    content.append(&crate::browser::note_card_details(note, show_category));
    row.set_child(Some(&content));
    row
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

fn trashed_category_row(category: &carver_sdk::TrashedCategorySummary) -> gtk::ListBoxRow {
    let row = gtk::ListBoxRow::new();
    row.set_widget_name(&format!("trashed-category:{}", category.category.id));
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
    details.append(&gtk::Label::new(Some(&format!(
        "{} recoverable notes",
        category.recoverable_note_count
    ))));
    content.append(&details);
    let restore = gtk::Button::with_label("Restore");
    restore.set_widget_name(&format!("restore-category:{}", category.category.id));
    restore.set_action_name(Some("trash.restore-category"));
    restore.set_action_target_value(Some(&category.category.id.to_string().to_variant()));
    content.append(&restore);
    row.set_child(Some(&content));
    row
}

fn trashed_note_row(note: &carver_sdk::TrashedNoteSummary) -> gtk::ListBoxRow {
    let row = gtk::ListBoxRow::new();
    row.set_widget_name(&format!("trashed-note:{}", note.id));
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
    details.append(&title);
    let excerpt = gtk::Label::new(Some(&note.excerpt));
    excerpt.set_xalign(0.0);
    excerpt.add_css_class("note-card-excerpt");
    details.append(&excerpt);
    content.append(&details);
    let restore = gtk::Button::with_label("Restore");
    restore.set_widget_name(&format!("restore-note:{}", note.id));
    restore.set_action_name(Some("trash.restore-note"));
    restore.set_action_target_value(Some(&note.id.to_string().to_variant()));
    content.append(&restore);
    row.set_child(Some(&content));
    row
}
