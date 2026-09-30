//! Imperative GTK rendering adapters for MVU model snapshots.

#[cfg(test)]
mod tests;

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;

use adw::prelude::*;
use gettextrs::{gettext, ngettext, pgettext};
use gtk::prelude::*;
use libadwaita as adw;
use time::{Date, OffsetDateTime};

use crate::mvu::{
    AppDispatcher, AppModel, AppMsg, BrowserModel, EditorSaveState, Effect, LoadState, MoveUndo,
    Route,
};
use crate::ui::browser::{BrowserFeedContext, BrowserFeedItem};

type SidebarRenderer = Box<dyn Fn(&AppModel)>;

/// Builds one independent editor projection tree for a note tab in the given initial mode.
pub(crate) type EditorFactory = Box<
    dyn Fn(
        carver_config::EditorMode,
    ) -> Result<
        (gtk::Widget, crate::ui::editor::EditorViewRefs),
        crate::ui::editor::SourceSyntaxError,
    >,
>;

/// The tabbed content host: a pinned Notes page plus one editor page per open note.
pub(crate) struct Workspace {
    pub(crate) tab_view: adw::TabView,
    pub(crate) notes_page: adw::TabPage,
    pub(crate) tab_bar: adw::TabBar,
    pub(crate) factory: EditorFactory,
    pages: Rc<RefCell<std::collections::BTreeMap<crate::mvu::TabId, adw::TabPage>>>,
    editors: Rc<
        RefCell<
            std::collections::BTreeMap<crate::mvu::TabId, Rc<crate::ui::editor::EditorViewRefs>>,
        >,
    >,
    syncing: Rc<Cell<bool>>,
    closing: Rc<RefCell<std::collections::BTreeSet<crate::mvu::TabId>>>,
    failed: Rc<RefCell<std::collections::BTreeSet<crate::mvu::TabId>>>,
    ready: Rc<RefCell<std::collections::BTreeSet<crate::mvu::TabId>>>,
    attached_bar_parent: RefCell<Option<gtk::Widget>>,
    pinned_identity: RefCell<Option<(String, &'static str)>>,
}

impl Workspace {
    pub(crate) fn new(
        tab_view: adw::TabView,
        notes_page: adw::TabPage,
        tab_bar: adw::TabBar,
        factory: EditorFactory,
    ) -> Self {
        tab_bar.set_view(Some(&tab_view));
        Self {
            tab_view,
            notes_page,
            tab_bar,
            factory,
            pages: Rc::new(RefCell::new(std::collections::BTreeMap::new())),
            editors: Rc::new(RefCell::new(std::collections::BTreeMap::new())),
            syncing: Rc::new(Cell::new(false)),
            closing: Rc::new(RefCell::new(std::collections::BTreeSet::new())),
            failed: Rc::new(RefCell::new(std::collections::BTreeSet::new())),
            ready: Rc::new(RefCell::new(std::collections::BTreeSet::new())),
            attached_bar_parent: RefCell::new(None),
            pinned_identity: RefCell::new(None),
        }
    }

    /// Places the tab bar directly below the active surface's header bar.
    fn attach_tab_bar(&self, surface: &gtk::Widget) {
        let Some(toolbar) = find_toolbar_view(surface) else {
            return;
        };
        let toolbar = toolbar.upcast::<gtk::Widget>();
        if self.attached_bar_parent.borrow().as_ref() == Some(&toolbar) {
            return;
        }
        self.tab_bar.unparent();
        if let Some(toolbar) = toolbar.downcast_ref::<adw::ToolbarView>() {
            toolbar.add_top_bar(&self.tab_bar);
        }
        self.attached_bar_parent.replace(Some(toolbar));
    }

    /// Removes the tab bar from a surface that should not show it.
    fn detach_tab_bar(&self) {
        if self.attached_bar_parent.borrow_mut().take().is_some() {
            self.tab_bar.unparent();
        }
    }

    /// Connects tab-view signals so the model stays authoritative.
    pub(crate) fn connect(&self, dispatcher: &crate::mvu::AppDispatcher) {
        let syncing = Rc::clone(&self.syncing);
        let pages = Rc::clone(&self.pages);
        let dispatcher_select = dispatcher.clone();
        self.tab_view.connect_selected_page_notify(move |view| {
            if syncing.get() {
                return;
            }
            let Some(page) = view.selected_page() else {
                return;
            };
            let tab_id = pages
                .borrow()
                .iter()
                .find(|(_, candidate)| **candidate == page)
                .map(|(id, _)| *id);
            let message = tab_id.map_or(
                crate::mvu::TabsMsg::ActivateNotes,
                crate::mvu::TabsMsg::Activate,
            );
            let _ = dispatcher_select.dispatch(crate::mvu::AppMsg::Tabs(message));
        });

        let syncing = Rc::clone(&self.syncing);
        let pages = Rc::clone(&self.pages);
        let closing = Rc::clone(&self.closing);
        let notes_page = self.notes_page.clone();
        let dispatcher_close = dispatcher.clone();
        self.tab_view.connect_close_page(move |_view, page| {
            if *page == notes_page {
                return glib::Propagation::Stop;
            }
            if !syncing.get() {
                let tab_id = pages
                    .borrow()
                    .iter()
                    .find(|(_, candidate)| **candidate == *page)
                    .map(|(id, _)| *id);
                if let Some(tab_id) = tab_id {
                    let already_closing = {
                        let mut closing = closing.borrow_mut();
                        !closing.insert(tab_id)
                    };
                    if !already_closing {
                        let _ = dispatcher_close
                            .dispatch(crate::mvu::AppMsg::Tabs(crate::mvu::TabsMsg::Close(tab_id)));
                    }
                }
            }
            // The model removes the tab; let the view perform the page close.
            glib::Propagation::Proceed
        });

        let pages_detached = Rc::clone(&self.pages);
        let editors_detached = Rc::clone(&self.editors);
        let closing_detached = Rc::clone(&self.closing);
        let failed_detached = Rc::clone(&self.failed);
        let ready_detached = Rc::clone(&self.ready);
        self.tab_view
            .connect_page_detached(move |_view, page, _position| {
                let tab_id = pages_detached
                    .borrow()
                    .iter()
                    .find(|(_, candidate)| **candidate == *page)
                    .map(|(id, _)| *id);
                if let Some(tab_id) = tab_id {
                    pages_detached.borrow_mut().remove(&tab_id);
                    editors_detached.borrow_mut().remove(&tab_id);
                    closing_detached.borrow_mut().remove(&tab_id);
                    failed_detached.borrow_mut().remove(&tab_id);
                    ready_detached.borrow_mut().remove(&tab_id);
                }
            });

        let syncing = Rc::clone(&self.syncing);
        let pages = Rc::clone(&self.pages);
        let dispatcher_reorder = dispatcher.clone();
        self.tab_view
            .connect_page_reordered(move |_view, page, position| {
                if syncing.get() {
                    return;
                }
                let tab_id = pages
                    .borrow()
                    .iter()
                    .find(|(_, candidate)| **candidate == *page)
                    .map(|(id, _)| *id);
                if let Some(tab_id) = tab_id {
                    let _ = dispatcher_reorder.dispatch(crate::mvu::AppMsg::Tabs(
                        crate::mvu::TabsMsg::Reordered {
                            tab_id,
                            position: note_tab_position(position),
                        },
                    ));
                }
            });

        // Break the tab-view → signal handler → TabPage cycle when the view is destroyed.
        let pages_for_destroy = Rc::clone(&self.pages);
        let editors_for_destroy = Rc::clone(&self.editors);
        let failed_for_destroy = Rc::clone(&self.failed);
        self.tab_view.connect_destroy(move |_| {
            pages_for_destroy.borrow_mut().clear();
            editors_for_destroy.borrow_mut().clear();
            failed_for_destroy.borrow_mut().clear();
        });

        self.install_shortcuts(dispatcher);
    }

