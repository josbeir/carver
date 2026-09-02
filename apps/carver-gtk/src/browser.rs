//! Recent-note browser, search, and responsive content composition.

use std::rc::Rc;

use carver_sdk::{NoteId, NoteSummary};
use gtk::prelude::*;
use libadwaita as adw;
use time::{Duration, OffsetDateTime, UtcOffset};

use crate::{
    controller::{AppState, create_note_for_active_category, open_note},
    editor::build_editor,
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
    let editor = build_editor(state, &stack, toast_overlay);
    stack.add_named(&editor, Some("editor"));
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
    header.pack_end(&new_note);
    let menu = gtk::gio::Menu::new();
    menu.append(Some("Preferences"), Some("win.preferences"));
    menu.append(Some("About Carver"), Some("win.about"));
    let app_menu = gtk::MenuButton::new();
    app_menu.set_widget_name("app-menu-button");
    app_menu.set_icon_name("open-menu-symbolic");
    app_menu.set_tooltip_text(Some("Main Menu"));
    app_menu.set_menu_model(Some(&menu));
    header.pack_end(&app_menu);
    let toggle_sidebar = gtk::ToggleButton::new();
    toggle_sidebar.set_icon_name("sidebar-show-symbolic");
    toggle_sidebar.set_widget_name("toggle-categories-button");
    toggle_sidebar.set_tooltip_text(Some("Hide Categories"));
    toggle_sidebar.set_active(!split_view.is_collapsed());
    let split = split_view.clone();
    toggle_sidebar.connect_toggled(move |button| {
        if button.is_active() {
            split.set_collapsed(false);
        } else {
            split.set_collapsed(true);
            split.set_show_content(true);
        }
    });
    let toggle_for_state = toggle_sidebar.clone();
    split_view.connect_collapsed_notify(move |split| {
        if split.is_collapsed() {
            toggle_for_state.set_active(false);
            toggle_for_state.set_tooltip_text(Some("Show Categories"));
        } else {
            toggle_for_state.set_active(true);
            toggle_for_state.set_tooltip_text(Some("Hide Categories"));
        }
    });
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
    view.set_content(Some(&clamp));

    state.browser_list.replace(Some(list.clone()));
    state.browser_stack.replace(Some(stack.clone()));
    state.browser_title.replace(Some(title));
    refresh_browser(state);
    let state_for_search = Rc::clone(state);
    search.connect_search_changed(move |entry| {
        state_for_search
            .search_query
            .replace(entry.text().to_string());
        refresh_browser(&state_for_search);
    });
    let state_for_new = Rc::clone(state);
    let stack_for_new = stack.clone();
    new_note.connect_clicked(move |_| {
        if let Ok(Some(_note)) = create_note_for_active_category(&state_for_new) {
            refresh_browser(&state_for_new);
            stack_for_new.set_visible_child_name("editor");
        }
    });
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
        if let Ok(Some(_note)) = open_note(&state_for_row, NoteId::from_uuid(id)) {
            stack_for_row.set_visible_child_name("editor");
        }
    });
    view.upcast()
}

/// Refreshes the browser widgets after a note or category action.
pub(crate) fn refresh_browser(state: &AppState) {
    refresh_browser_title(state);
    let (list, stack) = {
        let Some(list) = state.browser_list.borrow().clone() else {
            return;
        };
        let Some(stack) = state.browser_stack.borrow().clone() else {
            return;
        };
        (list, stack)
    };
    refresh_note_list(&list, state, &stack);
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

fn refresh_note_list(list: &gtk::ListBox, state: &AppState, _stack: &gtk::Stack) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }
    let search_query = state.search_query.borrow();
    let search_is_active = !search_query.trim().is_empty();
    let entries: Vec<NoteSummary> = if search_is_active {
        state
            .client
            .search(&search_query, state.selected_category.get(), 200)
            .unwrap_or_default()
            .into_iter()
            .map(|hit| hit.note)
            .collect()
    } else {
        state
            .client
            .recent_notes(state.selected_category.get(), 200, 0)
            .unwrap_or_default()
    };
    drop(search_query);
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
