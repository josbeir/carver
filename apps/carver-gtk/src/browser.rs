//! Recent-note browser and responsive content composition.

use std::{borrow::Cow, cell::Cell, rc::Rc};

use carver_config::Config;
use carver_sdk::{Category, CategorySummary, NoteSummary};
use gtk::prelude::*;
use libadwaita as adw;
use time::{Duration, Month, OffsetDateTime, UtcOffset};

use crate::{
    dialogs::{
        IMPORT_NOTE_ACTION, NEW_NOTE_ACTION, category_color_css_class, category_icon_name,
        show_category_dialog, show_category_trash_confirmation,
    },
    editor::{EditorViewRefs, SourceSyntaxError, build_editor},
    mvu::{ActionMsg, AppDispatcher, AppMsg, BrowserMsg, EditorMsg, LoadState, NavigationMsg},
    sidebar::sidebar_toggle_button,
    trash::{TrashViewRefs, build_trash},
};

const MOUSE_BACK_BUTTON: u32 = 8;
const TOUCHPAD_BACK_SCROLL_THRESHOLD: f64 = 80.0;

#[derive(Clone, Copy, Debug, PartialEq)]
enum TouchpadBackGesture {
    Idle,
    Tracking(f64),
    Triggered,
}

impl TouchpadBackGesture {
    fn advance(self, delta_x: f64, delta_y: f64) -> Self {
        if matches!(self, Self::Triggered) || delta_x.abs() <= delta_y.abs() {
            return self;
        }
        let distance = match self {
            Self::Idle if delta_x > 0.0 => delta_x,
            Self::Idle => return Self::Idle,
            Self::Tracking(distance) => (distance + delta_x).max(0.0),
            Self::Triggered => return Self::Triggered,
        };
        if distance >= TOUCHPAD_BACK_SCROLL_THRESHOLD {
            Self::Triggered
        } else {
            Self::Tracking(distance)
        }
    }

    fn is_tracking(self) -> bool {
        matches!(self, Self::Tracking(_) | Self::Triggered)
    }
}

/// A relative calendar section used to group the recent-notes browser.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum NoteDateGroup {
    /// Notes updated on the current local calendar day.
    Today,
    /// Notes updated on the previous local calendar day.
    Yesterday,
    /// Notes updated earlier in the current Monday-to-Sunday week.
    ThisWeek,
    /// Notes updated earlier in the current calendar month.
    ThisMonth,
    /// Notes updated earlier in the current calendar year.
    EarlierThisYear,
    /// Notes updated in a previous calendar year.
    Year(i32),
}

impl NoteDateGroup {
    /// Returns the user-visible heading for this group.
    #[must_use]
    pub(crate) fn label(self) -> Cow<'static, str> {
        match self {
            Self::Today => Cow::Borrowed("Today"),
            Self::Yesterday => Cow::Borrowed("Yesterday"),
            Self::ThisWeek => Cow::Borrowed("This Week"),
            Self::ThisMonth => Cow::Borrowed("This Month"),
            Self::EarlierThisYear => Cow::Borrowed("Earlier This Year"),
            Self::Year(year) => Cow::Owned(year.to_string()),
        }
    }

    /// Returns a stable widget-name suffix for this group.
    #[must_use]
    pub(crate) fn identifier(self) -> Cow<'static, str> {
        match self {
            Self::Today => Cow::Borrowed("today"),
            Self::Yesterday => Cow::Borrowed("yesterday"),
            Self::ThisWeek => Cow::Borrowed("this-week"),
            Self::ThisMonth => Cow::Borrowed("this-month"),
            Self::EarlierThisYear => Cow::Borrowed("earlier-this-year"),
            Self::Year(year) => Cow::Owned(year.to_string()),
        }
    }
}

/// Widget references needed to render the browser portion of a window snapshot.
pub(crate) struct BrowserViewRefs {
    pub(crate) list: gtk::ListBox,
    pub(crate) pages: gtk::Stack,
    pub(crate) search_empty_card: gtk::Box,
    pub(crate) empty_new_note_button: gtk::Button,
    pub(crate) category_hero: gtk::Box,
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
    install_editor_back_navigation(dispatcher, &stack);
    Ok(ContentSurface {
        widget: stack.clone().upcast(),
        route_stack: stack,
        editor: editor_refs,
        browser: browser_refs,
        trash: trash_refs,
    })
}

