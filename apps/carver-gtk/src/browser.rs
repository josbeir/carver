//! Recent-note browser, search, and responsive content composition.

use std::rc::Rc;

use carver_sdk::{NoteId, NoteSummary};
use gtk::prelude::*;
use libadwaita as adw;
use time::{Duration, OffsetDateTime, UtcOffset};

use crate::{
    controller::AppState,
    editor::build_editor,
    sidebar::{refresh_sidebar, sidebar_toggle_button},
    trash::build_trash,
};

/// Builds the browser and editor stack for the content pane.
pub(crate) fn build_content(
    state: &Rc<AppState>,
    split_view: &adw::NavigationSplitView,
    toast_overlay: &adw::ToastOverlay,
) -> gtk::Widget {
    let stack = gtk::Stack::new();
    stack.set_transition_type(gtk::StackTransitionType::SlideLeftRight);
    let browser = build_browser(state, &stack, split_view);
    stack.add_named(&browser, Some("browser"));
    let editor = build_editor(state, &stack, toast_overlay, split_view);
    stack.add_named(&editor, Some("editor"));
    let trash = build_trash(state, &stack, toast_overlay);
    stack.add_named(&trash, Some("trash"));
    stack.set_visible_child_name("browser");
    stack.upcast()
}

/// Builds the default recent-note and search view.
pub(crate) fn build_browser(
    state: &Rc<AppState>,
    stack: &gtk::Stack,
    split_view: &adw::NavigationSplitView,
) -> gtk::Widget {
    let view = adw::ToolbarView::new();
    let header = adw::HeaderBar::new();
    let title = adw::WindowTitle::new("Home", "All recent notes");
    header.set_title_widget(Some(&title));
    let new_note = gtk::Button::from_icon_name("document-new-symbolic");
    new_note.set_widget_name("new-note-button");
    new_note.set_tooltip_text(Some("New Note"));
    let menu = gtk::gio::Menu::new();
    menu.append(Some("Preferences"), Some("win.preferences"));
    menu.append(Some("About Carver"), Some("win.about"));
    let app_menu = gtk::MenuButton::new();
    app_menu.set_widget_name("app-menu-button");
    app_menu.set_icon_name("open-menu-symbolic");
    app_menu.set_tooltip_text(Some("Main Menu"));
    app_menu.set_menu_model(Some(&menu));
    header.pack_end(&app_menu);
    header.pack_end(&new_note);
    let toggle_sidebar = sidebar_toggle_button(split_view, "toggle-categories-button");
    header.pack_start(&toggle_sidebar);
    view.add_top_bar(&header);

    let content = gtk::Box::new(gtk::Orientation::Vertical, 12);
    content.set_margin_start(18);
    content.set_margin_end(18);
    content.set_margin_top(18);
    content.set_margin_bottom(18);
    let search = gtk::SearchEntry::new();
    search.set_widget_name("note-search-entry");
    search.set_placeholder_text(Some("Search notes"));
    content.append(&search);
    let list = gtk::ListBox::new();
    list.set_widget_name("note-list");
    list.set_selection_mode(gtk::SelectionMode::Single);
    list.add_css_class("note-feed");
    let scroll = gtk::ScrolledWindow::new();
    scroll.set_vexpand(true);
    scroll.set_child(Some(&list));
    content.append(&scroll);
    let clamp = adw::Clamp::new();
    clamp.set_widget_name("browser-content-clamp");
    clamp.set_maximum_size(720);
    clamp.set_tightening_threshold(520);
    clamp.set_child(Some(&content));
    let pages = gtk::Stack::new();
    pages.set_widget_name("browser-content-pages");
    pages.add_named(&clamp, Some("contents"));
    let status = adw::StatusPage::builder()
        .title("No notes yet")
        .description("Create a note to get started.")
        .icon_name("document-new-symbolic")
        .build();
    status.set_widget_name("browser-empty-status");
    let empty_new_note = gtk::Button::with_label("New Note");
    empty_new_note.set_widget_name("browser-empty-new-note-button");
    empty_new_note.add_css_class("suggested-action");
    let empty_action = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    empty_action.set_halign(gtk::Align::Center);
    empty_action.append(&empty_new_note);
    status.set_child(Some(&empty_action));
    pages.add_named(&status, Some("empty"));
    view.set_content(Some(&pages));

    state.browser_list.replace(Some(list.clone()));
    state.browser_stack.replace(Some(stack.clone()));
    state.browser_title.replace(Some(title));
    state.browser_content_stack.replace(Some(pages));
    state.browser_status.replace(Some(status));
    state
        .browser_empty_new_note_button
        .replace(Some(empty_new_note.clone()));
    refresh_browser(state);
    connect_browser_actions(state, &search, &new_note, &empty_new_note, &list, stack);
    view.upcast()
}

