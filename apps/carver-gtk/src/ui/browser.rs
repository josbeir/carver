//! Recent-note browser and responsive content composition.

use std::{borrow::Cow, cell::Cell, cell::RefCell, rc::Rc};

use carver_config::Config;
use carver_sdk::{Category, CategoryColor, CategorySummary, NoteSummary};
use gettextrs::{gettext, ngettext};
use gtk::prelude::*;
use libadwaita as adw;
use time::{Duration, OffsetDateTime, UtcOffset};

use super::{
    dialogs::{
        IMPORT_NOTE_ACTION, NEW_NOTE_ACTION, NEW_NOTE_FROM_CLIPBOARD_ACTION,
        NEW_NOTE_FROM_MARKDOWN_CLIPBOARD_ACTION, category_color_css_class, category_icon_name,
        show_category_dialog, show_category_trash_confirmation, show_move_note_dialog,
    },
    editor::{EditorSurface, build_editor},
    search::{build_search_controls, connect_search_controls, install_search_shortcut},
    sidebar::{CompactNavigation, sidebar_toggle_button},
    trash::{TrashViewRefs, build_trash},
};
use crate::mvu::{ActionMsg, AppDispatcher, AppMsg, BrowserMsg, LoadState, NavigationMsg};
use crate::view::{EditorFactory, Workspace};

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
    pub(crate) fn label(self) -> String {
        match self {
            Self::Today => gettext("Today"),
            Self::Yesterday => gettext("Yesterday"),
            Self::ThisWeek => gettext("This Week"),
            Self::ThisMonth => gettext("This Month"),
            Self::EarlierThisYear => gettext("Earlier This Year"),
            Self::Year(year) => year.to_string(),
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
    FavoritesHeading,
    Favorite(NoteSummary),
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
    pub(crate) hero: gtk::Box,
    pub(crate) pages: gtk::Stack,
    /// Whether the feed still has another page to load, so the scroll handler
    /// can skip dispatching once everything is on screen.
    pub(crate) load_more_available: Rc<Cell<bool>>,
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
    pub(crate) workspace: Workspace,
    pub(crate) browser: BrowserViewRefs,
    pub(crate) trash: TrashViewRefs,
    pub(crate) base: crate::ui::bases::BaseViewRefs,
}

/// Builds the Notes page, tab host, and per-tab editor factory.
pub(crate) fn build_content(
    dispatcher: &AppDispatcher,
    config: &Config,
    assets_dir: Option<&std::path::Path>,
    source_syntax_dir: &std::path::Path,
    split_view: &adw::NavigationSplitView,
    compact_navigation: &CompactNavigation,
    toast_overlay: &adw::ToastOverlay,
) -> ContentSurface {
    let stack = gtk::Stack::new();
    stack.set_widget_name("content-route-stack");
    stack.set_hhomogeneous(false);
    stack.set_transition_type(gtk::StackTransitionType::SlideLeftRight);
    let (browser, browser_refs) = build_browser(dispatcher, split_view, compact_navigation);
    stack.add_named(&browser, Some("browser"));
    let (base, base_refs) =
        crate::ui::bases::build_base(dispatcher, split_view, compact_navigation);
    stack.add_named(&base, Some("base"));
    let (trash, trash_refs) = build_trash(dispatcher);
    stack.add_named(&trash, Some("trash"));
    stack.set_visible_child_name("browser");

    let tab_view = adw::TabView::new();
    tab_view.set_widget_name("workspace-tabs");
    let notes_page = tab_view.append_pinned(&stack);
    notes_page.set_title(&gettext("Notes"));
    tab_view.set_page_pinned(&notes_page, true);

    let factory: EditorFactory = {
        let dispatcher = dispatcher.clone();
        let config = config.clone();
        let assets_dir = assets_dir.map(std::path::Path::to_path_buf);
        let source_syntax_dir = source_syntax_dir.to_path_buf();
        let toast_overlay = toast_overlay.clone();
        let split_view = split_view.clone();
        let compact_navigation = Rc::clone(compact_navigation);
        Box::new(move |mode| {
            build_editor(
                &dispatcher,
                &config,
                assets_dir.as_deref(),
                &source_syntax_dir,
                &toast_overlay,
                &split_view,
                &compact_navigation,
                mode,
            )
            .map(EditorSurface::into_parts)
        })
    };
    let tab_bar = adw::TabBar::new();
    tab_bar.set_widget_name("workspace-tab-bar");
    tab_bar.set_autohide(false);
    tab_bar.set_expand_tabs(true);
    let new_note = gtk::Button::from_icon_name("tab-new-symbolic");
    new_note.set_widget_name("workspace-new-note-button");
    new_note.add_css_class("flat");
    new_note.set_tooltip_text(Some(&gettext("New note")));
    new_note.update_property(&[gtk::accessible::Property::Label(&gettext("New note"))]);
    let new_note_dispatcher = dispatcher.clone();
    new_note.connect_clicked(move |_| {
        let _ = new_note_dispatcher.dispatch(AppMsg::Navigation(NavigationMsg::CreateNote));
    });
    tab_bar.set_end_action_widget(Some(&new_note));
    let workspace = Workspace::new(tab_view.clone(), notes_page.clone(), tab_bar, factory);
    workspace.connect(dispatcher);

    ContentSurface {
        widget: tab_view.clone().upcast(),
        route_stack: stack,
        workspace,
        browser: browser_refs,
        trash: trash_refs,
        base: base_refs,
    }
}

/// Builds the default recent-note and search view.
#[expect(
    clippy::too_many_lines,
    reason = "the one-time Browser composition keeps its widgets and paired view references together"
)]
pub(crate) fn build_browser(
    dispatcher: &AppDispatcher,
    split_view: &adw::NavigationSplitView,
    compact_navigation: &CompactNavigation,
) -> (gtk::Widget, BrowserViewRefs) {
    let view = adw::ToolbarView::new();
    view.set_widget_name("browser-surface");
    let header = adw::HeaderBar::new();
    let new_note = browser_new_note_split_button(dispatcher);
    header.pack_end(&new_note);
    header.pack_start(&sidebar_toggle_button(
        split_view,
        compact_navigation,
        "toggle-categories-button",
    ));
    let search = build_search_controls(
        "note",
        &gettext("Search notes"),
        &gettext("Search notes (Ctrl+F)"),
    );
    header.pack_start(&search.toggle);
    view.add_top_bar(&header);
    view.add_top_bar(&search.bar);

    let feed_store = gtk::gio::ListStore::new::<glib::BoxedAnyObject>();
    let feed_context = Rc::new(RefCell::new(BrowserFeedContext {
        show_category: true,
        sidebar: LoadState::Idle,
        selected_category: None,
        favorites: Vec::new(),
    }));
    // Note cards are activatable but do not have a persistent selection state.
    // A `SingleSelection` makes GTK retain the row selected after pointer motion,
    // which makes the hover treatment appear stuck once the pointer leaves.
    let selection = gtk::NoSelection::new(Some(feed_store.clone()));
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
    // The hero sits outside the scroller so the active library header stays
    // fixed above the feed, matching the Base list view.
    let hero = gtk::Box::new(gtk::Orientation::Vertical, 0);
    hero.set_widget_name("browser-category-hero");
    hero.add_css_class("category-hero");
    hero.set_margin_start(18);
    hero.set_margin_end(18);
    let hero_clamp = adw::Clamp::new();
    hero_clamp.set_widget_name("browser-hero-clamp");
    hero_clamp.set_maximum_size(720);
    hero_clamp.set_tightening_threshold(520);
    hero_clamp.set_child(Some(&hero));
    let list_page = gtk::Box::new(gtk::Orientation::Vertical, 0);
    list_page.set_widget_name("browser-list-page");
    list_page.set_vexpand(true);
    list_page.append(&hero_clamp);
    list_page.append(&scroll);
    let pages = gtk::Stack::new();
    pages.set_widget_name("browser-content-pages");
    pages.add_named(&list_page, Some("contents"));
    let status = adw::StatusPage::builder()
        .title(gettext("No notes yet"))
        .description(gettext("Create a note to get started."))
        .icon_name("document-new-symbolic")
        .build();
    status.set_widget_name("browser-empty-status");
    let empty_new_note = gtk::Button::with_label(&gettext("New Note"));
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
                BrowserFeedItem::Note(note) | BrowserFeedItem::Favorite(note) => note.id,
                _ => return,
            }
        };
        let _ = dispatcher_for_feed.dispatch(AppMsg::Navigation(NavigationMsg::OpenNote {
            note_id,
            intent: crate::mvu::NoteOpenIntent::Default,
        }));
    });

    connect_search_controls(
        dispatcher,
        &search,
        |query| AppMsg::Browser(BrowserMsg::SearchChanged(query)),
        AppMsg::Browser(BrowserMsg::SearchOpened),
        |visible| AppMsg::Browser(BrowserMsg::SearchVisibilityChanged(visible)),
    );
    install_search_shortcut(
        &view,
        dispatcher,
        "browser-search-shortcut",
        AppMsg::Browser(BrowserMsg::SearchShortcutRequested),
    );
    let references = BrowserViewRefs {
        list,
        feed_store,
        feed_context,
        hero,
        pages,
        load_more_available: Rc::new(Cell::new(false)),
        search_bar: search.bar,
        search_entry: search.entry,
        search_toggle: search.toggle,
        empty_new_note_button: empty_new_note,
        status,
    };
    connect_browser_actions(dispatcher, &references);
    connect_browser_paging(dispatcher, &references);
    install_browser_shortcuts(&view);
    (view.upcast(), references)
}

