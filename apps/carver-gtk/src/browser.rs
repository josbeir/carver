//! Recent-note browser, search, and responsive content composition.

use std::rc::Rc;

use carver_sdk::{NoteId, NoteSummary};
use gtk::prelude::*;
use libadwaita as adw;
use time::{Duration, Month, OffsetDateTime, UtcOffset};

use crate::{
    controller::AppState,
    editor::build_editor,
    mvu::{AppMsg, BrowserMsg},
    note_move::show_move_note_dialog,
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
    let browser = build_browser(state, &stack, split_view, toast_overlay);
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
    toast_overlay: &adw::ToastOverlay,
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
    let search_empty_card = gtk::Box::new(gtk::Orientation::Vertical, 4);
    search_empty_card.set_widget_name("browser-search-empty-card");
    search_empty_card.add_css_class("search-empty-card");
    search_empty_card.set_visible(false);
    let search_empty_title = gtk::Label::new(Some("No matching notes"));
    search_empty_title.set_xalign(0.0);
    search_empty_title.add_css_class("search-empty-card-title");
    let search_empty_description = gtk::Label::new(Some("Try a different search term."));
    search_empty_description.set_xalign(0.0);
    search_empty_description.add_css_class("dim-label");
    search_empty_card.append(&search_empty_title);
    search_empty_card.append(&search_empty_description);
    content.append(&search_empty_card);
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
        .browser_search_empty_card
        .replace(Some(search_empty_card));
    state
        .browser_empty_new_note_button
        .replace(Some(empty_new_note.clone()));
    state
        .browser_toast_overlay
        .replace(Some(toast_overlay.clone()));
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
        if state_for_search.is_mvu_rendering() {
            return;
        }
        if state_for_search.dispatch_mvu(AppMsg::Browser(BrowserMsg::SearchChanged(
            entry.text().to_string(),
        ))) {
            return;
        }
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
    if state.dispatch_mvu(AppMsg::Browser(BrowserMsg::Reload)) {
        return;
    }
    refresh_browser_title(state);
    let (list, stack, pages, status, search_empty_card, empty_new_note, toast_overlay) = {
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
        let Some(search_empty_card) = state.browser_search_empty_card.borrow().clone() else {
            return;
        };
        let Some(empty_new_note) = state.browser_empty_new_note_button.borrow().clone() else {
            return;
        };
        let Some(toast_overlay) = state.browser_toast_overlay.borrow().clone() else {
            return;
        };
        (
            list,
            stack,
            pages,
            status,
            search_empty_card,
            empty_new_note,
            toast_overlay,
        )
    };
    refresh_note_list(
        &list,
        &pages,
        &status,
        &search_empty_card,
        &empty_new_note,
        &toast_overlay,
        state,
        &stack,
    );
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

#[expect(
    clippy::too_many_arguments,
    reason = "browser refresh keeps its established page widgets and snapshot data explicit"
)]
fn refresh_note_list(
    list: &gtk::ListBox,
    pages: &gtk::Stack,
    status: &adw::StatusPage,
    search_empty_card: &gtk::Box,
    empty_new_note: &gtk::Button,
    toast_overlay: &adw::ToastOverlay,
    state: &Rc<AppState>,
    _stack: &gtk::Stack,
) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }
    search_empty_card.set_visible(false);
    let query = state.search_query.borrow().trim().to_owned();
    let search_is_active = !query.is_empty();
    let category_id = state.selected_category.get();
    let show_category = category_id.is_none();
    let category_name = state.selected_category_name.borrow().clone();
    let generation = state.browser_generation.get().saturating_add(1);
    state.browser_generation.set(generation);
    let state = Rc::clone(state);
    let list = list.clone();
    let pages = pages.clone();
    let status = status.clone();
    let search_empty_card = search_empty_card.clone();
    let empty_new_note = empty_new_note.clone();
    let toast_overlay = toast_overlay.clone();
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
            &search_empty_card,
            &empty_new_note,
            &toast_overlay,
            &state,
            entries,
            search_is_active,
            show_category,
            category_name.as_deref(),
        );
    });
}