/// Routes all conventional Back inputs through the editor's MVU close transition.
fn install_editor_back_navigation(dispatcher: &AppDispatcher, route_stack: &gtk::Stack) {
    install_editor_mouse_back_navigation(dispatcher, route_stack);
    install_editor_touchpad_back_navigation(dispatcher, route_stack);
}

fn install_editor_mouse_back_navigation(dispatcher: &AppDispatcher, route_stack: &gtk::Stack) {
    let back = gtk::EventControllerLegacy::new();
    back.set_name(Some("editor-mouse-back-controller"));
    // Capture the event before an embedded rich editor can consume it.
    back.set_propagation_phase(gtk::PropagationPhase::Capture);
    let dispatcher = dispatcher.clone();
    let route_stack_for_event = route_stack.clone();
    back.connect_event(move |_, event| {
        let is_mouse_back = event
            .downcast_ref::<gtk::gdk::ButtonEvent>()
            .is_some_and(|button| {
                button.event_type() == gtk::gdk::EventType::ButtonPress
                    && button.button() == MOUSE_BACK_BUTTON
            });
        if !is_mouse_back || !is_editor_route(&route_stack_for_event) {
            return glib::Propagation::Proceed;
        }
        let _ = dispatcher.dispatch(AppMsg::Editor(EditorMsg::BackRequested));
        glib::Propagation::Stop
    });
    route_stack.add_controller(back);
}

fn install_editor_touchpad_back_navigation(dispatcher: &AppDispatcher, route_stack: &gtk::Stack) {
    let back = gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::BOTH_AXES);
    back.set_name(Some("editor-touchpad-back-controller"));
    // Capture the scroll before a nested WebKit editor can claim a horizontal swipe.
    back.set_propagation_phase(gtk::PropagationPhase::Capture);
    let gesture = Rc::new(Cell::new(TouchpadBackGesture::Idle));
    let gesture_for_begin = Rc::clone(&gesture);
    back.connect_scroll_begin(move |_| gesture_for_begin.set(TouchpadBackGesture::Idle));
    let gesture_for_scroll = Rc::clone(&gesture);
    let dispatcher = dispatcher.clone();
    let route_stack_for_scroll = route_stack.clone();
    back.connect_scroll(move |controller, delta_x, delta_y| {
        if !is_editor_route(&route_stack_for_scroll) || !is_touchpad_surface_scroll(controller) {
            return glib::Propagation::Proceed;
        }
        let next = gesture_for_scroll.get().advance(delta_x, delta_y);
        let was_triggered = matches!(gesture_for_scroll.get(), TouchpadBackGesture::Triggered);
        gesture_for_scroll.set(next);
        if matches!(next, TouchpadBackGesture::Triggered) && !was_triggered {
            let _ = dispatcher.dispatch(AppMsg::Editor(EditorMsg::BackRequested));
        }
        if next.is_tracking() {
            glib::Propagation::Stop
        } else {
            glib::Propagation::Proceed
        }
    });
    route_stack.add_controller(back);
}

fn is_editor_route(route_stack: &gtk::Stack) -> bool {
    route_stack.visible_child_name().as_deref() == Some("editor")
}

fn is_touchpad_surface_scroll(controller: &gtk::EventControllerScroll) -> bool {
    controller.unit() == gtk::gdk::ScrollUnit::Surface
        && controller
            .current_event()
            .and_then(|event| event.device())
            .is_some_and(|device| device.source() == gtk::gdk::InputSource::Touchpad)
}

/// Builds the default recent-note and search view.
pub(crate) fn build_browser(
    dispatcher: &AppDispatcher,
    split_view: &adw::NavigationSplitView,
) -> (gtk::Widget, BrowserViewRefs) {
    let view = adw::ToolbarView::new();
    view.set_widget_name("browser-surface");
    let header = adw::HeaderBar::new();
    let new_note = gtk::Button::from_icon_name("document-new-symbolic");
    new_note.set_widget_name("new-note-button");
    new_note.set_tooltip_text(Some("New Note"));
    header.pack_end(&new_note);
    let import_note = gtk::Button::from_icon_name("document-open-symbolic");
    import_note.set_widget_name("import-note-button");
    import_note.set_tooltip_text(Some("Import note"));
    header.pack_end(&import_note);
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
    let category_hero = gtk::Box::new(gtk::Orientation::Vertical, 0);
    category_hero.set_widget_name("browser-category-hero");
    category_hero.add_css_class("category-hero");
    content.append(&category_hero);
    let search = gtk::SearchEntry::new();
    search.set_widget_name("note-search-entry");
    search.set_placeholder_text(Some("Search notes"));
    content.append(&search);
    let search_empty_card = gtk::Box::new(gtk::Orientation::Vertical, 4);
    search_empty_card.set_widget_name("browser-search-empty-card");
    search_empty_card.add_css_class("card");
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

    connect_browser_actions(
        dispatcher,
        &search,
        &new_note,
        &import_note,
        &empty_new_note,
        &list,
    );
    install_browser_shortcuts(&view);
    (
        view.upcast(),
        BrowserViewRefs {
            list,
            pages,
            search_empty_card,
            empty_new_note_button: empty_new_note,
            category_hero,
            status,
        },
    )
}