fn connect_browser_paging(dispatcher: &AppDispatcher, references: &BrowserViewRefs) {
    let dispatcher_for_scroll = dispatcher.clone();
    let available = Rc::clone(&references.load_more_available);
    let Some(adjustment) = references.list.vadjustment() else {
        return;
    };
    adjustment.connect_value_changed(move |adjustment| {
        // Dispatching clones and re-renders the whole model, so only ask for
        // more while the feed actually has another page.
        if !available.get() {
            return;
        }
        let remaining = adjustment.upper() - adjustment.page_size() - adjustment.value();
        if remaining <= adjustment.page_size() * 2.0 {
            let _ = dispatcher_for_scroll.dispatch(AppMsg::Browser(BrowserMsg::LoadMore));
        }
    });
}

/// Builds the header's note-creation split button.
///
/// The primary action creates a blank note; the dropdown groups the related
/// actions that add a note to the library. A split button keeps `Import Note`
/// discoverable without a standalone overflow menu whose only entry it was.
fn browser_new_note_split_button(dispatcher: &AppDispatcher) -> adw::SplitButton {
    let menu = gtk::gio::Menu::new();
    menu.append(Some(&gettext("Import Note")), Some(IMPORT_NOTE_ACTION));
    menu.append(
        Some(&gettext("New from Clipboard")),
        Some(NEW_NOTE_FROM_CLIPBOARD_ACTION),
    );
    menu.append(
        Some(&gettext("New from Clipboard as Markdown")),
        Some(NEW_NOTE_FROM_MARKDOWN_CLIPBOARD_ACTION),
    );

    let button = adw::SplitButton::new();
    button.set_widget_name("new-note-button");
    button.set_icon_name("document-new-symbolic");
    button.set_tooltip_text(Some(&gettext("New Note")));
    button.set_dropdown_tooltip(&gettext("More options"));
    button.update_property(&[gtk::accessible::Property::Label(&gettext("New Note"))]);
    button.set_menu_model(Some(&menu));

    let dispatcher = dispatcher.clone();
    button.connect_clicked(move |_| {
        let _ = dispatcher.dispatch(AppMsg::Navigation(NavigationMsg::CreateNote));
    });
    button
}

