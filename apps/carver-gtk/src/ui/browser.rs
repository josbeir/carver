//! Recent-note browser and responsive content composition.

use std::{
    borrow::Cow,
    cell::{Cell, RefCell},
    rc::Rc,
};

use carver_config::Config;
use carver_sdk::{Category, CategoryColor, CategorySummary, NoteSummary};
use gtk::prelude::*;
use libadwaita as adw;
use time::{Duration, OffsetDateTime, UtcOffset, macros::format_description};

use super::{
    dialogs::{
        IMPORT_NOTE_ACTION, NEW_NOTE_ACTION, category_color_css_class, category_icon_name,
        show_category_dialog, show_category_trash_confirmation, show_move_note_dialog,
    },
    editor::{EditorViewRefs, SourceSyntaxError, build_editor},
    sidebar::{CompactNavigation, sidebar_toggle_button},
    trash::{TrashViewRefs, build_trash},
};
use crate::mvu::{
    ActionMsg, AppDispatcher, AppMsg, BrowserMsg, EditorMsg, LoadState, NavigationMsg,
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct BrowserFeedContext {
    pub(crate) show_category: bool,
    pub(crate) sidebar: LoadState<Vec<CategorySummary>>,
    pub(crate) selected_category: Option<carver_sdk::CategoryId>,
    pub(crate) favorites: Vec<(carver_sdk::NoteId, carver_sdk::Revision)>,
}

#[derive(Clone)]
pub(crate) enum BrowserFeedItem {
    Hero,
    Favorites(Vec<NoteSummary>),
    SearchEmpty,
    CategoryEmpty,
    Heading(NoteDateGroup),
    Note(NoteSummary),
    LoadMore { label: String, sensitive: bool },
}

/// Widget references needed to render the browser portion of a window snapshot.
pub(crate) struct BrowserViewRefs {
    pub(crate) list: gtk::ListView,
    pub(crate) feed_store: gtk::gio::ListStore,
    pub(crate) feed_context: Rc<RefCell<BrowserFeedContext>>,
    pub(crate) pages: gtk::Stack,
    pub(crate) search_bar: gtk::SearchBar,
    pub(crate) search_entry: gtk::SearchEntry,
    pub(crate) search_toggle: gtk::ToggleButton,
    pub(crate) empty_new_note_button: gtk::Button,
    pub(crate) status: adw::StatusPage,
}

/// The complete content surface and the view references it creates.
pub(crate) struct ContentSurface {
    pub(crate) widget: gtk::Widget,
    pub(crate) route_stack: gtk::Stack,
    pub(crate) editor: EditorViewRefs,
    pub(crate) browser: BrowserViewRefs,
    pub(crate) trash: TrashViewRefs,
    pub(crate) base: crate::ui::bases::BaseViewRefs,
}

/// Builds the browser, editor, and trash pages for the content pane.
pub(crate) fn build_content(
    dispatcher: &AppDispatcher,
    config: &Config,
    assets_dir: Option<&std::path::Path>,
    source_syntax_dir: &std::path::Path,
    split_view: &adw::NavigationSplitView,
    compact_navigation: &CompactNavigation,
    toast_overlay: &adw::ToastOverlay,
) -> Result<ContentSurface, SourceSyntaxError> {
    let stack = gtk::Stack::new();
    stack.set_widget_name("content-route-stack");
    stack.set_hhomogeneous(false);
    stack.set_transition_type(gtk::StackTransitionType::SlideLeftRight);
    let (browser, browser_refs) = build_browser(dispatcher, split_view, compact_navigation);
    stack.add_named(&browser, Some("browser"));
    let (base, base_refs) =
        crate::ui::bases::build_base(dispatcher, split_view, compact_navigation);
    stack.add_named(&base, Some("base"));
    let (editor, editor_refs) = build_editor(
        dispatcher,
        config,
        assets_dir,
        source_syntax_dir,
        toast_overlay,
        split_view,
        compact_navigation,
    )?
    .into_parts();
    stack.add_named(&editor, Some("editor"));
    let (trash, trash_refs) = build_trash(dispatcher);
    stack.add_named(&trash, Some("trash"));
    stack.set_visible_child_name("browser");
    install_page_back_navigation(dispatcher, &stack, &base_refs.scroll);
    Ok(ContentSurface {
        widget: stack.clone().upcast(),
        route_stack: stack,
        editor: editor_refs,
        browser: browser_refs,
        trash: trash_refs,
        base: base_refs,
    })
}

/// Routes conventional Back inputs through the active editor or base transition.
fn install_page_back_navigation(
    dispatcher: &AppDispatcher,
    route_stack: &gtk::Stack,
    base_scroll: &gtk::ScrolledWindow,
) {
    install_mouse_back_navigation(dispatcher, route_stack);
    install_touchpad_back_navigation(dispatcher, route_stack, base_scroll);
}

fn install_mouse_back_navigation(dispatcher: &AppDispatcher, route_stack: &gtk::Stack) {
    let back = gtk::EventControllerLegacy::new();
    back.set_name(Some("page-mouse-back-controller"));
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
        if !is_mouse_back || !is_back_route(&route_stack_for_event) {
            return glib::Propagation::Proceed;
        }
        dispatch_route_back(&dispatcher, &route_stack_for_event);
        glib::Propagation::Stop
    });
    route_stack.add_controller(back);
}