#[expect(
    clippy::too_many_arguments,
    reason = "browser refresh keeps its established page widgets and snapshot data explicit"
)]
fn populate_note_list(
    list: &gtk::ListBox,
    pages: &gtk::Stack,
    status: &adw::StatusPage,
    search_empty_card: &gtk::Box,
    empty_new_note: &gtk::Button,
    toast_overlay: &adw::ToastOverlay,
    state: &Rc<AppState>,
    entries: Vec<NoteSummary>,
    search_is_active: bool,
    show_category: bool,
    category_name: Option<&str>,
) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }
    if entries.is_empty() {
        if search_is_active {
            search_empty_card.set_visible(true);
            pages.set_visible_child_name("contents");
            return;
        }
        configure_empty_state(status, empty_new_note, category_name);
        pages.set_visible_child_name("empty");
        return;
    }
    search_empty_card.set_visible(false);
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
        let content = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        content.set_margin_start(12);
        content.set_margin_end(8);
        content.set_margin_top(10);
        content.set_margin_bottom(10);
        let box_ = gtk::Box::new(gtk::Orientation::Vertical, 4);
        box_.set_hexpand(true);
        let title = gtk::Label::new(Some(&note.title));
        title.set_widget_name(&format!("note-title:{}", note.id));
        title.set_xalign(0.0);
        title.add_css_class("note-card-title");
        title.set_ellipsize(gtk::pango::EllipsizeMode::End);
        title.set_single_line_mode(true);
        box_.append(&title);
        let excerpt_text = card_excerpt(&note.title, &note.excerpt);
        let excerpt = gtk::Label::new(Some(&excerpt_text));
        excerpt.set_widget_name(&format!("note-excerpt:{}", note.id));
        excerpt.set_xalign(0.0);
        excerpt.set_ellipsize(gtk::pango::EllipsizeMode::End);
        excerpt.set_single_line_mode(true);
        excerpt.add_css_class("note-card-excerpt");
        if !excerpt_text.is_empty() {
            box_.append(&excerpt);
        }
        let metadata = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        metadata.set_margin_top(8);
        metadata.add_css_class("note-card-metadata");
        if show_category {
            let category = gtk::Label::new(Some(&note.category_name));
            category.set_widget_name(&format!("note-category:{}", note.id));
            category.add_css_class("note-category-pill");
            metadata.append(&category);
        }
        let updated = gtk::Label::new(Some(&format!(
            "Updated {}",
            relative_update_time(note.updated_at, OffsetDateTime::now_utc())
        )));
        updated.set_widget_name(&format!("note-updated:{}", note.id));
        updated.add_css_class("note-card-updated");
        metadata.append(&updated);
        box_.append(&metadata);
        content.append(&box_);
        content.append(&note_menu(state, &note, toast_overlay));
        row.set_child(Some(&content));
        list.append(&row);
    }
}

fn note_menu(
    state: &Rc<AppState>,
    note: &NoteSummary,
    toast_overlay: &adw::ToastOverlay,
) -> gtk::MenuButton {
    let menu = gtk::MenuButton::new();
    menu.set_widget_name(&format!("note-menu:{}", note.id));
    menu.set_icon_name("view-more-symbolic");
    menu.set_tooltip_text(Some("Note actions"));
    menu.add_css_class("flat");
    let popover = gtk::Popover::new();
    let actions = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let move_button = overflow_menu_action("Move to Category…");
    let state_for_move = Rc::clone(state);
    let toast_for_move = toast_overlay.clone();
    let note_for_move = note.clone();
    let popover_for_move = popover.clone();
    move_button.connect_clicked(move |button| {
        popover_for_move.popdown();
        let parent = button
            .root()
            .and_then(|root| root.downcast::<gtk::Window>().ok());
        show_move_note_dialog(
            &state_for_move,
            note_for_move.id,
            note_for_move.category_id,
            &note_for_move.title,
            &toast_for_move,
            parent.as_ref(),
        );
    });
    actions.append(&move_button);
    let trash_button = overflow_menu_action("Move to Trash");
    trash_button.add_css_class("destructive-action");
    let state_for_trash = Rc::clone(state);
    let toast_for_trash = toast_overlay.clone();
    let note_for_trash = note.clone();
    let popover_for_trash = popover.clone();
    trash_button.connect_clicked(move |_| {
        popover_for_trash.popdown();
        let state = Rc::clone(&state_for_trash);
        let toast = toast_for_trash.clone();
        let client = state.client.clone();
        glib::spawn_future_local(async move {
            match client.trash_note_async(note_for_trash.id).await {
                Ok(()) => {
                    refresh_browser(&state);
                    refresh_sidebar(&state);
                    toast.add_toast(adw::Toast::new("Moved note to Trash"));
                }
                Err(error) => toast.add_toast(adw::Toast::new(&format!(
                    "Could not move note to Trash: {error}"
                ))),
            }
        });
    });
    actions.append(&trash_button);
    popover.set_child(Some(&actions));
    menu.set_popover(Some(&popover));
    menu
}