fn build_category_empty_card() -> (gtk::Box, gtk::Button) {
    let card = gtk::Box::new(gtk::Orientation::Vertical, 8);
    card.add_css_class("card");
    card.add_css_class("category-empty-card");
    let title = gtk::Label::new(Some(&gettext("No notes in this category")));
    title.set_xalign(0.0);
    title.add_css_class("category-empty-card-title");
    let description = gtk::Label::new(Some(&gettext("Create a note to get started.")));
    description.set_xalign(0.0);
    description.add_css_class("dim-label");
    let new_note = gtk::Button::with_label(&gettext("New Note"));
    new_note.add_css_class("suggested-action");
    new_note.set_halign(gtk::Align::Start);
    card.append(&title);
    card.append(&description);
    card.append(&new_note);
    (card, new_note)
}

fn build_search_empty_card() -> gtk::Box {
    let card = gtk::Box::new(gtk::Orientation::Vertical, 4);
    card.add_css_class("card");
    card.add_css_class("search-empty-card");
    let title = gtk::Label::new(Some(&gettext("No matching notes")));
    title.set_xalign(0.0);
    title.add_css_class("search-empty-card-title");
    let description = gtk::Label::new(Some(&gettext("Try a different search term.")));
    description.set_xalign(0.0);
    description.add_css_class("dim-label");
    card.append(&title);
    card.append(&description);
    card
}

/// Captures browser shortcuts before child widgets consume them.
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

fn connect_browser_actions(dispatcher: &AppDispatcher, references: &BrowserViewRefs) {
    connect_new_note_action(dispatcher, &references.empty_new_note_button);
}

/// Builds one reusable browser row with modifier-aware note activation.
///
/// The row shell and one page per feed-item kind are created once per physical
/// row; [`bind_browser_row`] only updates the visible page. Scrolling therefore
/// never rebuilds a card or its overflow menu.
fn setup_browser_row(
    dispatcher: &AppDispatcher,
    context: &Rc<RefCell<BrowserFeedContext>>,
    item: &gtk::ListItem,
) {
    let container = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let stack = gtk::Stack::new();
    stack.set_transition_type(gtk::StackTransitionType::None);
    stack.set_hhomogeneous(false);
    stack.set_vhomogeneous(false);
    stack.set_interpolate_size(false);
    stack.set_hexpand(true);
    stack.add_named(&date_heading_page(), Some("heading"));
    stack.add_named(&favorites_heading(), Some("favorites-heading"));
    stack.add_named(&note_card_page(item, dispatcher, context), Some("card"));
    stack.add_named(&build_search_empty_card(), Some("search-empty"));
    let (category_empty, category_empty_button) = build_category_empty_card();
    connect_new_note_action(dispatcher, &category_empty_button);
    stack.add_named(&category_empty, Some("category-empty"));
    stack.add_named(&load_more_button(dispatcher), Some("load-more"));
    container.append(&stack);
    let weak_item = item.downgrade();
    crate::ui::intent::connect_modified_note_open_with(&container, dispatcher, move || {
        feed_item_note_id(&weak_item)
    });
    item.set_child(Some(&container));
}