fn install_touchpad_back_navigation(
    dispatcher: &AppDispatcher,
    route_stack: &gtk::Stack,
    base_scroll: &gtk::ScrolledWindow,
) {
    let back = gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::BOTH_AXES);
    back.set_name(Some("page-touchpad-back-controller"));
    // Capture the scroll before a nested WebKit editor can claim a horizontal swipe.
    back.set_propagation_phase(gtk::PropagationPhase::Capture);
    let gesture = Rc::new(Cell::new(TouchpadBackGesture::Idle));
    let gesture_for_begin = Rc::clone(&gesture);
    back.connect_scroll_begin(move |_| gesture_for_begin.set(TouchpadBackGesture::Idle));
    let gesture_for_scroll = Rc::clone(&gesture);
    let dispatcher = dispatcher.clone();
    let route_stack_for_scroll = route_stack.clone();
    let base_scroll = base_scroll.clone();
    back.connect_scroll(move |controller, delta_x, delta_y| {
        if !is_back_route(&route_stack_for_scroll) || !is_touchpad_surface_scroll(controller) {
            return glib::Propagation::Proceed;
        }
        if route_stack_for_scroll.visible_child_name().as_deref() == Some("base")
            && base_can_scroll_right(&base_scroll)
        {
            gesture_for_scroll.set(TouchpadBackGesture::Idle);
            return glib::Propagation::Proceed;
        }
        let next = gesture_for_scroll.get().advance(delta_x, delta_y);
        let was_triggered = matches!(gesture_for_scroll.get(), TouchpadBackGesture::Triggered);
        gesture_for_scroll.set(next);
        if matches!(next, TouchpadBackGesture::Triggered) && !was_triggered {
            dispatch_route_back(&dispatcher, &route_stack_for_scroll);
        }
        if next.is_tracking() {
            glib::Propagation::Stop
        } else {
            glib::Propagation::Proceed
        }
    });
    route_stack.add_controller(back);
}

fn base_can_scroll_right(scroll: &gtk::ScrolledWindow) -> bool {
    let adjustment = scroll.hadjustment();
    adjustment_can_scroll_right(
        adjustment.value(),
        adjustment.page_size(),
        adjustment.upper(),
    )
}

fn adjustment_can_scroll_right(value: f64, page_size: f64, upper: f64) -> bool {
    value + page_size < upper
}

fn is_editor_route(route_stack: &gtk::Stack) -> bool {
    route_stack.visible_child_name().as_deref() == Some("editor")
}

fn is_back_route(route_stack: &gtk::Stack) -> bool {
    matches!(
        route_stack.visible_child_name().as_deref(),
        Some("editor" | "base")
    )
}