    /// Installs the tab-navigation keys before the focused editor sees them.
    ///
    /// `AdwTabView`'s own shortcuts are enabled but unreachable while `WebKit` or the source view
    /// has focus, so navigation is owned here and routed through the reducer.
    fn install_shortcuts(&self, dispatcher: &crate::mvu::AppDispatcher) {
        let controller = gtk::EventControllerKey::new();
        controller.set_name(Some("workspace-tab-shortcuts"));
        controller.set_propagation_phase(gtk::PropagationPhase::Capture);
        let tab_view = self.tab_view.clone();
        let dispatcher = dispatcher.clone();
        controller.connect_key_pressed(move |_, key, _, modifiers| {
            if !modifiers.contains(gtk::gdk::ModifierType::CONTROL_MASK) {
                return glib::Propagation::Proceed;
            }
            let shift = modifiers.contains(gtk::gdk::ModifierType::SHIFT_MASK);
            let message = match (key, shift) {
                (gtk::gdk::Key::Page_Down, true) => {
                    reorder_selected_page(&tab_view, true);
                    None
                }
                (gtk::gdk::Key::Page_Up, true) => {
                    reorder_selected_page(&tab_view, false);
                    None
                }
                (gtk::gdk::Key::Page_Down | gtk::gdk::Key::Tab, false) => {
                    Some(crate::mvu::TabsMsg::ActivateNext)
                }
                (gtk::gdk::Key::Page_Up, false) | (gtk::gdk::Key::Tab, true) => {
                    Some(crate::mvu::TabsMsg::ActivatePrevious)
                }
                _ => return glib::Propagation::Proceed,
            };
            if let Some(message) = message {
                let _ = dispatcher.dispatch(crate::mvu::AppMsg::Tabs(message));
            }
            glib::Propagation::Stop
        });
        self.tab_view.add_controller(controller);
    }
}

/// Moves the selected non-pinned page one position, reporting the result through the tab view.
fn reorder_selected_page(tab_view: &adw::TabView, forward: bool) {
    let Some(page) = tab_view.selected_page() else {
        return;
    };
    if page.is_pinned() {
        return;
    }
    if forward {
        tab_view.reorder_forward(&page);
    } else {
        tab_view.reorder_backward(&page);
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
struct SidebarSnapshot {
    categories: Vec<carver_sdk::CategorySummary>,
    bases: LoadState<Vec<carver_sdk::BaseDefinition>>,
    selected_category: Option<carver_sdk::CategoryId>,
    selection: crate::mvu::SidebarSelection,
}

impl SidebarSnapshot {
    fn from_model(model: &AppModel) -> Option<Self> {
        let LoadState::Ready(categories) = &model.sidebar.state else {
            return None;
        };
        Some(Self {
            categories: categories.clone(),
            bases: model.bases.definitions.state.clone(),
            selected_category: model.selected_category,
            selection: model.sidebar_selection(),
        })
    }
}
struct BrowserProjectionSnapshot {
    browser: BrowserModel,
    selected_category: Option<carver_sdk::CategoryId>,
    sidebar: LoadState<Vec<carver_sdk::CategorySummary>>,
    route: Route,
    today: Date,
}

impl BrowserProjectionSnapshot {
    fn matches(&self, model: &AppModel, today: Date) -> bool {
        self.browser == model.browser
            && self.selected_category == model.selected_category
            && self.sidebar == model.sidebar.state
            && self.route == model.route
            && self.today == today
    }
}

struct BrowserContentRefs<'a> {
    list: &'a gtk::ListView,
    feed_store: &'a gtk::gio::ListStore,
    hero: &'a gtk::Box,
    pages: &'a gtk::Stack,
    empty_new_note: &'a gtk::Button,
}

/// Returns the display title for a note tab, falling back while it loads.
fn tab_display_title(tab: &crate::mvu::NoteTab) -> String {
    if tab.title.trim().is_empty() {
        gettext("Note")
    } else {
        tab.title.clone()
    }
}

/// The name shown for a tab origin, used to attribute note tabs to their surface.
fn tab_origin_label(model: &AppModel, origin: crate::mvu::TabOrigin) -> String {
    match origin {
        crate::mvu::TabOrigin::Browser => gettext("Notes"),
        crate::mvu::TabOrigin::Base(base_id) => base_display_name(model, base_id),
    }
}

/// The saved name of a Base, or a generic label while it is still loading.
fn base_display_name(model: &AppModel, base_id: carver_sdk::BaseId) -> String {
    if let LoadState::Ready(definitions) = &model.bases.definitions.state
        && let Some(base) = definitions.iter().find(|base| base.id == base_id)
    {
        return base.name.clone();
    }
    gettext("Base")
}

/// Title and icon for the pinned root tab, following the surface it hosts.
fn pinned_tab_identity(model: &AppModel) -> (String, &'static str) {
    let route = match model.route {
        Route::Editor => model.editor_return_route,
        route => route,
    };
    match route {
        Route::Base => (
            model.bases.selected.map_or_else(
                || gettext("Base"),
                |base_id| base_display_name(model, base_id),
            ),
            "carver-database-symbolic",
        ),
        Route::Trash => (gettext("Trash"), "user-trash-symbolic"),
        Route::Browser | Route::Editor => (gettext("Notes"), "view-list-symbolic"),
    }
}

/// Reflects the surface hosted by the pinned root tab in its title and icon.
fn sync_pinned_tab(workspace: &Workspace, model: &AppModel) {
    let identity = pinned_tab_identity(model);
    if workspace.pinned_identity.borrow().as_ref() != Some(&identity) {
        workspace.notes_page.set_title(&identity.0);
        workspace
            .notes_page
            .set_icon(Some(&gtk::gio::ThemedIcon::new(identity.1)));
        workspace.pinned_identity.replace(Some(identity));
    }
}

/// Converts an `AdwTabView` page position to a note-tab index.
///
/// `AdwTabView` reports positions in the full page list, which always starts with the pinned
/// Notes page, while `tabs.open` holds only the note tabs.
fn note_tab_position(position: i32) -> usize {
    usize::try_from(position.max(0))
        .unwrap_or(0)
        .saturating_sub(1)
}

/// Creates missing note pages and refreshes titles, loading state, and tooltips.
///
/// Returns the tabs whose editor could not be built so the caller can surface the failure.
fn sync_note_tabs(
    workspace: &Workspace,
    model: &AppModel,
) -> Vec<(crate::mvu::TabId, crate::ui::editor::SourceSyntaxError)> {
    let mut failures = Vec::new();
    for tab in &model.tabs.open {
        if workspace.pages.borrow().contains_key(&tab.id)
            || workspace.failed.borrow().contains(&tab.id)
        {
            continue;
        }
        match (workspace.factory)(model.preferences.editor_mode) {
            Ok((widget, refs)) => {
                let page = workspace.tab_view.append(&widget);
                page.set_title(&tab_display_title(tab));
                page.set_loading(tab_loading(workspace, tab));
                let ready = Rc::clone(&workspace.ready);
                let page_for_ready = page.clone();
                let tab_id = tab.id;
                refs.set_surface_ready_handler(Rc::new(move || {
                    ready.borrow_mut().insert(tab_id);
                    page_for_ready.set_loading(false);
                }));
                workspace.pages.borrow_mut().insert(tab.id, page);
                workspace.editors.borrow_mut().insert(tab.id, Rc::new(refs));
            }
            Err(error) => {
                // Remember the failure so the editor is not rebuilt on every render.
                workspace.failed.borrow_mut().insert(tab.id);
                failures.push((tab.id, error));
            }
        }
    }
    for tab in &model.tabs.open {
        if let Some(page) = workspace.pages.borrow().get(&tab.id) {
            let title = tab_display_title(tab);
            if page.title() != title.as_str() {
                page.set_title(&title);
            }
            let loading = tab_loading(workspace, tab);
            if page.is_loading() != loading {
                page.set_loading(loading);
            }
            let tooltip = tr_fmt!(
                gettext("Opened from {context}"),
                context = tab_origin_label(model, tab.origin)
            );
            if page.tooltip().as_deref() != Some(tooltip.as_str()) {
                page.set_tooltip(&tooltip);
            }
        }
    }
    failures
}

/// Whether a note tab should still show its loading spinner.
///
/// The tab spins while its document loads and until its editor surface finishes its initial load.
fn tab_loading(workspace: &Workspace, tab: &crate::mvu::NoteTab) -> bool {
    tab.loading || !workspace.ready.borrow().contains(&tab.id)
}

/// Renders the Base header actions, including the context-scoped tab cleanup.
fn render_base_header_actions(
    model: &AppModel,
    refs: &crate::ui::bases::BaseViewRefs,
    base_id: carver_sdk::BaseId,
    definition: Option<&carver_sdk::BaseDefinition>,
    dispatcher: &AppDispatcher,
) {
    crate::ui::bases::actions::render_delete(
        &refs.delete,
        definition.filter(|base| !model.bases.deleting.contains(&base.id)),
        dispatcher,
    );
    let has_tabs = model
        .tabs
        .open
        .iter()
        .any(|tab| tab.origin == crate::mvu::TabOrigin::Base(base_id));
    crate::ui::bases::actions::render_close_tabs(
        &refs.close_tabs,
        definition,
        has_tabs,
        dispatcher,
    );
}

/// Finds the toolbar view that owns a surface so the tab bar can sit below its header.
fn find_toolbar_view(widget: &gtk::Widget) -> Option<adw::ToolbarView> {
    if let Some(toolbar) = widget.downcast_ref::<adw::ToolbarView>() {
        return Some(toolbar.clone());
    }
    let mut child = widget.first_child();
    while let Some(current) = child {
        if let Some(toolbar) = find_toolbar_view(&current) {
            return Some(toolbar);
        }
        child = current.next_sibling();
    }
    None
}

/// GTK references used to render the high-level MVU resources.
///
/// This type intentionally owns widgets only. Application state lives in [`AppModel`].
pub struct ViewRefs {
    template_list: RefCell<Option<adw::Dialog>>,
    template_editor: RefCell<Option<crate::ui::templates::EditorHandle>>,
    route_stack: gtk::Stack,
    browser_list: Option<gtk::ListView>,
    browser_feed_store: Option<gtk::gio::ListStore>,
    browser_feed_context: Option<Rc<RefCell<BrowserFeedContext>>>,
    browser_hero: Option<gtk::Box>,
    browser_load_more_available: Option<Rc<Cell<bool>>>,
    browser_rendered_rows: RefCell<Vec<(carver_sdk::NoteId, carver_sdk::Revision)>>,
    browser_rendered_context: RefCell<Option<BrowserFeedContext>>,
    browser_pages: Option<gtk::Stack>,
    browser_search_bar: Option<gtk::SearchBar>,
    browser_search_entry: Option<gtk::SearchEntry>,
    browser_search_toggle: Option<gtk::ToggleButton>,
    browser_empty_new_note_button: Option<gtk::Button>,
    browser_status: adw::StatusPage,
    base: Option<crate::ui::bases::BaseViewRefs>,
    add_dialog: Option<crate::ui::add::AddDialogSlot>,
    trash_list: Option<gtk::ListBox>,
    trash_pages: Option<gtk::Stack>,
    empty_trash_button: Option<gtk::Button>,
    trash_status: adw::StatusPage,
    toast_overlay: Option<adw::ToastOverlay>,
    dispatcher: Option<AppDispatcher>,
    last_notice_revision: Cell<u64>,
    external_change_toast: RefCell<Option<(crate::mvu::EditorSessionId, adw::Toast)>>,
    last_editor_save_error: RefCell<Option<String>>,
    last_undo_move: RefCell<Option<MoveUndo>>,
    last_undo_trash_note: Cell<Option<carver_sdk::NoteId>>,
    last_browser_search_open: Cell<bool>,
    last_browser_snapshot: RefCell<Option<BrowserProjectionSnapshot>>,
    last_trash_snapshot: RefCell<Option<LoadState<carver_sdk::TrashContents>>>,
    sidebar_renderer: Option<SidebarRenderer>,
    editor: RefCell<Option<Rc<crate::ui::editor::EditorViewRefs>>>,
    workspace: Option<Workspace>,
    link_dialog: RefCell<Option<crate::ui::editor::link_dialog::LinkDialogHandle>>,
    last_link_candidates: RefCell<Option<LoadState<Vec<carver_sdk::NoteSummary>>>>,
    source_syntax_dir: Option<PathBuf>,
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
            template_list: RefCell::new(None),
            template_editor: RefCell::new(None),
            route_stack,
            browser_list: None,
            browser_feed_store: None,
            browser_feed_context: None,
            browser_hero: None,
            browser_load_more_available: None,
            browser_rendered_rows: RefCell::new(Vec::new()),
            browser_rendered_context: RefCell::new(None),
            browser_pages: None,
            browser_search_bar: None,
            browser_search_entry: None,
            browser_search_toggle: None,
            browser_empty_new_note_button: None,
            browser_status,
            base: None,
            add_dialog: None,
            trash_list: None,
            trash_pages: None,
            empty_trash_button: None,
            trash_status,
            toast_overlay: None,
            dispatcher: None,
            last_notice_revision: Cell::new(0),
            external_change_toast: RefCell::new(None),
            last_editor_save_error: RefCell::new(None),
            last_undo_move: RefCell::new(None),
            last_undo_trash_note: Cell::new(None),
            last_browser_search_open: Cell::new(false),
            last_browser_snapshot: RefCell::new(None),
            last_trash_snapshot: RefCell::new(None),
            sidebar_renderer: None,
            editor: RefCell::new(None),
            workspace: None,
            link_dialog: RefCell::new(None),
            last_link_candidates: RefCell::new(None),
            source_syntax_dir: None,
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
    pub(crate) fn with_browser(mut self, browser: crate::ui::browser::BrowserViewRefs) -> Self {
        self.browser_list = Some(browser.list);
        self.browser_feed_store = Some(browser.feed_store);
        self.browser_feed_context = Some(browser.feed_context);
        self.browser_hero = Some(browser.hero);
        self.browser_load_more_available = Some(browser.load_more_available);
        self.browser_pages = Some(browser.pages);
        self.browser_search_bar = Some(browser.search_bar);
        self.browser_search_entry = Some(browser.search_entry);
        self.browser_search_toggle = Some(browser.search_toggle);
        self.browser_empty_new_note_button = Some(browser.empty_new_note_button);
        self
    }

    /// Adds the native saved-base grid.
    #[must_use]
    pub(crate) fn with_base(mut self, base: crate::ui::bases::BaseViewRefs) -> Self {
        self.base = Some(base);
        self
    }

    /// Adds the shared Add dialog slot so Base creation can mount into its tab.
    #[must_use]
    pub(crate) fn with_add_dialog(mut self, add_dialog: crate::ui::add::AddDialogSlot) -> Self {
        self.add_dialog = Some(add_dialog);
        self
    }

    /// Uses the complete category-row renderer for changed MVU snapshots.
    #[must_use]
    pub fn with_sidebar_renderer(mut self, renderer: impl Fn(&AppModel) + 'static) -> Self {
        self.sidebar_renderer = Some(Box::new(renderer));
        self
    }

    /// Adds the tabbed workspace created by the composition shell.
    #[must_use]
    pub(crate) fn with_workspace(mut self, workspace: Workspace) -> Self {
        self.workspace = Some(workspace);
        self
    }

    /// Installs one editor directly for display tests that do not exercise the tab host.
    #[cfg(test)]
    #[must_use]
    pub(crate) fn with_editor_for_test(
        mut self,
        editor: crate::ui::editor::EditorViewRefs,
    ) -> Self {
        self.editor = RefCell::new(Some(Rc::new(editor)));
        self
    }

    /// Adds the installed `GtkSourceView` syntax directory used by the properties dialog.
    #[must_use]
    pub(crate) fn with_source_syntax_dir(mut self, source_syntax_dir: PathBuf) -> Self {
        self.source_syntax_dir = Some(source_syntax_dir);
        self
    }

    /// Renders one immutable model snapshot without invoking application actions.
    pub fn render(&self, model: &AppModel) {
        self.rendering.set(true);
        if let Some(child) = match model.route {
            Route::Browser => Some("browser"),
            Route::Base => Some("base"),
            Route::Trash => Some("trash"),
            // Without a tab host, the legacy single-editor stack owns the editor page.
            Route::Editor if self.workspace.is_none() => Some("editor"),
            Route::Editor => None,
        } {
            self.route_stack.set_visible_child_name(child);
        }
        self.render_sidebar(model);
        self.render_browser(model);
        self.render_base(model);
        self.render_trash(model);
        self.render_tabs(model);
        self.render_editor(model);
        self.render_link_dialog(model);
        self.clear_resolved_external_notice(model);
        self.render_notice(model);
        self.render_editor_save_error(model);
        self.render_undo_move(model);
        self.render_undo_trash_note(model);
        self.rendering.set(false);
    }

    /// Reconciles the tab host with the model and selects the active page.
    fn render_tabs(&self, model: &AppModel) {
        let Some(workspace) = &self.workspace else {
            return;
        };
        workspace.syncing.set(true);
        // The strip is shown whenever notes are open, on every surface.
        let show_tabs = !model.tabs.open.is_empty();
        workspace.tab_bar.set_visible(show_tabs);
        // A Base owns its own filtering, so a new note there would be ambiguous.
        let show_new_note = model.can_create_note();
        if let Some(action) = workspace.tab_bar.end_action_widget() {
            action.set_visible(show_new_note);
        }

        // The pinned root tab reflects the surface it currently hosts.
        sync_pinned_tab(workspace, model);

        let stale: Vec<crate::mvu::TabId> = workspace
            .pages
            .borrow()
            .keys()
            .copied()
            .filter(|tab_id| model.note_tab(*tab_id).is_none())
            .collect();
        for tab_id in stale {
            if workspace.closing.borrow().contains(&tab_id) {
                continue;
            }
            let page = workspace.pages.borrow().get(&tab_id).cloned();
            if let Some(page) = page {
                // The close-page handler confirms the close and page-detached cleans up.
                workspace.closing.borrow_mut().insert(tab_id);
                workspace.tab_view.close_page(&page);
            }
        }

        let sync_failures = sync_note_tabs(workspace, model);
        for (tab_id, error) in sync_failures {
            if let Some(toast_overlay) = &self.toast_overlay {
                toast_overlay.add_toast(adw::Toast::new(&tr_fmt!(
                    gettext("Could not open the note editor: {error}"),
                    error = error.to_string()
                )));
            }
            // Drop the unusable tab once the render pass unwinds, so the model does not keep a
            // document-less page the user cannot reach.
            if let Some(dispatcher) = &self.dispatcher {
                let dispatcher = dispatcher.clone();
                glib::idle_add_local_once(move || {
                    let _ = dispatcher
                        .dispatch(crate::mvu::AppMsg::Tabs(crate::mvu::TabsMsg::Close(tab_id)));
                });
            }
        }

        let selected = match model.route {
            Route::Editor => model
                .tabs
                .active
                .and_then(|tab_id| workspace.pages.borrow().get(&tab_id).cloned()),
            Route::Browser | Route::Base | Route::Trash => Some(workspace.notes_page.clone()),
        };
        if let Some(page) = selected
            && workspace.tab_view.selected_page().as_ref() != Some(&page)
        {
            workspace.tab_view.set_selected_page(&page);
        }

        if show_tabs {
            let surface = match model.route {
                Route::Editor => model.tabs.active.and_then(|tab_id| {
                    workspace
                        .pages
                        .borrow()
                        .get(&tab_id)
                        .map(adw::TabPage::child)
                }),
                Route::Browser | Route::Base | Route::Trash => self.route_stack.visible_child(),
            };
            if let Some(surface) = surface {
                workspace.attach_tab_bar(&surface);
            }
        } else {
            workspace.detach_tab_bar();
        }

        let active = match model.route {
            Route::Editor => model.tabs.active,
            Route::Browser | Route::Base | Route::Trash => None,
        };
        let active_editor =
            active.and_then(|tab_id| workspace.editors.borrow().get(&tab_id).cloned());
        self.editor.replace(active_editor);

        workspace.syncing.set(false);
    }

    fn render_link_dialog(&self, model: &AppModel) {
        let candidates = model
            .editor_link_dialog
            .as_ref()
            .map(|dialog| dialog.candidates.state.clone());
        if candidates.is_none() {
            self.last_link_candidates.replace(None);
            // Drop the borrow before closing: closing can emit a response that re-renders.
            let handle = self.link_dialog.borrow_mut().take();
            let editor = self.editor.borrow().clone();
            if let Some(handle) = handle {
                handle.close();
                // Return focus to the editor after Insert or Cancel, like the old dialog did.
                if let Some(editor) = editor {
                    editor.restore_focus_later();
                }
            }
            return;
        }
        if *self.last_link_candidates.borrow() == candidates {
            return;
        }
        self.last_link_candidates.replace(candidates.clone());
        if let (Some(handle), Some(LoadState::Ready(notes))) =
            (self.link_dialog.borrow().as_ref(), candidates)
        {
            crate::ui::editor::link_dialog::render_candidates(handle, &notes);
        }
    }

    fn render_base(&self, model: &AppModel) {
        if model.route != Route::Base {
            return;
        }
        let (Some(refs), Some(base_id), Some(dispatcher)) =
            (&self.base, model.bases.selected, &self.dispatcher)
        else {
            return;
        };
        crate::ui::bases::render_base_search(
            refs,
            model.bases.search_open,
            &model.bases.search_query,
        );
        let LoadState::Ready(definitions) = &model.bases.definitions.state else {
            crate::ui::bases::actions::render_delete(&refs.delete, None, dispatcher);
            crate::ui::bases::actions::render_configure(&refs.configure, None, dispatcher);
            crate::ui::bases::actions::render_close_tabs(&refs.close_tabs, None, false, dispatcher);
            refs.grid.set_sensitive(false);
            if let LoadState::Failed(error) = &model.bases.definitions.state {
                crate::ui::bases::render_base_status(
                    refs,
                    &gettext("Couldn’t load base"),
                    &error.message,
                );
                return;
            }
            if !matches!(model.bases.definitions.state, LoadState::Loading(id)
                if model.bases.definitions_loading_elapsed == Some(id))
            {
                return;
            }
            crate::ui::bases::render_base_status(
                refs,
                &gettext("Loading base…"),
                &gettext("Loading its definition."),
            );
            return;
        };
        let definition = definitions.iter().find(|base| base.id == base_id);
        render_base_header_actions(model, refs, base_id, definition, dispatcher);
        let rows = match &model.bases.rows.state {
            LoadState::Ready(rows) => rows,
            LoadState::Failed(error) => {
                crate::ui::bases::render_base_status(
                    refs,
                    &gettext("Couldn’t load rows"),
                    &error.message,
                );
                return;
            }
            LoadState::Idle | LoadState::Loading(_) => {
                refs.grid.set_sensitive(false);
                if !matches!(model.bases.rows.state, LoadState::Loading(id)
                    if model.bases.rows_loading_elapsed == Some(id))
                {
                    return;
                }
                crate::ui::bases::render_base_status(
                    refs,
                    &gettext("Loading rows…"),
                    &gettext("Refreshing this base."),
                );
                return;
            }
        };
        if let Some(definition) = definitions.iter().find(|base| base.id == base_id) {
            crate::ui::bases::actions::render_configure(
                &refs.configure,
                Some(definition),
                dispatcher,
            );
            let descriptors = match &model.bases.property_descriptors.state {
                LoadState::Ready(items) => items.as_slice(),
                _ => &[],
            };
            crate::ui::bases::render_base(
                refs,
                definition,
                rows,
                descriptors,
                &model.config.document_properties.entries,
                &model.sidebar.state,
                dispatcher,
            );
            refs.grid.set_sensitive(!model.bases.saving_configuration);
            refs.list.set_sensitive(!model.bases.saving_configuration);
            let loading = model.bases.rows_append_request.is_some();
            refs.load_more
                .set_visible(model.bases.rows_has_more || model.bases.rows_append_error.is_some());
            refs.load_more.set_sensitive(!loading);
            refs.load_more.set_label(&if loading {
                gettext("Loading more rows…")
            } else if model.bases.rows_append_error.is_some() {
                gettext("Retry loading rows")
            } else {
                gettext("Load more rows")
            });
        }
    }

    /// Executes a native GTK adapter effect after the runtime has rendered its model snapshot.
    ///
    /// These effects deliberately live outside `render`: rendering remains a projection of the
    /// model and cannot repeat clipboard, dialog, or print work on a later redraw.
    pub(crate) fn run_template_effect(&self, effect: Effect) {
        let Some(dispatcher) = &self.dispatcher else {
            return;
        };
        let Some(parent) = self.route_stack.root().and_downcast::<gtk::Window>() else {
            return;
        };
        match effect {
            Effect::ShowTemplates { templates, purpose } => {
                if purpose == crate::mvu::TemplatePurpose::NewCategory {
                    let host = self
                        .add_dialog
                        .as_ref()
                        .and_then(|slot| slot.borrow().clone());
                    if let Some(host) = host {
                        host.category.set_templates(&templates, None);
                        host.sync_height();
                    }
                    return;
                }
                let previous = self.template_list.borrow_mut().take();
                if let Some(dialog) = previous
                    && dialog.is_mapped()
                {
                    dialog.close();
                }
                if let crate::mvu::TemplatePurpose::Category(category) = purpose {
                    crate::ui::dialogs::show_category_template_dialog(
                        &parent, dispatcher, &category, &templates,
                    );
                } else {
                    let dialog =
                        crate::ui::templates::show_list(&parent, dispatcher, &templates, &purpose);
                    self.template_list.replace(Some(dialog));
                }
            }
            Effect::ShowTemplateEditor {
                preferences,
                request_id,
                original,
                name,
                source,
            } => {
                let Some(syntax) = &self.source_syntax_dir else {
                    return;
                };
                match crate::ui::templates::show_editor(
                    &parent,
                    dispatcher,
                    crate::ui::templates::EditorDraft {
                        id: request_id,
                        original,
                        name: &name,
                        source: &source,
                    },
                    syntax,
                    &preferences,
                ) {
                    Ok(handle) => {
                        self.template_editor.replace(Some(handle));
                    }
                    Err(_) => {
                        let _ = dispatcher.dispatch(AppMsg::Templates(
                            crate::mvu::TemplatesMsg::OpenFailed {
                                request_id,
                                error: crate::mvu::UiError::new(gettext(
                                    "Could not open the template editor.",
                                )),
                            },
                        ));
                    }
                }
            }
            Effect::FinishTemplateEdit { request_id, error } => {
                let handle = self.template_editor.borrow_mut().take();
                if let Some(handle) = handle {
                    if handle.id == request_id {
                        handle.finish(error.as_ref());
                    }
                    if error.is_some() {
                        self.template_editor.replace(Some(handle));
                    }
                }
            }
            _ => {}
        }
    }

    // CONTEXT: Native dialog effects stay in one visible dispatch table so GTK callbacks never
    // bypass the MVU runtime or apply work to a stale dialog session.
    #[expect(
        clippy::too_many_lines,
        reason = "the native effect boundary keeps dialog session routing explicit"
    )]
    pub(crate) fn run_editor_effect(&self, effect: Effect) {
        if let Effect::ShowNewBaseConfiguration {
            dialog_id,
            descriptors,
            default_properties,
        } = &effect
        {
            if let Some(host) = self
                .add_dialog
                .as_ref()
                .and_then(|slot| slot.borrow().clone())
                && let Some(dispatcher) = &self.dispatcher
            {
                let definition = crate::ui::bases::actions::new_base_definition();
                let form = crate::ui::bases::actions::build_base_configuration_form(
                    dispatcher,
                    *dialog_id,
                    &definition,
                    descriptors,
                    default_properties,
                    crate::ui::bases::actions::BaseConfigurationMode::Create,
                );
                while let Some(child) = host.base_slot.first_child() {
                    host.base_slot.remove(&child);
                }
                host.base_slot.append(&form.page);
                host.base_footer.append(&form.save);
                form.name.grab_focus();
                host.base_dialog_id.set(Some(*dialog_id));
                host.base_ready.set(true);
                host.form.replace(Some(form));
                host.sync_height();
                return;
            }
            if let Some(dispatcher) = &self.dispatcher {
                use adw::prelude::*;
                if let Some(parent) = self.route_stack.root().and_downcast::<gtk::Window>() {
                    let (dialog, form) = crate::ui::bases::actions::show_new_configuration_dialog(
                        &parent,
                        dispatcher,
                        *dialog_id,
                        descriptors,
                        default_properties,
                    );
                    if let Some(refs) = &self.base {
                        refs.configuration.replace(Some((*dialog_id, dialog, form)));
                    }
                }
            }
            return;
        }
        if let Effect::ShowBaseConfiguration {
            dialog_id,
            definition,
            descriptors,
            default_properties,
        } = &effect
        {
            if let (Some(refs), Some(dispatcher)) = (&self.base, &self.dispatcher) {
                use adw::prelude::*;
                if let Some(parent) = refs.configure.root().and_downcast::<gtk::Window>() {
                    let mapped = refs
                        .configuration
                        .borrow()
                        .as_ref()
                        .filter(|(_, dialog, _)| dialog.is_mapped())
                        .map(|(_, dialog, _)| dialog.clone());
                    if let Some(dialog) = mapped {
                        dialog.grab_focus();
                        return;
                    }
                    let (dialog, form) = crate::ui::bases::actions::show_configuration_dialog(
                        &parent,
                        dispatcher,
                        *dialog_id,
                        definition,
                        descriptors,
                        default_properties,
                    );
                    refs.configuration.replace(Some((*dialog_id, dialog, form)));
                }
            }
            return;
        }
        if let Effect::FinishBaseConfiguration { success } = effect {
            if let Some(host) = self
                .add_dialog
                .as_ref()
                .and_then(|slot| slot.borrow().clone())
                && host.base_ready.get()
            {
                if let Some(form) = host.form.borrow().as_ref() {
                    form.set_busy(false);
                }
                if success {
                    host.dialog.close();
                }
                return;
            }
            if let Some(refs) = &self.base {
                if success {
                    if let Some((_, dialog, form)) = refs.configuration.take() {
                        crate::ui::bases::actions::finish_configuration(&form, &dialog, true);
                    }
                } else if let Some((_, dialog, form)) = refs.configuration.borrow().as_ref() {
                    crate::ui::bases::actions::finish_configuration(form, dialog, false);
                }
            }
            return;
        }
        if let Effect::UpdateBaseConfigurationPreview { dialog_id, count } = effect {
            if let Some(host) = self
                .add_dialog
                .as_ref()
                .and_then(|slot| slot.borrow().clone())
                && host.base_dialog_id.get() == Some(dialog_id)
                && let Some(form) = host.form.borrow().as_ref()
            {
                crate::ui::bases::actions::render_preview(form.page.upcast_ref(), count);
                return;
            }
            if let Some(refs) = &self.base
                && let Some((current_dialog_id, dialog, _)) = refs.configuration.borrow().as_ref()
                && current_dialog_id == &dialog_id
                && let Some(root) = dialog.child()
            {
                crate::ui::bases::actions::render_preview(&root, count);
            }
            return;
        }
        if let Effect::ShowLinkDialog {
            dialog_id, origin, ..
        } = &effect
        {
            let existing = self.link_dialog.borrow_mut().take();
            if let Some(existing) = existing {
                if existing.matches(*dialog_id) {
                    self.link_dialog.replace(Some(existing));
                    return;
                }
                existing.close();
            }
            if let Some(dispatcher) = &self.dispatcher
                && let Some(parent) = self.route_stack.root().and_downcast::<gtk::Window>()
            {
                let handle =
                    crate::ui::editor::link_dialog::show(&parent, dispatcher, *dialog_id, origin);
                self.link_dialog.replace(Some(handle));
            }
            return;
        }
        if let Effect::ShowExternalEdit { session, deleted } = effect {
            self.show_external_edit(session, deleted);
            return;
        }
        if let Effect::ShowDocumentProperties { request } = &effect {
            if let Some(dispatcher) = &self.dispatcher {
                use adw::prelude::*;
                if let Some(parent) = self.route_stack.root().and_downcast::<gtk::Window>() {
                    crate::ui::editor::properties_dialog::show(
                        &parent,
                        dispatcher,
                        request,
                        self.source_syntax_dir.as_deref(),
                    );
                }
            }
            return;
        }
        let editor = self.editor.borrow().clone();
        let Some(editor) = editor else {
            return;
        };
        match effect {
            Effect::ApplyRichEditorCommand { command } => editor.apply_rich_command(&command),
            Effect::ReloadRichEditor { session, source } => {
                editor.reload_rich_source(session, &source);
            }
            Effect::InsertRichSource {
                session,
                request_id,
                source,
                structured,
                fallback,
                host_initiated,
            } => {
                editor.insert_rich_source(
                    session,
                    request_id,
                    &source,
                    structured,
                    &fallback,
                    host_initiated,
                );
            }
            Effect::SelectEditorSource { session, selection } => {
                editor.select_source_range(session, selection);
            }
            Effect::FocusEditor { session } => editor.focus(session),
            Effect::FocusDocumentTarget {
                session,
                selection,
                generation,
                target,
            } => {
                editor.focus_document_target(session, generation, selection, &target);
            }
            Effect::ShowMediaPreview { session, path } => editor.preview_media(session, &path),
            Effect::ShowMediaDownloadDialog {
                session,
                path,
                label,
            } => editor.show_media_download(session, &path, &label),
            Effect::CopyEditorDocument { request } => editor.copy_document(&request),
            Effect::ShowEditorExportDialog { request } => editor.show_export_dialog(request),
            Effect::ShowEditorExportWarning { request } => editor.show_export_warning(&request),
            Effect::ExportEditorPdf { request } => editor.export_pdf(&request),
            _ => {}
        }
    }