/// Resolves the note bound to a recycled browser row.
fn feed_item_note_id(item: &glib::WeakRef<gtk::ListItem>) -> Option<carver_sdk::NoteId> {
    let item = item.upgrade()?;
    let object = item.item().and_downcast::<glib::BoxedAnyObject>()?;
    match &*object.borrow::<BrowserFeedItem>() {
        BrowserFeedItem::Note(note) | BrowserFeedItem::Favorite(note) => Some(note.id),
        _ => None,
    }
}

fn browser_feed_factory(
    dispatcher: &AppDispatcher,
    context: Rc<RefCell<BrowserFeedContext>>,
) -> gtk::SignalListItemFactory {
    let factory = gtk::SignalListItemFactory::new();
    factory.connect_setup({
        let dispatcher = dispatcher.clone();
        let context = Rc::clone(&context);
        move |_, item| {
            let Some(item) = item.downcast_ref::<gtk::ListItem>() else {
                return;
            };
            setup_browser_row(&dispatcher, &context, item);
        }
    });
    factory.connect_bind(move |_, item| {
        let Some(item) = item.downcast_ref::<gtk::ListItem>() else {
            return;
        };
        let Some(container) = item.child().and_downcast::<gtk::Box>() else {
            return;
        };
        bind_browser_row(&container, item, &context);
    });
    factory
}

/// Updates a recycled row for the bound feed item without rebuilding it.
fn bind_browser_row(
    container: &gtk::Box,
    item: &gtk::ListItem,
    context: &Rc<RefCell<BrowserFeedContext>>,
) {
    let Some(stack) = container.first_child().and_downcast::<gtk::Stack>() else {
        return;
    };
    let Some(object) = item.item().and_downcast::<glib::BoxedAnyObject>() else {
        return;
    };
    let feed_item = object.borrow::<BrowserFeedItem>();
    let target = match &*feed_item {
        BrowserFeedItem::FavoritesHeading => "favorites-heading",
        BrowserFeedItem::Favorite(_) | BrowserFeedItem::Note(_) => "card",
        BrowserFeedItem::SearchEmpty => "search-empty",
        BrowserFeedItem::CategoryEmpty => "category-empty",
        BrowserFeedItem::Heading(_) => "heading",
        BrowserFeedItem::LoadMore { .. } => "load-more",
    };
    // Only touch the previously shown page when the row is recycled to a new
    // kind; a same-kind rebind keeps the page and its names in place.
    let previous = stack.visible_child_name();
    let switching = previous.as_deref() != Some(target);
    container.set_widget_name("");
    if switching
        && let Some(previous) = previous
        && let Some(page) = stack.child_by_name(&previous)
    {
        blank_names(&page);
    }
    container.set_css_classes(&[]);
    container.set_margin_start(18);
    container.set_margin_end(18);
    container.set_margin_top(0);
    container.set_margin_bottom(0);
    match &*feed_item {
        BrowserFeedItem::FavoritesHeading => {
            container.set_margin_top(12);
            container.set_margin_bottom(2);
            if let Some(page) = stack.child_by_name("favorites-heading") {
                page.set_widget_name("favorites-section");
                if let Some(icon) = find_descendant::<gtk::Image>(&page) {
                    icon.set_widget_name("favorites-heading-icon");
                }
            }
        }
        BrowserFeedItem::Favorite(note) => {
            bind_browser_card(container, &stack, note, true, &context.borrow());
        }
        BrowserFeedItem::SearchEmpty => {
            container.set_margin_top(12);
            if let Some(page) = stack.child_by_name("search-empty") {
                page.set_widget_name("browser-search-empty-card");
            }
        }
        BrowserFeedItem::CategoryEmpty => {
            container.set_margin_top(12);
            if let Some(page) = stack.child_by_name("category-empty") {
                page.set_widget_name("browser-category-empty-card");
                if let Some(button) = find_descendant::<gtk::Button>(&page) {
                    button.set_widget_name("browser-category-empty-new-note-button");
                }
            }
        }
        BrowserFeedItem::Heading(group) => {
            if let Some(page) = stack.child_by_name("heading")
                && let Some(label) = find_label_with_class(&page, "date-heading-label")
            {
                label.set_text(&group.label());
                label.set_widget_name(&format!("note-group:{}", group.identifier()));
            }
        }
        BrowserFeedItem::Note(note) => {
            bind_browser_card(container, &stack, note, false, &context.borrow());
        }
        BrowserFeedItem::LoadMore { label, sensitive } => {
            container.set_margin_top(8);
            container.set_margin_bottom(18);
            if let Some(button) = stack
                .child_by_name("load-more")
                .and_downcast::<gtk::Button>()
            {
                button.set_widget_name("browser-load-more");
                button.set_label(label);
                button.set_sensitive(*sensitive);
            }
        }
    }
    if switching {
        stack.set_visible_child_name(target);
    }
}