fn dispatch_route_back(dispatcher: &AppDispatcher, route_stack: &gtk::Stack) {
    let message = if is_editor_route(route_stack) {
        AppMsg::Editor(EditorMsg::BackRequested)
    } else {
        AppMsg::Navigation(NavigationMsg::ShowBrowser)
    };
    let _ = dispatcher.dispatch(message);
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
    compact_navigation: &CompactNavigation,
) -> (gtk::Widget, BrowserViewRefs) {
    let view = adw::ToolbarView::new();
    view.set_widget_name("browser-surface");
    let header = adw::HeaderBar::new();
    let new_note = gtk::Button::from_icon_name("document-new-symbolic");
    new_note.set_widget_name("new-note-button");
    new_note.set_tooltip_text(Some("New Note"));
    header.pack_end(&browser_menu_button());
    header.pack_end(&new_note);
    header.pack_start(&sidebar_toggle_button(
        split_view,
        compact_navigation,
        "toggle-categories-button",
    ));
    let (search_bar, search, search_toggle) = build_note_search_controls();
    header.pack_start(&search_toggle);
    view.add_top_bar(&header);
    view.add_top_bar(&search_bar);

    let feed_store = gtk::gio::ListStore::new::<glib::BoxedAnyObject>();
    let feed_context = Rc::new(RefCell::new(BrowserFeedContext {
        show_category: true,
        sidebar: LoadState::Idle,
        selected_category: None,
        favorites: Vec::new(),
    }));
    let selection = gtk::SingleSelection::new(Some(feed_store.clone()));
    selection.set_autoselect(false);
    selection.set_can_unselect(true);
    let list = gtk::ListView::new(
        Some(selection),
        Some(browser_feed_factory(dispatcher, Rc::clone(&feed_context))),
    );
    list.set_widget_name("note-list");
    list.add_css_class("note-feed");
    list.set_single_click_activate(true);
    // GtkScrolledWindow owns the viewport and adjustment. ClampScrollable only
    // constrains the direct virtual child and forwards that adjustment to it.
    let clamp = adw::ClampScrollable::new();
    clamp.set_widget_name("browser-content-clamp");
    clamp.set_maximum_size(720);
    clamp.set_tightening_threshold(520);
    clamp.set_child(Some(&list));
    let scroll = gtk::ScrolledWindow::new();
    scroll.set_widget_name("browser-content-scroll");
    scroll.set_vexpand(true);
    scroll.set_child(Some(&clamp));
    let pages = gtk::Stack::new();
    pages.set_widget_name("browser-content-pages");
    pages.add_named(&scroll, Some("contents"));
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

    let dispatcher_for_feed = dispatcher.clone();
    list.connect_activate(move |view, position| {
        let Some(item) = view
            .model()
            .and_then(|model| model.item(position))
            .and_downcast::<glib::BoxedAnyObject>()
        else {
            return;
        };
        let note_id = {
            let item = item.borrow::<BrowserFeedItem>();
            match &*item {
                BrowserFeedItem::Note(note) => note.id,
                _ => return,
            }
        };
        let _ = dispatcher_for_feed.dispatch(AppMsg::Navigation(NavigationMsg::OpenNote(note_id)));
    });

    let references = BrowserViewRefs {
        list,
        feed_store,
        feed_context,
        pages,
        search_bar,
        search_entry: search,
        search_toggle,
        empty_new_note_button: empty_new_note,
        status,
    };
    connect_browser_actions(dispatcher, &references, &new_note);
    connect_browser_paging(dispatcher, &references);
    install_browser_shortcuts(&view, dispatcher);
    (view.upcast(), references)
}

fn connect_browser_paging(dispatcher: &AppDispatcher, references: &BrowserViewRefs) {
    let dispatcher_for_scroll = dispatcher.clone();
    let Some(adjustment) = references.list.vadjustment() else {
        return;
    };
    adjustment.connect_value_changed(move |adjustment| {
        let remaining = adjustment.upper() - adjustment.page_size() - adjustment.value();
        if remaining <= adjustment.page_size() * 2.0 {
            let _ = dispatcher_for_scroll.dispatch(AppMsg::Browser(BrowserMsg::LoadMore));
        }
    });
}

fn build_favorites_section() -> (gtk::Box, gtk::ListBox) {
    let section = gtk::Box::new(gtk::Orientation::Vertical, 8);
    section.set_widget_name("favorites-section");
    section.set_visible(false);
    let list = gtk::ListBox::new();
    list.set_widget_name("favorites-list");
    list.set_selection_mode(gtk::SelectionMode::Single);
    list.add_css_class("note-feed");
    section.append(&list);
    (section, list)
}