    fn show_external_edit(&self, session: crate::mvu::EditorSessionId, deleted: bool) {
        use adw::prelude::*;
        let (Some(overlay), Some(dispatcher)) = (&self.toast_overlay, &self.dispatcher) else {
            return;
        };
        let toast = adw::Toast::builder()
            .title(if deleted {
                gettext("This note was deleted elsewhere. Your open text is preserved.")
            } else {
                gettext("This note changed elsewhere. Your unsaved edits are preserved.")
            })
            .button_label(if deleted {
                gettext("Close Note…")
            } else {
                gettext("Reload…")
            })
            .timeout(0)
            .build();
        let parent = self.route_stack.clone();
        let dispatcher = dispatcher.clone();
        toast.connect_button_clicked(move |_| {
            let dialog = adw::AlertDialog::builder()
                .heading(if deleted {
                    gettext("Close deleted note?")
                } else {
                    gettext("Reload note?")
                })
                .body(if deleted {
                    gettext("Closing discards your local draft and opens Trash, where you can restore the saved note. Cancel to keep or export your draft first.")
                } else {
                    gettext("Reloading replaces your unsaved edits with the latest saved note.")
                })
                .default_response("cancel")
                .close_response("cancel")
                .build();
            let cancel = gettext("Cancel");
            let reload = if deleted {
                gettext("Discard Draft and Open Trash")
            } else {
                gettext("Discard Edits and Reload")
            };
            dialog.add_responses(&[
                ("cancel", cancel.as_str()),
                ("reload", reload.as_str()),
            ]);
            dialog.set_response_appearance("reload", adw::ResponseAppearance::Destructive);
            let dispatcher = dispatcher.clone();
            dialog.connect_response(None, move |_, response| {
                if response == "reload" {
                    let _ = dispatcher.dispatch(AppMsg::Editor(
                        if deleted { crate::mvu::EditorMsg::CloseDeleted { session } }
                        else { crate::mvu::EditorMsg::ReloadExternal { session } },
                    ));
                } else {
                    let _ = dispatcher.dispatch(AppMsg::Editor(
                        crate::mvu::EditorMsg::KeepExternalDraft { session },
                    ));
                }
            });
            dialog.present(Some(&parent));
        });
        let previous = self
            .external_change_toast
            .replace(Some((session, toast.clone())));
        if let Some((_, previous)) = previous {
            previous.dismiss();
        }
        overlay.add_toast(toast);
    }