/// Clears widget names in a recycled row page so hidden rows cannot be matched.
fn blank_names(root: &gtk::Widget) {
    root.set_widget_name("");
    let mut child = root.first_child();
    while let Some(current) = child {
        blank_names(&current);
        child = current.next_sibling();
    }
}

/// Finds the first descendant label carrying `class`.
fn find_label_with_class(root: &gtk::Widget, class: &str) -> Option<gtk::Label> {
    if let Some(label) = root.downcast_ref::<gtk::Label>()
        && label.has_css_class(class)
    {
        return Some(label.clone());
    }
    let mut child = root.first_child();
    while let Some(current) = child {
        if let Some(found) = find_label_with_class(&current, class) {
            return Some(found);
        }
        child = current.next_sibling();
    }
    None
}

/// Finds the first descendant box carrying `class`.
fn find_box_with_class(root: &gtk::Widget, class: &str) -> Option<gtk::Box> {
    if let Some(box_) = root.downcast_ref::<gtk::Box>()
        && box_.has_css_class(class)
    {
        return Some(box_.clone());
    }
    let mut child = root.first_child();
    while let Some(current) = child {
        if let Some(found) = find_box_with_class(&current, class) {
            return Some(found);
        }
        child = current.next_sibling();
    }
    None
}

/// Finds the first descendant widget of type `T`.
fn find_descendant<T: IsA<gtk::Widget> + Clone>(root: &gtk::Widget) -> Option<T> {
    if let Some(widget) = root.downcast_ref::<T>() {
        return Some(widget.clone());
    }
    let mut child = root.first_child();
    while let Some(current) = child {
        if let Some(found) = find_descendant::<T>(&current) {
            return Some(found);
        }
        child = current.next_sibling();
    }
    None
}

fn favorites_heading() -> gtk::Widget {
    let content = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    content.set_halign(gtk::Align::Start);
    let label = gtk::Label::new(Some(&gettext("Favorites")));
    label.set_xalign(0.0);
    label.add_css_class("date-heading-label");
    content.append(&label);
    let icon = gtk::Image::from_icon_name("starred-symbolic");
    icon.set_pixel_size(14);
    icon.set_valign(gtk::Align::Center);
    content.append(&icon);
    content.upcast()
}

fn date_heading_page() -> gtk::Box {
    let label = gtk::Label::new(None);
    label.set_xalign(0.0);
    label.add_css_class("date-heading-label");
    let heading = gtk::Box::new(gtk::Orientation::Vertical, 0);
    heading.add_css_class("date-heading");
    heading.set_margin_top(22);
    heading.set_margin_bottom(6);
    heading.append(&label);
    heading
}

/// Builds the persistent note-card page shared by note and favorite rows.
fn note_card_page(
    item: &gtk::ListItem,
    dispatcher: &AppDispatcher,
    context: &Rc<RefCell<BrowserFeedContext>>,
) -> gtk::Box {
    let page = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let body = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    body.set_margin_start(12);
    body.set_margin_end(8);
    body.set_margin_top(10);
    body.set_margin_bottom(10);
    body.append(&note_card_details_skeleton());
    body.append(&note_menu_button(item, dispatcher, context));
    page.append(&body);
    page
}

/// Projects one note into the persistent card page and its overflow menu.
fn bind_browser_card(
    container: &gtk::Box,
    stack: &gtk::Stack,
    note: &NoteSummary,
    favorite: bool,
    context: &BrowserFeedContext,
) {
    let prefix = if favorite { "favorite-note" } else { "note" };
    container.set_widget_name(&format!("{prefix}:{}", note.id));
    container.set_css_classes(&["card", "activatable", "note-card"]);
    container.set_margin_top(6);
    container.set_margin_bottom(6);
    let Some(page) = stack.child_by_name("card").and_downcast::<gtk::Box>() else {
        return;
    };
    if let Some(details) = find_box_with_class(page.upcast_ref(), "note-card-details") {
        bind_note_card_details(
            &details,
            &NoteCardData {
                note_id: note.id,
                title: &note.title,
                excerpt: &note.excerpt,
                category_name: &note.category_name,
                category_color: note_category_color(note, &context.sidebar),
                updated_at: note.updated_at,
                show_category: context.show_category,
                name_prefix: "note",
            },
        );
    }
    let actions_visible = matches!(context.sidebar, LoadState::Ready(_));
    if let Some(menu) = find_descendant::<gtk::MenuButton>(page.upcast_ref()) {
        menu.set_visible(actions_visible);
        menu.set_widget_name(&format!("note-menu:{}", note.id));
        // Rebuilding the model repopulates the popover, so only swap it when the
        // favorite state (and therefore the label) actually changed.
        if actions_visible {
            let desired = if note.is_favorite {
                gettext("Remove from Favorites")
            } else {
                gettext("Mark as Favorite")
            };
            if menu_favorite_label(&menu).as_deref() != Some(desired.as_str()) {
                menu.set_menu_model(Some(&note_menu_model(note.is_favorite)));
            }
        }
    }
}