fn overflow_menu_action(label: &str) -> gtk::Button {
    let button = gtk::Button::new();
    button.add_css_class("flat");
    let label = gtk::Label::new(Some(label));
    label.set_xalign(0.0);
    label.set_margin_start(8);
    label.set_margin_end(8);
    button.set_child(Some(&label));
    button
}

fn configure_empty_state(
    status: &adw::StatusPage,
    new_note: &gtk::Button,
    category_name: Option<&str>,
) {
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

fn card_excerpt(title: &str, excerpt: &str) -> String {
    let excerpt = excerpt.split_whitespace().collect::<Vec<_>>().join(" ");
    excerpt
        .strip_prefix(title)
        .and_then(|remaining| remaining.strip_prefix(' '))
        .unwrap_or(&excerpt)
        .to_owned()
}

fn relative_update_time(updated_at: OffsetDateTime, now: OffsetDateTime) -> String {
    let elapsed_seconds = (now - updated_at).whole_seconds().max(0);
    if elapsed_seconds < 60 {
        return elapsed_label(elapsed_seconds, "second");
    }
    let elapsed_minutes = elapsed_seconds / 60;
    if elapsed_minutes < 60 {
        return elapsed_label(elapsed_minutes, "minute");
    }
    let elapsed_hours = elapsed_minutes / 60;
    if elapsed_hours < 24 {
        return elapsed_label(elapsed_hours, "hour");
    }

    let day = local_day(updated_at);
    let today = local_day(now);
    if day == today - Duration::days(1) {
        return String::from("Yesterday");
    }
    let date = format!("{} {}", month_name(day.month()), day.day());
    if day.year() == today.year() {
        date
    } else {
        format!("{date}, {}", day.year())
    }
}

fn elapsed_label(amount: i64, unit: &str) -> String {
    let suffix = if amount == 1 { "" } else { "s" };
    format!("{amount} {unit}{suffix} ago")
}

const fn month_name(month: Month) -> &'static str {
    match month {
        Month::January => "Jan",
        Month::February => "Feb",
        Month::March => "Mar",
        Month::April => "Apr",
        Month::May => "May",
        Month::June => "Jun",
        Month::July => "Jul",
        Month::August => "Aug",
        Month::September => "Sep",
        Month::October => "Oct",
        Month::November => "Nov",
        Month::December => "Dec",
    }
}

#[cfg(test)]
mod tests {
    use time::macros::datetime;

    use super::{card_excerpt, relative_update_time};

    #[test]
    fn card_excerpt_collapses_whitespace_and_omits_a_repeated_title() {
        assert_eq!(
            card_excerpt("Heading 1", "Heading 1\n\n  Relevant   body text"),
            "Relevant body text"
        );
    }

    #[test]
    fn relative_update_time_reports_seconds_before_a_minute() {
        assert_eq!(
            relative_update_time(
                datetime!(2026-09-03 12:00:00 UTC),
                datetime!(2026-09-03 12:00:30 UTC)
            ),
            "30 seconds ago"
        );
    }

    #[test]
    fn relative_update_time_uses_yesterday_for_the_previous_day() {
        assert_eq!(
            relative_update_time(
                datetime!(2026-09-02 08:00:00 UTC),
                datetime!(2026-09-03 12:00:00 UTC)
            ),
            "Yesterday"
        );
    }
}