fn browser_menu_button() -> gtk::MenuButton {
    let menu = gtk::gio::Menu::new();
    menu.append(Some("Import Note"), Some(IMPORT_NOTE_ACTION));

    let button = gtk::MenuButton::new();
    button.set_widget_name("browser-menu-button");
    button.set_icon_name("view-more-symbolic");
    button.set_tooltip_text(Some("More options"));
    button.add_css_class("flat");
    button.set_menu_model(Some(&menu));
    button
}

fn build_category_empty_card() -> (gtk::Box, gtk::Button) {
    let card = gtk::Box::new(gtk::Orientation::Vertical, 8);
    card.set_widget_name("browser-category-empty-card");
    card.add_css_class("card");
    card.add_css_class("category-empty-card");
    card.set_visible(false);
    let title = gtk::Label::new(Some("No notes in this category"));
    title.set_xalign(0.0);
    title.add_css_class("category-empty-card-title");
    let description = gtk::Label::new(Some("Create a note to get started."));
    description.set_xalign(0.0);
    description.add_css_class("dim-label");
    let new_note = gtk::Button::with_label("New Note");
    new_note.set_widget_name("browser-category-empty-new-note-button");
    new_note.add_css_class("suggested-action");
    new_note.set_halign(gtk::Align::Start);
    card.append(&title);
    card.append(&description);
    card.append(&new_note);
    (card, new_note)
}

fn build_search_empty_card() -> gtk::Box {
    let card = gtk::Box::new(gtk::Orientation::Vertical, 4);
    card.set_widget_name("browser-search-empty-card");
    card.add_css_class("card");
    card.add_css_class("search-empty-card");
    let title = gtk::Label::new(Some("No matching notes"));
    title.set_xalign(0.0);
    title.add_css_class("search-empty-card-title");
    let description = gtk::Label::new(Some("Try a different search term."));
    description.set_xalign(0.0);
    description.add_css_class("dim-label");
    card.append(&title);
    card.append(&description);
    card
}

fn build_note_search_controls() -> (gtk::SearchBar, gtk::SearchEntry, gtk::ToggleButton) {
    let search_toggle = gtk::ToggleButton::new();
    search_toggle.set_widget_name("note-search-toggle");
    search_toggle.set_icon_name("system-search-symbolic");
    search_toggle.set_tooltip_text(Some("Search notes (Ctrl+F)"));
    search_toggle.add_css_class("flat");
    let search_bar = gtk::SearchBar::new();
    search_bar.set_widget_name("note-search-bar");
    search_bar.set_size_request(0, -1);
    search_bar.set_show_close_button(true);
    let search = gtk::SearchEntry::new();
    search.set_widget_name("note-search-entry");
    search.set_placeholder_text(Some("Search notes"));
    search.set_hexpand(true);
    search_bar.set_child(Some(&search));
    search_bar.connect_entry(&search);
    (search_bar, search, search_toggle)
}