    fn clear_resolved_external_notice(&self, model: &AppModel) {
        let resolved = self
            .external_change_toast
            .borrow()
            .as_ref()
            .is_some_and(|(session, _)| {
                model.editor.as_ref().is_none_or(|document| {
                    document.session != *session || document.external_change.is_none()
                })
            });
        if resolved {
            let previous = self.external_change_toast.take();
            if let Some((_, toast)) = previous {
                toast.dismiss();
            }
        }
    }

    /// Reports whether a widget signal was caused by a programmatic render.
    #[must_use]
    pub fn is_rendering(&self) -> bool {
        self.rendering.get()
    }

    fn render_sidebar(&self, model: &AppModel) {
        if let Some(renderer) = &self.sidebar_renderer {
            let Some(snapshot) = SidebarSnapshot::from_model(model) else {
                if matches!(model.sidebar.state, LoadState::Loading(_)) {
                    return;
                }
                if self.last_sidebar_snapshot.borrow_mut().take().is_some() {
                    renderer(model);
                }
                return;
            };
            let changed = self.last_sidebar_snapshot.borrow().as_ref() != Some(&snapshot);
            if changed {
                self.last_sidebar_snapshot.replace(Some(snapshot));
                renderer(model);
            }
        }
    }

    fn render_editor(&self, model: &AppModel) {
        if let Some(editor) = self.editor.borrow().as_ref() {
            editor.render(model);
        }
    }