/// Returns the favorite entry label currently shown by a note menu.
fn menu_favorite_label(menu: &gtk::MenuButton) -> Option<String> {
    menu.menu_model()
        .and_then(|model| model.item_attribute_value(0, "label", None))
        .map(|value| value.str().unwrap_or_default().to_string())
}

/// Builds the persistent per-row overflow menu.
///
/// The actions are created once and resolve the currently bound note from the
/// list item when they fire, so rebinding a recycled row never rebuilds them.
fn note_menu_button(
    item: &gtk::ListItem,
    dispatcher: &AppDispatcher,
    context: &Rc<RefCell<BrowserFeedContext>>,
) -> gtk::MenuButton {
    let menu = gtk::MenuButton::new();
    menu.set_icon_name("view-more-symbolic");
    menu.set_tooltip_text(Some(&gettext("Note actions")));
    menu.add_css_class("flat");

    let actions = gtk::gio::SimpleActionGroup::new();

    let favorite = gtk::gio::SimpleAction::new("favorite", None);
    let weak_item = item.downgrade();
    let dispatcher_for_favorite = dispatcher.clone();
    favorite.connect_activate(move |_, _| {
        let Some(note) = activate_bound_note(&weak_item) else {
            return;
        };
        let _ = dispatcher_for_favorite.dispatch(AppMsg::Action(ActionMsg::SetNoteFavorite {
            note_id: note.id,
            revision: note.revision,
            is_favorite: !note.is_favorite,
        }));
    });
    actions.add_action(&favorite);

    // A `SimpleAction` activation carries no widget, so resolve the dialog's
    // transient parent from the button. The button is held weakly to avoid a
    // button -> action group -> action -> closure reference cycle.
    let move_action = gtk::gio::SimpleAction::new("move", None);
    let weak_menu = menu.downgrade();
    let weak_item = item.downgrade();
    let dispatcher_for_move = dispatcher.clone();
    let context_for_move = Rc::clone(context);
    move_action.connect_activate(move |_, _| {
        let Some(note) = activate_bound_note(&weak_item) else {
            return;
        };
        let Some(parent) = weak_menu
            .upgrade()
            .and_then(|button| button.root())
            .and_downcast::<gtk::Window>()
        else {
            return;
        };
        let categories = match &context_for_move.borrow().sidebar {
            LoadState::Ready(categories) => categories.clone(),
            _ => Vec::new(),
        };
        show_move_note_dialog(
            Some(&parent),
            &dispatcher_for_move,
            note.id,
            note.category_id,
            &note.title,
            &categories,
        );
    });
    actions.add_action(&move_action);

    let export = gtk::gio::SimpleAction::new("export", None);
    let weak_item = item.downgrade();
    let dispatcher_for_export = dispatcher.clone();
    export.connect_activate(move |_, _| {
        let Some(note) = activate_bound_note(&weak_item) else {
            return;
        };
        let _ =
            dispatcher_for_export.dispatch(AppMsg::Navigation(NavigationMsg::ExportNote(note.id)));
    });
    actions.add_action(&export);

    let trash = gtk::gio::SimpleAction::new("trash", None);
    let weak_item = item.downgrade();
    let dispatcher_for_trash = dispatcher.clone();
    trash.connect_activate(move |_, _| {
        let Some(note) = activate_bound_note(&weak_item) else {
            return;
        };
        let _ = dispatcher_for_trash.dispatch(AppMsg::Action(ActionMsg::TrashNote(note.id)));
    });
    actions.add_action(&trash);

    menu.insert_action_group("note", Some(&actions));
    menu
}

/// Builds a note overflow menu model for the given favorite state.
fn note_menu_model(is_favorite: bool) -> gtk::gio::Menu {
    let model = gtk::gio::Menu::new();
    let favorite_label = if is_favorite {
        gettext("Remove from Favorites")
    } else {
        gettext("Mark as Favorite")
    };
    model.append(Some(&favorite_label), Some("note.favorite"));
    model.append(Some(&gettext("Move…")), Some("note.move"));
    model.append(Some(&gettext("Export note…")), Some("note.export"));
    let danger = gtk::gio::Menu::new();
    danger.append(Some(&gettext("Move to Trash")), Some("note.trash"));
    model.append_section(None, &danger);
    model
}

/// Resolves the note bound to a row when one of its actions fires.
fn activate_bound_note(item: &glib::WeakRef<gtk::ListItem>) -> Option<NoteSummary> {
    let item = item.upgrade()?;
    let object = item.item().and_downcast::<glib::BoxedAnyObject>()?;
    match &*object.borrow::<BrowserFeedItem>() {
        BrowserFeedItem::Note(note) | BrowserFeedItem::Favorite(note) => Some(note.clone()),
        _ => None,
    }
}

