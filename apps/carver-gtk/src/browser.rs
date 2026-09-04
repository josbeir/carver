//! Recent-note browser and responsive content composition.

use carver_config::Config;
use carver_sdk::NoteSummary;
use gtk::prelude::*;
use libadwaita as adw;
use time::{Duration, Month, OffsetDateTime, UtcOffset};

use crate::{
    editor::{EditorViewRefs, SourceSyntaxError, build_editor},
    mvu::{AppDispatcher, AppMsg, BrowserMsg, NavigationMsg},
    sidebar::sidebar_toggle_button,
    trash::{TrashViewRefs, build_trash},
};

/// Widget references needed to render the browser portion of a window snapshot.
pub(crate) struct BrowserViewRefs {
    pub(crate) list: gtk::ListBox,
    pub(crate) pages: gtk::Stack,
    pub(crate) search_empty_card: gtk::Box,
    pub(crate) empty_new_note_button: gtk::Button,
    pub(crate) title: adw::WindowTitle,
    pub(crate) status: adw::StatusPage,
}

/// The complete content surface and the view references it creates.
pub(crate) struct ContentSurface {
    pub(crate) widget: gtk::Widget,
    pub(crate) route_stack: gtk::Stack,
    pub(crate) editor: EditorViewRefs,
    pub(crate) browser: BrowserViewRefs,
    pub(crate) trash: TrashViewRefs,
}

/// Builds the browser, editor, and trash pages for the content pane.
pub(crate) fn build_content(
    dispatcher: &AppDispatcher,
    config: &Config,
    assets_dir: Option<&std::path::Path>,
    source_syntax_dir: &std::path::Path,
    split_view: &adw::NavigationSplitView,
    toast_overlay: &adw::ToastOverlay,
) -> Result<ContentSurface, SourceSyntaxError> {
    let stack = gtk::Stack::new();
    stack.set_widget_name("content-route-stack");
    stack.set_transition_type(gtk::StackTransitionType::SlideLeftRight);
    let (browser, browser_refs) = build_browser(dispatcher, split_view);
    stack.add_named(&browser, Some("browser"));
    let (editor, editor_refs) = build_editor(
        dispatcher,
        config,
        assets_dir,
        source_syntax_dir,
        toast_overlay,
        split_view,
    )?
    .into_parts();
    stack.add_named(&editor, Some("editor"));
    let (trash, trash_refs) = build_trash(dispatcher);
    stack.add_named(&trash, Some("trash"));
    stack.set_visible_child_name("browser");
    Ok(ContentSurface {
        widget: stack.clone().upcast(),
        route_stack: stack,
        editor: editor_refs,
        browser: browser_refs,
        trash: trash_refs,
    })
}

/// Builds the default recent-note and search view.
pub(crate) fn build_browser(
    dispatcher: &AppDispatcher,
    split_view: &adw::NavigationSplitView,
) -> (gtk::Widget, BrowserViewRefs) {
    let view = adw::ToolbarView::new();
    let header = adw::HeaderBar::new();
    let title = adw::WindowTitle::new("Home", "All recent notes");
    title.set_widget_name("browser-window-title");
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
    header.pack_start(&sidebar_toggle_button(
        split_view,
        "toggle-categories-button",
    ));
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

    connect_browser_actions(dispatcher, &search, &new_note, &empty_new_note, &list);
    (
        view.upcast(),
        BrowserViewRefs {
            list,
            pages,
            search_empty_card,
            empty_new_note_button: empty_new_note,
            title,
            status,
        },
    )
}

fn connect_browser_actions(
    dispatcher: &AppDispatcher,
    search: &gtk::SearchEntry,
    new_note: &gtk::Button,
    empty_new_note: &gtk::Button,
    list: &gtk::ListBox,
) {
    let dispatcher_for_search = dispatcher.clone();
    search.connect_search_changed(move |entry| {
        let _ = dispatcher_for_search.dispatch(AppMsg::Browser(BrowserMsg::SearchChanged(
            entry.text().to_string(),
        )));
    });
    connect_new_note_action(dispatcher, new_note);
    connect_new_note_action(dispatcher, empty_new_note);
    let dispatcher_for_row = dispatcher.clone();
    list.connect_row_activated(move |_list, row| {
        let widget_name = row.widget_name();
        let Some(raw_id) = widget_name.strip_prefix("note:") else {
            return;
        };
        let Ok(id) = uuid::Uuid::parse_str(raw_id) else {
            return;
        };
        let _ = dispatcher_for_row.dispatch(AppMsg::Navigation(NavigationMsg::OpenNote(
            carver_sdk::NoteId::from_uuid(id),
        )));
    });
}

fn connect_new_note_action(dispatcher: &AppDispatcher, button: &gtk::Button) {
    let dispatcher = dispatcher.clone();
    button.connect_clicked(move |_| {
        let _ = dispatcher.dispatch(AppMsg::Navigation(NavigationMsg::CreateNote));
    });
}

/// Builds the shared note-card details rendered from browser snapshots.
pub(crate) fn note_card_details(note: &NoteSummary, show_category: bool) -> gtk::Box {
    let details = gtk::Box::new(gtk::Orientation::Vertical, 4);
    details.set_hexpand(true);
    let title = gtk::Label::new(Some(&note.title));
    title.set_widget_name(&format!("note-title:{}", note.id));
    title.set_xalign(0.0);
    title.add_css_class("note-card-title");
    title.set_ellipsize(gtk::pango::EllipsizeMode::End);
    title.set_single_line_mode(true);
    details.append(&title);
    let excerpt_text = card_excerpt(&note.title, &note.excerpt);
    if !excerpt_text.is_empty() {
        let excerpt = gtk::Label::new(Some(&excerpt_text));
        excerpt.set_widget_name(&format!("note-excerpt:{}", note.id));
        excerpt.set_xalign(0.0);
        excerpt.set_ellipsize(gtk::pango::EllipsizeMode::End);
        excerpt.set_single_line_mode(true);
        excerpt.add_css_class("note-card-excerpt");
        details.append(&excerpt);
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
    details.append(&metadata);
    details
}

fn local_day(timestamp: OffsetDateTime) -> time::Date {
    timestamp
        .to_offset(UtcOffset::current_local_offset().unwrap_or(UtcOffset::UTC))
        .date()
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
