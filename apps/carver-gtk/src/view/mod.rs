//! Imperative GTK rendering adapters for MVU model snapshots.

use std::cell::Cell;

use gtk::prelude::*;
use libadwaita as adw;

use crate::mvu::{AppModel, LoadState, Route};

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
    trash_status: adw::StatusPage,
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
            trash_status,
            rendering: Cell::new(false),
        }
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
        render_resource(&self.trash_status, &model.trash.state, "Trash is empty");
        self.rendering.set(false);
    }

    /// Reports whether a widget signal was caused by a programmatic render.
    #[must_use]
    pub fn is_rendering(&self) -> bool {
        self.rendering.get()
    }

    fn render_sidebar(&self, model: &AppModel) {
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
        while let Some(child) = list.first_child() {
            list.remove(&child);
        }
        match &model.browser.notes.state {
            LoadState::Ready(notes)
                if notes.is_empty() && !model.browser.search_query.trim().is_empty() =>
            {
                search_empty.set_visible(true);
                empty_new_note.set_visible(false);
                pages.set_visible_child_name("contents");
            }
            LoadState::Ready(notes) if notes.is_empty() => {
                search_empty.set_visible(false);
                empty_new_note.set_visible(true);
                self.browser_status.set_title("No notes yet");
                self.browser_status
                    .set_description(Some("Create a note to get started."));
                pages.set_visible_child_name("empty");
            }
            LoadState::Ready(notes) => {
                search_empty.set_visible(false);
                empty_new_note.set_visible(true);
                pages.set_visible_child_name("contents");
                for note in notes {
                    list.append(&browser_row(note));
                }
            }
            state => {
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

fn browser_row(note: &carver_sdk::NoteSummary) -> gtk::ListBoxRow {
    let row = gtk::ListBoxRow::new();
    row.set_widget_name(&format!("note:{}", note.id));
    row.add_css_class("note-card");
    let content = gtk::Box::new(gtk::Orientation::Vertical, 4);
    content.set_margin_start(12);
    content.set_margin_end(12);
    content.set_margin_top(10);
    content.set_margin_bottom(10);
    let title = gtk::Label::new(Some(&note.title));
    title.set_xalign(0.0);
    title.add_css_class("note-card-title");
    content.append(&title);
    if !note.excerpt.is_empty() {
        let excerpt = gtk::Label::new(Some(&note.excerpt));
        excerpt.set_xalign(0.0);
        excerpt.add_css_class("note-card-excerpt");
        excerpt.set_ellipsize(gtk::pango::EllipsizeMode::End);
        excerpt.set_single_line_mode(true);
        content.append(&excerpt);
    }
    row.set_child(Some(&content));
    row
}