/// Builds the persistent footer button that pages in more notes.
fn load_more_button(dispatcher: &AppDispatcher) -> gtk::Button {
    let button = gtk::Button::new();
    button.add_css_class("flat");
    button.set_halign(gtk::Align::Center);
    let dispatcher = dispatcher.clone();
    button.connect_clicked(move |_| {
        let _ = dispatcher.dispatch(AppMsg::Browser(BrowserMsg::LoadMore));
    });
    button
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
    build_hero_content(
        "go-home-symbolic",
        "all-notes-icon",
        &gettext("All notes"),
        &note_count_label(note_count),
        "browser-hero",
        None,
    )
}

fn category_hero(summary: &CategorySummary, dispatcher: Option<&AppDispatcher>) -> gtk::Widget {
    let category = &summary.category;
    let color = category.appearance.color.resolved_for(category.id);
    let actions = dispatcher.map(|dispatcher| category_hero_actions(category, dispatcher));
    build_hero_content(
        category_icon_name(category.appearance.icon),
        category_color_css_class(color),
        &category.name,
        &note_count_label(summary.note_count),
        "browser-hero",
        actions.as_ref(),
    )
}

/// Builds the shared hero header shown above a note list.
///
/// The browser's category hero and the Base list hero share one layout; only the
/// icon, accent color, text, and widget-name prefix differ between them.
pub(crate) fn build_hero_content(
    icon_name: &str,
    color_class: &str,
    title: &str,
    subtitle: &str,
    name_prefix: &str,
    actions: Option<&gtk::Box>,
) -> gtk::Widget {
    let content = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    content.add_css_class("category-hero-content");
    let icon = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    icon.set_widget_name(&format!("{name_prefix}-icon"));
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
    title_label.set_widget_name(&format!("{name_prefix}-title"));
    title_label.add_css_class("title-2");
    title_label.set_xalign(0.0);
    title_label.set_ellipsize(gtk::pango::EllipsizeMode::End);
    title_label.set_single_line_mode(true);
    text.append(&title_label);
    let subtitle_label = gtk::Label::new(Some(subtitle));
    subtitle_label.set_widget_name(&format!("{name_prefix}-subtitle"));
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
    edit.set_tooltip_text(Some(&gettext("Edit Category")));
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
            &gettext("Edit Category"),
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
    trash.set_tooltip_text(Some(&gettext("Move Category to Trash")));
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

pub(crate) fn note_count_label(note_count: usize) -> String {
    tr_fmt!(
        ngettext(
            "{count} note",
            "{count} notes",
            u32::try_from(note_count).unwrap_or(u32::MAX),
        ),
        count = note_count
    )
}

/// Field values the shared read-only note card renders.
///
/// Both the browser feed and the Base list present notes through this type, so a
/// change to the card template applies everywhere without diverging copies.
pub(crate) struct NoteCardData<'a> {
    /// Note identity, used to build stable widget names.
    pub note_id: carver_sdk::NoteId,
    /// Derived title.
    pub title: &'a str,
    /// Short plaintext excerpt.
    pub excerpt: &'a str,
    /// Owning category display name.
    pub category_name: &'a str,
    /// Resolved category accent color for the pill.
    pub category_color: Option<CategoryColor>,
    /// Last edit time.
    pub updated_at: OffsetDateTime,
    /// Whether the category pill is shown.
    pub show_category: bool,
    /// Widget-name prefix, such as `note` or `base-list-note`.
    pub name_prefix: &'a str,
}

/// Applies the shared note-card surface to a recycled row container and appends its body.
pub(crate) fn fill_note_card(container: &gtk::Box, data: &NoteCardData<'_>) {
    container.set_widget_name(&format!("{}:{}", data.name_prefix, data.note_id));
    container.set_css_classes(&["card", "activatable", "note-card"]);
    container.set_margin_top(6);
    container.set_margin_bottom(6);
    let content = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    content.set_margin_start(12);
    content.set_margin_end(8);
    content.set_margin_top(10);
    content.set_margin_bottom(10);
    content.append(&note_card_details(data));
    container.append(&content);
}

/// Clears a recycled list-row surface before it is bound again.
pub(crate) fn reset_note_card_surface(container: &gtk::Box) {
    while let Some(child) = container.first_child() {
        container.remove(&child);
    }
    container.set_css_classes(&[]);
    container.set_widget_name("");
    container.set_margin_start(0);
    container.set_margin_end(0);
    container.set_margin_top(0);
    container.set_margin_bottom(0);
}

/// Builds the shared title/excerpt/metadata body for a note card.
pub(crate) fn note_card_details(data: &NoteCardData<'_>) -> gtk::Box {
    let details = note_card_details_skeleton();
    bind_note_card_details(&details, data);
    details
}