    fn render_browser(&self, model: &AppModel) {
        if let Some(available) = &self.browser_load_more_available {
            let needs_page = model.browser.has_more || model.browser.append_error.is_some();
            available.set(needs_page && model.browser.append_request.is_none());
        }
        self.render_browser_search(model);
        // Keep the complete previous browser visible until both lists are ready.
        if matches!(model.browser.notes.state, LoadState::Loading(_))
            && !model.browser.loading_indicator_visible
            && model.browser.last_ready_notes.is_some()
        {
            return;
        }
        if !self.browser_projection_changed(model) {
            return;
        }
        let Some(BrowserContentRefs {
            list,
            feed_store,
            hero,
            pages,
            empty_new_note,
        }) = self.browser_content_refs()
        else {
            return;
        };
        crate::ui::browser::render_category_hero(
            hero,
            &model.sidebar.state,
            model.selected_category,
            self.dispatcher.as_ref(),
        );
        match &model.browser.notes.state {
            LoadState::Ready(notes)
                if notes.is_empty() && !model.browser.search_query.trim().is_empty() =>
            {
                self.render_browser_special(feed_store, model, BrowserFeedItem::SearchEmpty);
                empty_new_note.set_visible(false);
                pages.set_visible_child_name("contents");
            }
            LoadState::Ready(notes) if notes.is_empty() && model.selected_category.is_some() => {
                self.render_browser_special(feed_store, model, BrowserFeedItem::CategoryEmpty);
                empty_new_note.set_visible(false);
                pages.set_visible_child_name("contents");
            }
            LoadState::Ready(notes) if notes.is_empty() => {
                feed_store.remove_all();
                self.browser_rendered_rows.borrow_mut().clear();
                self.browser_rendered_context.replace(None);
                empty_new_note.set_visible(true);
                self.browser_status.set_title(&gettext("No notes yet"));
                self.browser_status
                    .set_description(Some(&gettext("Create a note to get started.")));
                pages.set_visible_child_name("empty");
            }
            LoadState::Ready(notes) => {
                self.render_browser_notes(list, feed_store, pages, empty_new_note, notes, model);
            }
            state => {
                feed_store.remove_all();
                self.browser_rendered_rows.borrow_mut().clear();
                self.browser_rendered_context.replace(None);
                empty_new_note.set_visible(false);
                self.browser_status
                    .set_title(&resource_label(state, "No notes yet"));
                if let LoadState::Failed(error) = state {
                    self.browser_status.set_description(Some(&error.message));
                }
                pages.set_visible_child_name("empty");
            }
        }
    }