/// Captures overview shortcuts before child widgets consume them.
fn install_browser_shortcuts(view: &adw::ToolbarView) {
    let controller = gtk::EventControllerKey::new();
    controller.set_name(Some("browser-shortcuts"));
    controller.set_propagation_phase(gtk::PropagationPhase::Capture);
    let action_host = view.clone().upcast::<gtk::Widget>();
    let action_host_for_callback = action_host.clone();
    controller.connect_key_pressed(move |_, key, _, modifiers| {
        if !modifiers.contains(gtk::gdk::ModifierType::CONTROL_MASK)
            || modifiers.contains(gtk::gdk::ModifierType::SHIFT_MASK)
        {
            return glib::Propagation::Proceed;
        }
        let action = match key {
            gtk::gdk::Key::n => NEW_NOTE_ACTION,
            gtk::gdk::Key::o => IMPORT_NOTE_ACTION,
            _ => return glib::Propagation::Proceed,
        };
        let _ = action_host_for_callback.activate_action(action, None::<&glib::Variant>);
        glib::Propagation::Stop
    });
    action_host.add_controller(controller);
}

fn connect_browser_actions(
    dispatcher: &AppDispatcher,
    search: &gtk::SearchEntry,
    new_note: &gtk::Button,
    import_note: &gtk::Button,
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
    connect_import_note_action(import_note);
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

fn connect_import_note_action(button: &gtk::Button) {
    button.connect_clicked(|button| {
        let _ = button.activate_action(IMPORT_NOTE_ACTION, None::<&glib::Variant>);
    });
}

fn connect_new_note_action(dispatcher: &AppDispatcher, button: &gtk::Button) {
    let dispatcher = dispatcher.clone();
    button.connect_clicked(move |_| {
        let _ = dispatcher.dispatch(AppMsg::Navigation(NavigationMsg::CreateNote));
    });
}

/// Renders the browser's current-library or category hero from an immutable sidebar snapshot.
pub(crate) fn render_category_hero(
    hero: &gtk::Box,
    sidebar: &LoadState<Vec<CategorySummary>>,
    selected_category: Option<carver_sdk::CategoryId>,
    dispatcher: Option<&AppDispatcher>,
) {
    while let Some(child) = hero.first_child() {
        hero.remove(&child);
    }
    let LoadState::Ready(categories) = sidebar else {
        hero.set_visible(false);
        return;
    };
    hero.set_visible(true);
    let selected = selected_category.and_then(|category_id| {
        categories
            .iter()
            .find(|summary| summary.category.id == category_id)
    });
    let content = match selected {
        Some(summary) => category_hero(summary, dispatcher),
        None => all_notes_hero(categories),
    };
    hero.append(&content);
}

fn all_notes_hero(categories: &[CategorySummary]) -> gtk::Widget {
    let note_count = categories.iter().map(|summary| summary.note_count).sum();
    category_hero_content(
        "go-home-symbolic",
        "all-notes-icon",
        "All notes",
        &note_count_label(note_count),
        None,
    )
}

fn category_hero(summary: &CategorySummary, dispatcher: Option<&AppDispatcher>) -> gtk::Widget {
    let category = &summary.category;
    let color = category.appearance.color.resolved_for(category.id);
    let actions = dispatcher.map(|dispatcher| category_hero_actions(category, dispatcher));
    category_hero_content(
        category_icon_name(category.appearance.icon),
        category_color_css_class(color),
        &category.name,
        &note_count_label(summary.note_count),
        actions.as_ref(),
    )
}

fn category_hero_content(
    icon_name: &str,
    color_class: &str,
    title: &str,
    subtitle: &str,
    actions: Option<&gtk::Box>,
) -> gtk::Widget {
    let content = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    content.add_css_class("category-hero-content");
    let icon = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    icon.set_widget_name("browser-hero-icon");
    icon.add_css_class("category-icon-tile");
    icon.add_css_class("category-hero-icon");
    icon.add_css_class(color_class);
    icon.set_halign(gtk::Align::Fill);
    icon.set_valign(gtk::Align::Fill);
    icon.set_homogeneous(true);
    let image = gtk::Image::from_icon_name(icon_name);
    image.set_pixel_size(24);
    image.set_halign(gtk::Align::Center);
    image.set_valign(gtk::Align::Center);
    icon.append(&image);
    content.append(&icon);
    let text = gtk::Box::new(gtk::Orientation::Vertical, 2);
    text.set_hexpand(true);
    let title_label = gtk::Label::new(Some(title));
    title_label.set_widget_name("browser-hero-title");
    title_label.add_css_class("title-2");
    title_label.set_xalign(0.0);
    title_label.set_ellipsize(gtk::pango::EllipsizeMode::End);
    title_label.set_single_line_mode(true);
    text.append(&title_label);
    let subtitle_label = gtk::Label::new(Some(subtitle));
    subtitle_label.set_widget_name("browser-hero-subtitle");
    subtitle_label.add_css_class("dim-label");
    subtitle_label.set_xalign(0.0);
    text.append(&subtitle_label);
    content.append(&text);
    if let Some(actions) = actions {
        content.append(actions);
    }
    content.upcast()
}

fn category_hero_actions(category: &Category, dispatcher: &AppDispatcher) -> gtk::Box {
    let actions = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    actions.set_widget_name("browser-category-hero-actions");
    let edit = gtk::Button::from_icon_name("document-edit-symbolic");
    edit.set_widget_name("edit-selected-category-button");
    edit.set_tooltip_text(Some("Edit Category"));
    edit.add_css_class("flat");
    let dispatcher_for_edit = dispatcher.clone();
    let category_id = category.id;
    let category_name = category.name.clone();
    let appearance = category.appearance;
    edit.connect_clicked(move |button| {
        let parent = button
            .root()
            .and_then(|root| root.downcast::<gtk::Window>().ok());
        let dispatcher = dispatcher_for_edit.clone();
        show_category_dialog(
            parent.as_ref(),
            "Edit Category",
            &category_name,
            appearance,
            move |name, appearance| {
                let _ = dispatcher.dispatch(AppMsg::Action(ActionMsg::UpdateCategory {
                    category_id,
                    name,
                    appearance,
                }));
            },
        );
    });
    actions.append(&edit);
    let trash = gtk::Button::from_icon_name("user-trash-symbolic");
    trash.set_widget_name("trash-selected-category-button");
    trash.set_tooltip_text(Some("Move Category to Trash"));
    trash.add_css_class("flat");
    let dispatcher_for_trash = dispatcher.clone();
    let category_name = category.name.clone();
    trash.connect_clicked(move |button| {
        let parent = button
            .root()
            .and_then(|root| root.downcast::<gtk::Window>().ok());
        let dispatcher = dispatcher_for_trash.clone();
        show_category_trash_confirmation(parent.as_ref(), &category_name, move || {
            let _ = dispatcher.dispatch(AppMsg::Action(ActionMsg::TrashCategory(category_id)));
        });
    });
    actions.append(&trash);
    actions
}

fn note_count_label(note_count: usize) -> String {
    if note_count == 1 {
        "1 note".to_owned()
    } else {
        format!("{note_count} notes")
    }
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
    let excerpt_text = compact_note_excerpt(&note.title, &note.excerpt);
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

/// Returns the relative calendar group for a note updated at `updated_at`.
#[must_use]
pub(crate) fn note_date_group(updated_at: OffsetDateTime, now: OffsetDateTime) -> NoteDateGroup {
    note_date_group_for_days(local_day(updated_at), local_day(now))
}

fn note_date_group_for_days(updated_day: time::Date, today: time::Date) -> NoteDateGroup {
    if updated_day >= today {
        return NoteDateGroup::Today;
    }
    if updated_day == today - Duration::days(1) {
        return NoteDateGroup::Yesterday;
    }
    let week_start = today - Duration::days(i64::from(today.weekday().number_days_from_monday()));
    if updated_day >= week_start {
        return NoteDateGroup::ThisWeek;
    }
    if updated_day.year() == today.year() && updated_day.month() == today.month() {
        return NoteDateGroup::ThisMonth;
    }
    if updated_day.year() == today.year() {
        return NoteDateGroup::EarlierThisYear;
    }
    NoteDateGroup::Year(updated_day.year())
}

/// Returns the one-line excerpt used by every note card.
pub(crate) fn compact_note_excerpt(title: &str, excerpt: &str) -> String {
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
mod tests;