/// Captures browser shortcuts before child widgets consume them.
fn install_browser_shortcuts(view: &adw::ToolbarView, dispatcher: &AppDispatcher) {
    let controller = gtk::EventControllerKey::new();
    controller.set_name(Some("browser-shortcuts"));
    controller.set_propagation_phase(gtk::PropagationPhase::Capture);
    let action_host = view.clone().upcast::<gtk::Widget>();
    let action_host_for_callback = action_host.clone();
    let dispatcher = dispatcher.clone();
    controller.connect_key_pressed(move |_, key, _, modifiers| {
        if !modifiers.contains(gtk::gdk::ModifierType::CONTROL_MASK)
            || modifiers.contains(gtk::gdk::ModifierType::SHIFT_MASK)
        {
            return glib::Propagation::Proceed;
        }
        if key == gtk::gdk::Key::f {
            let _ = dispatcher.dispatch(AppMsg::Browser(BrowserMsg::SearchShortcutRequested));
            return glib::Propagation::Stop;
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
    references: &BrowserViewRefs,
    new_note: &gtk::Button,
) {
    let dispatcher_for_search = dispatcher.clone();
    references.search_entry.connect_changed(move |entry| {
        let _ = dispatcher_for_search.dispatch(AppMsg::Browser(BrowserMsg::SearchChanged(
            entry.text().to_string(),
        )));
    });
    let dispatcher_for_toggle = dispatcher.clone();
    references.search_toggle.connect_toggled(move |toggle| {
        let message = if toggle.is_active() {
            BrowserMsg::SearchOpened
        } else {
            BrowserMsg::SearchVisibilityChanged(false)
        };
        let _ = dispatcher_for_toggle.dispatch(AppMsg::Browser(message));
    });
    let dispatcher_for_search_bar = dispatcher.clone();
    references
        .search_bar
        .connect_search_mode_enabled_notify(move |bar| {
            let _ = dispatcher_for_search_bar.dispatch(AppMsg::Browser(
                BrowserMsg::SearchVisibilityChanged(bar.is_search_mode()),
            ));
        });
    let dispatcher_for_search_stop = dispatcher.clone();
    references.search_entry.connect_stop_search(move |_| {
        let _ = dispatcher_for_search_stop
            .dispatch(AppMsg::Browser(BrowserMsg::SearchVisibilityChanged(false)));
    });
    connect_new_note_action(dispatcher, new_note);
    connect_new_note_action(dispatcher, &references.empty_new_note_button);
}

fn browser_feed_factory(
    dispatcher: &AppDispatcher,
    context: Rc<RefCell<BrowserFeedContext>>,
) -> gtk::SignalListItemFactory {
    let factory = gtk::SignalListItemFactory::new();
    factory.connect_setup(|_, item| {
        let Some(item) = item.downcast_ref::<gtk::ListItem>() else {
            return;
        };
        item.set_child(Some(&gtk::Box::new(gtk::Orientation::Vertical, 0)));
    });
    let dispatcher = dispatcher.clone();
    factory.connect_bind(move |_, item| {
        let Some(item) = item.downcast_ref::<gtk::ListItem>() else {
            return;
        };
        let Some(container) = item.child().and_downcast::<gtk::Box>() else {
            return;
        };
        while let Some(child) = container.first_child() {
            container.remove(&child);
        }
        container.set_css_classes(&[]);
        container.set_widget_name("");
        container.set_margin_start(0);
        container.set_margin_end(0);
        container.set_margin_top(0);
        container.set_margin_bottom(0);
        let Some(object) = item.item().and_downcast::<glib::BoxedAnyObject>() else {
            return;
        };
        let feed_item = object.borrow::<BrowserFeedItem>().clone();
        container.set_margin_start(18);
        container.set_margin_end(18);
        match feed_item {
            BrowserFeedItem::Hero => {
                container.set_margin_top(18);
                container.set_margin_bottom(2);
                let hero = gtk::Box::new(gtk::Orientation::Vertical, 0);
                hero.set_widget_name("browser-category-hero");
                hero.add_css_class("category-hero");
                render_category_hero(
                    &hero,
                    &context.borrow().sidebar,
                    context.borrow().selected_category,
                    Some(&dispatcher),
                );
                container.append(&hero);
            }
            BrowserFeedItem::Favorites(notes) => {
                let context = context.borrow().clone();
                container.set_margin_bottom(4);
                container.append(&favorites_feed_section(&notes, &context, &dispatcher));
            }
            BrowserFeedItem::SearchEmpty => {
                container.set_margin_top(12);
                container.append(&build_search_empty_card());
            }
            BrowserFeedItem::CategoryEmpty => {
                container.set_margin_top(12);
                let (card, new_note) = build_category_empty_card();
                card.set_visible(true);
                connect_new_note_action(&dispatcher, &new_note);
                container.append(&card);
            }
            BrowserFeedItem::Heading(group) => container.append(&date_group_heading(group)),
            BrowserFeedItem::Note(note) => {
                let context = context.borrow().clone();
                container.set_widget_name(&format!("note:{}", note.id));
                container.set_css_classes(&["card", "activatable", "note-card"]);
                // Match the former ListBox row's card spacing without nesting a second card.
                container.set_margin_start(4);
                container.set_margin_end(4);
                container.set_margin_top(6);
                container.set_margin_bottom(6);
                populate_note_card(&container, &note, &context, Some(&dispatcher));
            }
            BrowserFeedItem::LoadMore { label, sensitive } => {
                container.set_margin_top(8);
                container.set_margin_bottom(18);
                let button = gtk::Button::with_label(&label);
                button.set_widget_name("browser-load-more");
                button.add_css_class("flat");
                button.set_halign(gtk::Align::Center);
                button.set_sensitive(sensitive);
                let dispatcher = dispatcher.clone();
                button.connect_clicked(move |_| {
                    let _ = dispatcher.dispatch(AppMsg::Browser(BrowserMsg::LoadMore));
                });
                container.append(&button);
            }
        }
    });
    factory
}

fn favorites_feed_section(
    notes: &[NoteSummary],
    context: &BrowserFeedContext,
    dispatcher: &AppDispatcher,
) -> gtk::Widget {
    let (section, list) = build_favorites_section();
    section.set_visible(!notes.is_empty());
    if notes.is_empty() {
        return section.upcast();
    }
    append_favorites_heading(&list);
    for note in notes {
        let row = gtk::ListBoxRow::new();
        row.set_widget_name(&format!("favorite-note:{}", note.id));
        row.add_css_class("card");
        row.add_css_class("activatable");
        row.add_css_class("note-card");
        row.set_child(Some(&note_feed_card(note, context, Some(dispatcher))));
        list.append(&row);
    }
    connect_note_row_activation(dispatcher, &list);
    section.upcast()
}

fn append_favorites_heading(list: &gtk::ListBox) {
    let content = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    content.set_halign(gtk::Align::Start);
    let label = gtk::Label::new(Some("Favorites"));
    label.set_xalign(0.0);
    label.add_css_class("date-heading-label");
    content.append(&label);
    let icon = gtk::Image::from_icon_name("starred-symbolic");
    icon.set_widget_name("favorites-heading-icon");
    icon.set_pixel_size(14);
    icon.set_valign(gtk::Align::Center);
    content.append(&icon);
    let row = gtk::ListBoxRow::new();
    row.set_selectable(false);
    row.add_css_class("date-heading");
    row.set_child(Some(&content));
    list.append(&row);
}

fn date_group_heading(group: NoteDateGroup) -> gtk::Widget {
    let label = gtk::Label::new(Some(&group.label()));
    label.set_widget_name(&format!("note-group:{}", group.identifier()));
    label.set_xalign(0.0);
    label.add_css_class("date-heading-label");
    let heading = gtk::Box::new(gtk::Orientation::Vertical, 0);
    heading.add_css_class("date-heading");
    heading.set_margin_top(22);
    heading.set_margin_bottom(6);
    heading.append(&label);
    heading.upcast()
}

pub(crate) fn note_feed_card(
    note: &NoteSummary,
    context: &BrowserFeedContext,
    dispatcher: Option<&AppDispatcher>,
) -> gtk::Widget {
    let card = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    card.set_widget_name(&format!("note:{}", note.id));
    populate_note_card(&card, note, context, dispatcher);
    card.upcast()
}

fn populate_note_card(
    card: &gtk::Box,
    note: &NoteSummary,
    feed_context: &BrowserFeedContext,
    dispatcher: Option<&AppDispatcher>,
) {
    let content = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    content.set_margin_start(12);
    content.set_margin_end(8);
    content.set_margin_top(10);
    content.set_margin_bottom(10);
    let category_color = feed_context
        .show_category
        .then(|| note_category_color(note, &feed_context.sidebar))
        .flatten();
    content.append(&note_card_details(
        note,
        feed_context.show_category,
        category_color,
    ));
    if let (LoadState::Ready(categories), Some(dispatcher)) = (&feed_context.sidebar, dispatcher) {
        content.append(&note_actions(note, categories, dispatcher));
    }
    card.append(&content);
}

pub(crate) fn note_category_color(
    note: &NoteSummary,
    sidebar: &LoadState<Vec<CategorySummary>>,
) -> Option<CategoryColor> {
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
    note: &NoteSummary,
    categories: &[CategorySummary],
    dispatcher: &AppDispatcher,
) -> gtk::MenuButton {
    let menu = gtk::MenuButton::new();
    menu.set_widget_name(&format!("note-menu:{}", note.id));
    menu.set_icon_name("view-more-symbolic");
    menu.set_tooltip_text(Some("Note actions"));
    menu.add_css_class("flat");
    let popover = gtk::Popover::new();
    let actions = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let favorite_button = gtk::Button::with_label(if note.is_favorite {
        "Remove from Favorites"
    } else {
        "Mark as Favorite"
    });
    favorite_button.set_widget_name(&format!("favorite-note-menu-button:{}", note.id));
    favorite_button.add_css_class("flat");
    let dispatcher_for_favorite = dispatcher.clone();
    let popover_for_favorite = popover.clone();
    let note_id = note.id;
    let revision = note.revision;
    let is_favorite = note.is_favorite;
    favorite_button.connect_clicked(move |_| {
        popover_for_favorite.popdown();
        let _ = dispatcher_for_favorite.dispatch(AppMsg::Action(ActionMsg::SetNoteFavorite {
            note_id,
            revision,
            is_favorite: !is_favorite,
        }));
    });
    actions.append(&favorite_button);
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
        let _ =
            dispatcher_for_export.dispatch(AppMsg::Navigation(NavigationMsg::ExportNote(note_id)));
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

fn connect_note_row_activation(dispatcher: &AppDispatcher, list: &gtk::ListBox) {
    let dispatcher = dispatcher.clone();
    list.connect_row_activated(move |_list, row| {
        let widget_name = row.widget_name();
        let raw_id = widget_name
            .strip_prefix("note:")
            .or_else(|| widget_name.strip_prefix("favorite-note:"));
        let Some(raw_id) = raw_id else {
            return;
        };
        let Ok(id) = uuid::Uuid::parse_str(raw_id) else {
            return;
        };
        let _ = dispatcher.dispatch(AppMsg::Navigation(NavigationMsg::OpenNote(
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
pub(crate) fn note_card_details(
    note: &NoteSummary,
    show_category: bool,
    category_color: Option<CategoryColor>,
) -> gtk::Box {
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
        category.set_ellipsize(gtk::pango::EllipsizeMode::End);
        category.set_single_line_mode(true);
        category.set_max_width_chars(18);
        category.add_css_class("note-category-pill");
        if let Some(color) = category_color {
            category.add_css_class(category_color_css_class(color));
        }
        metadata.append(&category);
    }
    let updated = gtk::Label::new(Some(&format!(
        "Updated {}",
        relative_update_time(note.updated_at, OffsetDateTime::now_utc())
    )));
    updated.set_widget_name(&format!("note-updated:{}", note.id));
    updated.add_css_class("note-card-updated");
    updated.set_xalign(0.0);
    updated.set_ellipsize(gtk::pango::EllipsizeMode::End);
    updated.set_single_line_mode(true);
    updated.set_hexpand(true);
    metadata.append(&updated);
    details.append(&metadata);
    details
}

pub(crate) fn local_day(timestamp: OffsetDateTime) -> time::Date {
    day_in_timezone(timestamp, &glib::TimeZone::local())
        .unwrap_or_else(|_| timestamp.to_offset(UtcOffset::UTC).date())
}

fn day_in_timezone(
    timestamp: OffsetDateTime,
    timezone: &glib::TimeZone,
) -> Result<time::Date, glib::BoolError> {
    let local = glib::DateTime::from_unix_utc(timestamp.unix_timestamp())?.to_timezone(timezone)?;
    timestamp
        .to_offset(UtcOffset::UTC)
        .checked_add(Duration::microseconds(local.utc_offset().as_microseconds()))
        .map(OffsetDateTime::date)
        .ok_or_else(|| glib::bool_error!("Local date is outside the supported range"))
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

pub(crate) fn relative_update_time(updated_at: OffsetDateTime, now: OffsetDateTime) -> String {
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
    let format = if day.year() == today.year() {
        format_description!("[month repr:short] [day padding:none]")
    } else {
        format_description!("[month repr:short] [day padding:none], [year]")
    };
    day.format(format).unwrap_or_else(|_| day.to_string())
}

fn elapsed_label(amount: i64, unit: &str) -> String {
    let suffix = if amount == 1 { "" } else { "s" };
    format!("{amount} {unit}{suffix} ago")
}

#[cfg(test)]
mod tests;