    fn browser_projection_changed(&self, model: &AppModel) -> bool {
        let today = crate::ui::browser::local_day(OffsetDateTime::now_utc());
        if self
            .last_browser_snapshot
            .borrow()
            .as_ref()
            .is_some_and(|snapshot| snapshot.matches(model, today))
        {
            return false;
        }
        self.last_browser_snapshot
            .replace(Some(browser_projection_snapshot(model, today)));
        true
    }

    fn browser_content_refs(&self) -> Option<BrowserContentRefs<'_>> {
        Some(BrowserContentRefs {
            list: self.browser_list.as_ref()?,
            feed_store: self.browser_feed_store.as_ref()?,
            hero: self.browser_hero.as_ref()?,
            pages: self.browser_pages.as_ref()?,
            empty_new_note: self.browser_empty_new_note_button.as_ref()?,
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
        if self.last_trash_snapshot.borrow().as_ref() == Some(&model.trash.state) {
            return;
        }
        self.last_trash_snapshot
            .replace(Some(model.trash.state.clone()));
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
                self.trash_status.set_title(&gettext("Trash is empty"));
                self.trash_status.set_description(Some(&gettext(
                    "Deleted notes and categories can be restored here.",
                )));
                empty_button.set_sensitive(false);
                pages.set_visible_child_name("empty");
            }
            LoadState::Ready(contents) => {
                empty_button.set_sensitive(true);
                if !contents.categories.is_empty() {
                    append_section_heading(list, &pgettext("Trash section", "Categories"));
                    for category in &contents.categories {
                        list.append(&trashed_category_row(category));
                    }
                }
                if !contents.notes.is_empty() {
                    append_section_heading(list, &pgettext("Trash section", "Notes"));
                    for note in &contents.notes {
                        list.append(&trashed_note_row(note));
                    }
                }
                pages.set_visible_child_name("contents");
            }
            state => {
                empty_button.set_sensitive(false);
                self.trash_status
                    .set_title(&resource_label(state, "Trash is empty"));
                if let LoadState::Failed(error) = state {
                    self.trash_status.set_description(Some(&error.message));
                }
                pages.set_visible_child_name("empty");
            }
        }
    }

    fn render_notice(&self, model: &AppModel) {
        let Some(error) = &model.notice else {
            return;
        };
        if self.last_notice_revision.get() == model.notice_revision {
            return;
        }
        if let Some(toast_overlay) = &self.toast_overlay {
            toast_overlay.add_toast(adw::Toast::new(&error.message));
            self.last_notice_revision.set(model.notice_revision);
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
            let toast = adw::Toast::new(&tr_fmt!(
                gettext("Could not save note: {error}"),
                error = error.message
            ));
            toast.set_button_label(Some(&gettext("Retry")));
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
            let toast = adw::Toast::new(&gettext("Moved note"));
            toast.set_button_label(Some(&gettext("Undo")));
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
            let toast = adw::Toast::new(&gettext("Moved note to Trash"));
            toast.set_button_label(Some(&gettext("Undo")));
            toast.set_action_name(Some("mvu.undo-trash-note"));
            toast_overlay.add_toast(toast);
        }
    }
}