/// Builds an empty note-card body whose labels [`bind_note_card_details`] fills.
pub(crate) fn note_card_details_skeleton() -> gtk::Box {
    let details = gtk::Box::new(gtk::Orientation::Vertical, 4);
    details.set_hexpand(true);
    details.add_css_class("note-card-details");
    let title = gtk::Label::new(None);
    title.set_xalign(0.0);
    title.add_css_class("note-card-title");
    title.set_ellipsize(gtk::pango::EllipsizeMode::End);
    title.set_single_line_mode(true);
    details.append(&title);
    let excerpt = gtk::Label::new(None);
    excerpt.set_xalign(0.0);
    excerpt.add_css_class("note-card-excerpt");
    excerpt.set_ellipsize(gtk::pango::EllipsizeMode::End);
    excerpt.set_single_line_mode(true);
    excerpt.set_visible(false);
    details.append(&excerpt);
    let metadata = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    metadata.set_margin_top(8);
    metadata.add_css_class("note-card-metadata");
    let category = gtk::Label::new(None);
    category.set_ellipsize(gtk::pango::EllipsizeMode::End);
    category.set_single_line_mode(true);
    category.set_max_width_chars(18);
    category.add_css_class("note-category-pill");
    category.set_visible(false);
    metadata.append(&category);
    let updated = gtk::Label::new(None);
    updated.add_css_class("note-card-updated");
    updated.set_xalign(0.0);
    updated.set_ellipsize(gtk::pango::EllipsizeMode::End);
    updated.set_single_line_mode(true);
    updated.set_hexpand(true);
    metadata.append(&updated);
    details.append(&metadata);
    details
}

/// Projects one note into an existing note-card body.
pub(crate) fn bind_note_card_details(details: &gtk::Box, data: &NoteCardData<'_>) {
    if let Some(title) = find_label_with_class(details.upcast_ref(), "note-card-title") {
        title.set_text(data.title);
        title.set_widget_name(&format!("{}-title:{}", data.name_prefix, data.note_id));
    }
    if let Some(excerpt) = find_label_with_class(details.upcast_ref(), "note-card-excerpt") {
        let excerpt_text = compact_note_excerpt(data.title, data.excerpt);
        if excerpt_text.is_empty() {
            excerpt.set_text("");
            excerpt.set_visible(false);
            excerpt.set_widget_name("");
        } else {
            excerpt.set_text(&excerpt_text);
            excerpt.set_visible(true);
            excerpt.set_widget_name(&format!("{}-excerpt:{}", data.name_prefix, data.note_id));
        }
    }
    if let Some(category) = find_label_with_class(details.upcast_ref(), "note-category-pill") {
        if data.show_category {
            category.set_visible(true);
            category.set_text(data.category_name);
            category.set_widget_name(&format!("{}-category:{}", data.name_prefix, data.note_id));
            let color_class = data.category_color.map(category_color_css_class);
            let mut classes = vec!["note-category-pill"];
            if let Some(color_class) = color_class {
                classes.push(color_class);
            }
            category.set_css_classes(&classes);
        } else {
            category.set_visible(false);
            category.set_widget_name("");
            category.set_css_classes(&["note-category-pill"]);
        }
    }
    if let Some(updated) = find_label_with_class(details.upcast_ref(), "note-card-updated") {
        updated.set_text(&tr_fmt!(
            gettext("Updated {time}"),
            time = relative_update_time(data.updated_at, OffsetDateTime::now_utc())
        ));
        updated.set_widget_name(&format!("{}-updated:{}", data.name_prefix, data.note_id));
    }
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
        return tr_fmt!(
            ngettext(
                "{amount} second ago",
                "{amount} seconds ago",
                u32::try_from(elapsed_seconds).unwrap_or(u32::MAX),
            ),
            amount = elapsed_seconds
        );
    }
    let elapsed_minutes = elapsed_seconds / 60;
    if elapsed_minutes < 60 {
        return tr_fmt!(
            ngettext(
                "{amount} minute ago",
                "{amount} minutes ago",
                u32::try_from(elapsed_minutes).unwrap_or(u32::MAX),
            ),
            amount = elapsed_minutes
        );
    }
    let elapsed_hours = elapsed_minutes / 60;
    if elapsed_hours < 24 {
        return tr_fmt!(
            ngettext(
                "{amount} hour ago",
                "{amount} hours ago",
                u32::try_from(elapsed_hours).unwrap_or(u32::MAX),
            ),
            amount = elapsed_hours
        );
    }
    let day = local_day(updated_at);
    let today = local_day(now);
    if day == today - Duration::days(1) {
        return gettext("Yesterday");
    }
    let format = if day.year() == today.year() {
        "%b %-d"
    } else {
        "%b %-d, %Y"
    };
    glib::DateTime::from_unix_local(updated_at.unix_timestamp())
        .and_then(|datetime| datetime.format(format))
        .map_or_else(|_| day.to_string(), |formatted| formatted.to_string())
}

#[cfg(test)]
mod tests;