fn connect_browser_actions(
    state: &Rc<AppState>,
    search: &gtk::SearchEntry,
    new_note: &gtk::Button,
    empty_new_note: &gtk::Button,
    list: &gtk::ListBox,
    stack: &gtk::Stack,
) {
    let state_for_search = Rc::clone(state);
    search.connect_search_changed(move |entry| {
        state_for_search
            .search_query
            .replace(entry.text().to_string());
        refresh_browser(&state_for_search);
    });
    connect_new_note_action(state, stack, new_note);
    connect_new_note_action(state, stack, empty_new_note);
    let state_for_row = Rc::clone(state);
    let stack_for_row = stack.clone();
    list.connect_row_activated(move |_list, row| {
        let widget_name = row.widget_name();
        let Some(raw_id) = widget_name.strip_prefix("note:") else {
            return;
        };
        let Ok(id) = uuid::Uuid::parse_str(raw_id) else {
            return;
        };
        let state = Rc::clone(&state_for_row);
        let stack = stack_for_row.clone();
        let client = state.client.clone();
        glib::spawn_future_local(async move {
            if let Ok(Some(note)) = client.note_async(NoteId::from_uuid(id)).await {
                state.current_note.replace(Some(note));
                stack.set_visible_child_name("editor");
            }
        });
    });
}

fn connect_new_note_action(state: &Rc<AppState>, stack: &gtk::Stack, button: &gtk::Button) {
    let state_for_new = Rc::clone(state);
    let stack_for_new = stack.clone();
    button.connect_clicked(move |_| {
        let state = Rc::clone(&state_for_new);
        let stack = stack_for_new.clone();
        let client = state.client.clone();
        glib::spawn_future_local(async move {
            let category_id = match state.selected_category.get() {
                Some(category_id) => Some(category_id),
                None => client
                    .categories_async()
                    .await
                    .ok()
                    .and_then(|categories| categories.first().map(|category| category.id)),
            };
            let Some(category_id) = category_id else {
                return;
            };
            if let Ok(note) = client.create_note_async(category_id).await {
                state.current_note.replace(Some(note));
                refresh_browser(&state);
                refresh_sidebar(&state);
                stack.set_visible_child_name("editor");
            }
        });
    });
}

/// Refreshes the browser widgets after a note or category action.
pub(crate) fn refresh_browser(state: &Rc<AppState>) {
    refresh_browser_title(state);
    let (list, stack, pages, status, empty_new_note) = {
        let Some(list) = state.browser_list.borrow().clone() else {
            return;
        };
        let Some(stack) = state.browser_stack.borrow().clone() else {
            return;
        };
        let Some(pages) = state.browser_content_stack.borrow().clone() else {
            return;
        };
        let Some(status) = state.browser_status.borrow().clone() else {
            return;
        };
        let Some(empty_new_note) = state.browser_empty_new_note_button.borrow().clone() else {
            return;
        };
        (list, stack, pages, status, empty_new_note)
    };
    refresh_note_list(&list, &pages, &status, &empty_new_note, state, &stack);
}

fn refresh_browser_title(state: &AppState) {
    let Some(title) = state.browser_title.borrow().clone() else {
        return;
    };
    let category_name = state.selected_category_name.borrow();
    if let Some(category_name) = category_name.as_deref() {
        title.set_title(category_name);
        title.set_subtitle("Recently edited");
    } else {
        title.set_title("Home");
        title.set_subtitle("All recent notes");
    }
}