fn browser_projection_snapshot(model: &AppModel, today: Date) -> BrowserProjectionSnapshot {
    BrowserProjectionSnapshot {
        browser: model.browser.clone(),
        selected_category: model.selected_category,
        sidebar: model.sidebar.state.clone(),
        route: model.route,
        today,
    }
}

impl ViewRefs {
    fn render_browser_special(
        &self,
        feed_store: &gtk::gio::ListStore,
        model: &AppModel,
        item: BrowserFeedItem,
    ) {
        let context = browser_feed_context(model);
        if let Some(feed_context) = &self.browser_feed_context {
            feed_context.replace(context.clone());
        }
        feed_store.remove_all();
        feed_store.append(&glib::BoxedAnyObject::new(item));
        self.browser_rendered_rows.borrow_mut().clear();
        // The special card replaces the note rows, so no rendered row prefix
        // remains. Clearing the context forces the next note render to rebuild
        // the feed instead of appending after the stale empty-state card.
        self.browser_rendered_context.replace(None);
    }

    fn render_browser_notes(
        &self,
        _list: &gtk::ListView,
        feed_store: &gtk::gio::ListStore,
        pages: &gtk::Stack,
        empty_new_note: &gtk::Button,
        notes: &[carver_sdk::NoteSummary],
        model: &AppModel,
    ) {
        empty_new_note.set_visible(true);
        pages.set_visible_child_name("contents");
        let context = browser_feed_context(model);
        if let Some(feed_context) = &self.browser_feed_context {
            feed_context.replace(context.clone());
        }
        let show_date_groups = model.browser.search_query.trim().is_empty();
        let existing_context = self.browser_rendered_context.borrow().clone();
        let incoming: Vec<_> = notes.iter().map(|note| (note.id, note.revision)).collect();
        let mut rendered = self.browser_rendered_rows.borrow_mut();
        let can_append = existing_context.as_ref() == Some(&context)
            && notes.len() >= rendered.len()
            && incoming
                .get(..rendered.len())
                .is_some_and(|prefix| prefix == rendered.as_slice());
        if can_append {
            remove_browser_footer(feed_store);
        } else {
            feed_store.remove_all();
            rendered.clear();
            append_browser_prelude(feed_store, model);
        }
        let now = OffsetDateTime::now_utc();
        let mut previous_group = rendered.last().and_then(|(id, _)| {
            notes
                .iter()
                .find(|note| note.id == *id)
                .map(|note| crate::ui::browser::note_date_group(note.updated_at, now))
        });
        for note in &notes[rendered.len()..] {
            if show_date_groups {
                let group = crate::ui::browser::note_date_group(note.updated_at, now);
                if previous_group != Some(group) {
                    feed_store.append(&glib::BoxedAnyObject::new(BrowserFeedItem::Heading(group)));
                    previous_group = Some(group);
                }
            }
            feed_store.append(&glib::BoxedAnyObject::new(BrowserFeedItem::Note(
                note.clone(),
            )));
        }
        append_browser_footer(feed_store, model);
        *rendered = incoming;
        self.browser_rendered_context.replace(Some(context));
    }
}