fn refresh_note_list(
    list: &gtk::ListBox,
    pages: &gtk::Stack,
    status: &adw::StatusPage,
    empty_new_note: &gtk::Button,
    state: &Rc<AppState>,
    _stack: &gtk::Stack,
) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }
    let query = state.search_query.borrow().trim().to_owned();
    let search_is_active = !query.is_empty();
    let category_id = state.selected_category.get();
    let category_name = state.selected_category_name.borrow().clone();
    let generation = state.browser_generation.get().saturating_add(1);
    state.browser_generation.set(generation);
    let state = Rc::clone(state);
    let list = list.clone();
    let pages = pages.clone();
    let status = status.clone();
    let empty_new_note = empty_new_note.clone();
    let client = state.client.clone();
    glib::spawn_future_local(async move {
        let entries = if search_is_active {
            client
                .search_async(query, category_id, 200)
                .await
                .unwrap_or_default()
                .into_iter()
                .map(|hit| hit.note)
                .collect()
        } else {
            client
                .recent_notes_async(category_id, 200, 0)
                .await
                .unwrap_or_default()
        };
        if state.browser_generation.get() != generation {
            return;
        }
        populate_note_list(
            &list,
            &pages,
            &status,
            &empty_new_note,
            entries,
            search_is_active,
            category_name.as_deref(),
        );
    });
}

fn populate_note_list(
    list: &gtk::ListBox,
    pages: &gtk::Stack,
    status: &adw::StatusPage,
    empty_new_note: &gtk::Button,
    entries: Vec<NoteSummary>,
    search_is_active: bool,
    category_name: Option<&str>,
) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }
    if entries.is_empty() {
        configure_empty_state(status, empty_new_note, search_is_active, category_name);
        pages.set_visible_child_name("empty");
        return;
    }
    pages.set_visible_child_name("contents");
    let mut previous_day = None;
    for note in entries {
        let day = local_day(note.updated_at);
        if !search_is_active && previous_day != Some(day) {
            let header = gtk::ListBoxRow::new();
            header.set_selectable(false);
            header.add_css_class("date-heading");
            let label = gtk::Label::new(Some(&day_label(day)));
            label.set_xalign(0.0);
            label.add_css_class("date-heading-label");
            header.set_child(Some(&label));
            list.append(&header);
            previous_day = Some(day);
        }
        let row = gtk::ListBoxRow::new();
        row.set_widget_name(&format!("note:{}", note.id));
        row.add_css_class("note-card");
        let box_ = gtk::Box::new(gtk::Orientation::Vertical, 4);
        box_.set_margin_start(12);
        box_.set_margin_end(12);
        box_.set_margin_top(10);
        box_.set_margin_bottom(10);
        let title = gtk::Label::new(Some(&note.title));
        title.set_widget_name(&format!("note-title:{}", note.id));
        title.set_xalign(0.0);
        title.add_css_class("note-card-title");
        title.set_ellipsize(gtk::pango::EllipsizeMode::End);
        title.set_single_line_mode(true);
        box_.append(&title);
        let excerpt = gtk::Label::new(Some(&note.excerpt));
        excerpt.set_xalign(0.0);
        excerpt.set_ellipsize(gtk::pango::EllipsizeMode::End);
        excerpt.set_single_line_mode(true);
        excerpt.add_css_class("note-card-excerpt");
        box_.append(&excerpt);
        row.set_child(Some(&box_));
        list.append(&row);
    }
}

fn configure_empty_state(
    status: &adw::StatusPage,
    new_note: &gtk::Button,
    search_is_active: bool,
    category_name: Option<&str>,
) {
    if search_is_active {
        status.set_title("No matching notes");
        status.set_description(Some("Try a different search term."));
        status.set_icon_name(Some("edit-find-symbolic"));
        new_note.set_visible(false);
        return;
    }

    let title = category_name.map_or_else(
        || "No notes yet".to_owned(),
        |category_name| format!("No notes in {category_name}"),
    );
    status.set_title(&title);
    status.set_description(Some("Create a note to get started."));
    status.set_icon_name(Some("document-new-symbolic"));
    new_note.set_visible(true);
}

fn local_day(timestamp: OffsetDateTime) -> time::Date {
    timestamp
        .to_offset(UtcOffset::current_local_offset().unwrap_or(UtcOffset::UTC))
        .date()
}

fn day_label(day: time::Date) -> String {
    let today = OffsetDateTime::now_utc()
        .to_offset(UtcOffset::current_local_offset().unwrap_or(UtcOffset::UTC))
        .date();
    if day == today {
        return "Today".to_owned();
    }
    if day == today - Duration::days(1) {
        return "Yesterday".to_owned();
    }
    day.to_string()
}