fn browser_feed_context(model: &AppModel) -> BrowserFeedContext {
    let favorites = match &model.browser.favorites.state {
        LoadState::Ready(notes) if model.browser.search_query.trim().is_empty() => {
            notes.iter().map(|note| (note.id, note.revision)).collect()
        }
        _ => Vec::new(),
    };
    BrowserFeedContext {
        show_category: model.selected_category.is_none(),
        sidebar: model.sidebar.state.clone(),
        selected_category: model.selected_category,
        favorites,
    }
}

fn append_browser_prelude(feed_store: &gtk::gio::ListStore, model: &AppModel) {
    if model.browser.search_query.trim().is_empty()
        && let LoadState::Ready(notes) = &model.browser.favorites.state
        && !notes.is_empty()
    {
        feed_store.append(&glib::BoxedAnyObject::new(
            BrowserFeedItem::FavoritesHeading,
        ));
        for note in notes {
            feed_store.append(&glib::BoxedAnyObject::new(BrowserFeedItem::Favorite(
                note.clone(),
            )));
        }
    }
}

fn append_browser_footer(feed_store: &gtk::gio::ListStore, model: &AppModel) {
    let loading = model.browser.append_request.is_some();
    if model.browser.has_more || model.browser.append_error.is_some() {
        let label = if loading {
            gettext("Loading more notes…")
        } else if model.browser.append_error.is_some() {
            gettext("Retry loading notes")
        } else {
            gettext("Load more notes")
        };
        feed_store.append(&glib::BoxedAnyObject::new(BrowserFeedItem::LoadMore {
            label: label.clone(),
            sensitive: !loading,
        }));
    }
}

fn remove_browser_footer(feed_store: &gtk::gio::ListStore) {
    let Some(position) = feed_store.n_items().checked_sub(1) else {
        return;
    };
    let Some(item) = feed_store
        .item(position)
        .and_downcast::<glib::BoxedAnyObject>()
    else {
        return;
    };
    if matches!(
        &*item.borrow::<BrowserFeedItem>(),
        BrowserFeedItem::LoadMore { .. }
    ) {
        feed_store.remove(position);
    }
}

fn render_resource<T>(status: &adw::StatusPage, resource: &LoadState<T>, empty_title: &str) {
    match resource {
        LoadState::Idle | LoadState::Ready(_) => status.set_title(&gettext(empty_title)),
        LoadState::Loading(_) => status.set_title(&gettext("Loading…")),
        LoadState::Failed(error) => {
            status.set_title(&gettext("Could not load content"));
            status.set_description(Some(&error.message));
        }
    }
}

fn resource_label<T>(resource: &LoadState<T>, empty: &str) -> String {
    match resource {
        LoadState::Idle | LoadState::Ready(_) => gettext(empty),
        LoadState::Loading(_) => gettext("Loading…"),
        LoadState::Failed(_) => gettext("Could not load content"),
    }
}

fn append_section_heading(list: &gtk::ListBox, text: &str) {
    let label = gtk::Label::new(Some(text));
    label.set_xalign(0.0);
    label.add_css_class("date-heading-label");
    append_section_heading_widget(list, &label);
}

fn append_section_heading_widget(list: &gtk::ListBox, child: &impl IsA<gtk::Widget>) {
    let row = gtk::ListBoxRow::new();
    row.set_selectable(false);
    row.add_css_class("date-heading");
    row.set_child(Some(child));
    list.append(&row);
}

fn trashed_category_row(category: &carver_sdk::TrashedCategorySummary) -> adw::ActionRow {
    let row = adw::ActionRow::new();
    row.set_widget_name(&format!("trashed-category:{}", category.category.id));
    row.add_css_class("card");
    row.add_css_class("note-card");
    row.set_use_markup(false);
    row.set_title(&category.category.name);
    row.set_subtitle(&tr_fmt!(
        ngettext(
            "{count} recoverable note",
            "{count} recoverable notes",
            u32::try_from(category.recoverable_note_count).unwrap_or(u32::MAX),
        ),
        count = category.recoverable_note_count
    ));
    row.add_suffix(&restore_button(
        &format!("restore-category:{}", category.category.id),
        "trash.restore-category",
        &category.category.id.to_string(),
    ));
    row
}

fn trashed_note_row(note: &carver_sdk::TrashedNoteSummary) -> adw::ActionRow {
    let row = adw::ActionRow::new();
    row.set_widget_name(&format!("trashed-note:{}", note.id));
    row.add_css_class("card");
    row.add_css_class("note-card");
    row.set_use_markup(false);
    row.set_title(&note.title);
    let excerpt_text = crate::ui::browser::compact_note_excerpt(&note.title, &note.excerpt);
    if !excerpt_text.is_empty() {
        row.set_subtitle(&excerpt_text);
    }
    row.add_suffix(&restore_button(
        &format!("restore-note:{}", note.id),
        "trash.restore-note",
        &note.id.to_string(),
    ));
    row
}

fn restore_button(name: &str, action: &str, target: &str) -> gtk::Button {
    let restore = gtk::Button::from_icon_name("edit-undo-symbolic");
    restore.set_widget_name(name);
    restore.add_css_class("flat");
    restore.set_tooltip_text(Some(&gettext("Restore")));
    restore.update_property(&[gtk::accessible::Property::Label(&gettext("Restore"))]);
    restore.set_valign(gtk::Align::Center);
    restore.set_action_name(Some(action));
    restore.set_action_target_value(Some(&target.to_variant()));
    restore
}
