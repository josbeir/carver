//! Immutable document-sidebar projection, shared page layout and navigation.
use crate::mvu::{
    AppDispatcher, AppMsg, EditorDocument, EditorMsg, EditorSessionId, PreferencesMsg,
};
use carver_editor_protocol::DocumentTarget;
use gettextrs::{gettext, pgettext};
use gtk::prelude::*;
use libadwaita::{self as adw, prelude::*};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

mod links;
mod media;
mod outline;

type ThumbnailCache =
    std::collections::BTreeMap<String, (std::sync::Arc<Vec<u8>>, gtk::gdk::Texture)>;

pub(super) struct DocumentSidebar {
    pub root: gtk::Box,
    pub container: adw::BreakpointBin,
    pub toggle: gtk::ToggleButton,
    pub add_files: gtk::Button,
    split: adw::OverlaySplitView,
    stack: adw::ViewStack,
    setting_page: Rc<Cell<bool>>,
    outline_stack_page: adw::ViewStackPage,
    media_stack_page: adw::ViewStackPage,
    links_stack_page: adw::ViewStackPage,
    outline: outline::Outline,
    outline_page: Page,
    media_list: gtk::ListBox,
    media_page: Page,
    linked_group: adw::PreferencesGroup,
    linked_rows: RefCell<Vec<adw::ActionRow>>,
    backlinks_group: adw::PreferencesGroup,
    backlinks_rows: RefCell<Vec<adw::ActionRow>>,
    thumbnails: RefCell<ThumbnailCache>,
    rendered_media_files:
        RefCell<std::collections::BTreeMap<String, Option<crate::mvu::MediaFile>>>,
    rendered_links: RefCell<Option<carver_sdk::NoteLinks>>,
    rendered_document: RefCell<Option<(EditorSessionId, u64)>>,
}

impl DocumentSidebar {
    // CONTEXT: Construction wires the three sidebar pages and their switcher in one place so the
    // shared split view, breakpoint, and page ordering stay auditable.
    #[expect(
        clippy::too_many_lines,
        reason = "the sidebar composition keeps its page ordering explicit"
    )]
    pub fn new(
        content: &impl IsA<gtk::Widget>,
        toggle: gtk::ToggleButton,
        dispatcher: &AppDispatcher,
    ) -> Self {
        let root = gtk::Box::new(gtk::Orientation::Vertical, 12);
        root.set_widget_name("editor-document-sidebar");
        root.set_margin_top(12);
        root.set_margin_bottom(12);
        root.set_margin_start(12);
        root.set_margin_end(12);

        let add_files = gtk::Button::with_label(&gettext("Add files…"));
        add_files.set_widget_name("editor-media-add-files");
        add_files.set_icon_name("list-add-symbolic");
        add_files.set_halign(gtk::Align::Start);
        add_files.set_tooltip_text(Some(&gettext("Add files…")));
        add_files.update_property(&[gtk::accessible::Property::Label(&gettext("Add files"))]);

        let stack = adw::ViewStack::new();
        stack.set_widget_name("editor-sidebar-stack");
        stack.set_hhomogeneous(false);
        stack.set_vhomogeneous(false);
        // Instant page switches avoid the crossfade leaving stale pixels on the narrow sidebar.
        stack.set_enable_transitions(false);

        let outline = outline::Outline::new(dispatcher);
        let outline_page = Page::new(
            "outline",
            &gettext("No headings yet"),
            outline.view.upcast_ref(),
        );
        let outline_stack_page = stack.add_titled_with_icon(
            &outline_page.pages,
            Some("outline"),
            &pgettext("document sidebar", "Outline"),
            "view-list-symbolic",
        );

        let media_list = gtk::ListBox::new();
        media_list.set_widget_name("editor-media-list");
        media_list.set_selection_mode(gtk::SelectionMode::Single);
        media_list.set_valign(gtk::Align::Start);
        media_list.add_css_class("boxed-list");
        let media_page = Page::new("media", &gettext("No media yet"), media_list.upcast_ref());
        let media_root = gtk::Box::new(gtk::Orientation::Vertical, 6);
        media_root.set_vexpand(true);
        media_root.append(&media_page.pages);
        media_root.append(&add_files);
        let media_stack_page = stack.add_titled_with_icon(
            &media_root,
            Some("media"),
            &pgettext("document sidebar", "Media"),
            "emblem-photos-symbolic",
        );

        let linked_group = adw::PreferencesGroup::new();
        linked_group.set_widget_name("editor-linked-group");
        linked_group.set_title(&pgettext("document sidebar", "Linked notes"));
        let backlinks_group = adw::PreferencesGroup::new();
        backlinks_group.set_widget_name("editor-backlinks-group");
        backlinks_group.set_title(&pgettext("document sidebar", "Backlinks"));
        let links_root = gtk::ScrolledWindow::new();
        links_root.set_widget_name("editor-links-page");
        links_root.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
        links_root.set_vexpand(true);
        let links_box = gtk::Box::new(gtk::Orientation::Vertical, 12);
        links_box.append(&linked_group);
        links_box.append(&backlinks_group);
        links_root.set_child(Some(&links_box));
        let links_stack_page = stack.add_titled_with_icon(
            &links_root,
            Some("links"),
            &pgettext("document sidebar", "Links"),
            "insert-link-symbolic",
        );

        let switcher = adw::ViewSwitcher::new();
        switcher.set_widget_name("editor-sidebar-switcher");
        switcher.set_policy(adw::ViewSwitcherPolicy::Narrow);
        switcher.set_stack(Some(&stack));
        root.append(&switcher);
        root.append(&stack);

        // Persist the selected page so every note's sidebar restores it.
        let setting_page = Rc::new(Cell::new(false));
        {
            let dispatcher = dispatcher.clone();
            let setting = Rc::clone(&setting_page);
            stack.connect_visible_child_name_notify(move |stack| {
                if setting.get() {
                    return;
                }
                let page = match stack.visible_child_name().as_deref() {
                    Some("media") => carver_config::DocumentSidebarPage::Media,
                    Some("links") => carver_config::DocumentSidebarPage::Links,
                    _ => carver_config::DocumentSidebarPage::Outline,
                };
                let _ = dispatcher.dispatch(AppMsg::Preferences(
                    PreferencesMsg::SetDocumentSidebarPage(page),
                ));
            });
        }

        content.set_hexpand(true);
        content.set_vexpand(true);
        let split = adw::OverlaySplitView::new();
        split.set_widget_name("editor-document-sidebar-split-view");
        split.set_sidebar_position(gtk::PackType::End);
        split.set_min_sidebar_width(240.0);
        split.set_max_sidebar_width(320.0);
        split.set_sidebar_width_fraction(0.25);
        split.set_pin_sidebar(true);
        // Start hidden; the reducer-owned visibility is applied on the first render so a newly
        // created note tab does not flash the sidebar open.
        split.set_show_sidebar(false);
        // Visibility is reducer-owned, including the persisted preference.
        split.set_enable_hide_gesture(false);
        split.set_enable_show_gesture(false);
        split.set_content(Some(content));
        split.set_sidebar(Some(&root));
        let container = adw::BreakpointBin::new();
        container.set_size_request(360, 240);
        container.set_child(Some(&split));
        let breakpoint = adw::Breakpoint::new(adw::BreakpointCondition::new_length(
            adw::BreakpointConditionLengthType::MaxWidth,
            900.0,
            adw::LengthUnit::Px,
        ));
        breakpoint.add_setters(&[(&split, "collapsed", true)]);
        container.add_breakpoint(breakpoint);
        Self {
            root,
            container,
            toggle,
            add_files,
            split,
            stack: stack.clone(),
            setting_page: Rc::clone(&setting_page),
            outline_stack_page,
            media_stack_page,
            links_stack_page,
            outline,
            outline_page,
            media_list,
            media_page,
            linked_group,
            linked_rows: RefCell::default(),
            backlinks_group,
            backlinks_rows: RefCell::default(),
            thumbnails: RefCell::default(),
            rendered_media_files: RefCell::default(),
            rendered_links: RefCell::default(),
            rendered_document: RefCell::default(),
        }
    }

    pub fn render(
        &self,
        document: &EditorDocument,
        page: carver_config::DocumentSidebarPage,
        dispatcher: &AppDispatcher,
    ) {
        let page_name = match page {
            carver_config::DocumentSidebarPage::Outline => "outline",
            carver_config::DocumentSidebarPage::Media => "media",
            carver_config::DocumentSidebarPage::Links => "links",
        };
        if self.stack.visible_child_name().as_deref() != Some(page_name) {
            self.setting_page.set(true);
            self.stack.set_visible_child_name(page_name);
            self.setting_page.set(false);
        }
        let visible = document.document_sidebar.is_visible();
        self.toggle.set_active(visible);
        self.split.set_show_sidebar(visible);
        let label = if visible {
            gettext("Hide document sidebar")
        } else {
            gettext("Show document sidebar")
        };
        self.toggle.set_tooltip_text(Some(&label));
        self.toggle
            .update_property(&[gtk::accessible::Property::Label(&label)]);
        self.add_files
            .set_sensitive(document.mode != carver_config::EditorMode::Rendered);
        let identity = (document.session, document.source_generation);
        let changed = self.rendered_document.borrow().as_ref() != Some(&identity);
        if changed {
            self.outline.rebuild(document);
        }
        if changed || *self.rendered_media_files.borrow() != document.media_files {
            self.update_thumbnails(&document.media_files);
            let thumbnails = self
                .thumbnails
                .borrow()
                .iter()
                .map(|(path, (_, texture))| (path.clone(), texture.clone()))
                .collect();
            media::render_media_list(
                &self.media_list,
                document.analysis.media(),
                dispatcher,
                &document.media_files,
                &thumbnails,
                document.session,
                document.source_generation,
            );
            self.rendered_media_files
                .replace(document.media_files.clone());
        }
        self.rendered_document.replace(Some(identity));
        let ready_links = match &document.links.state {
            crate::mvu::LoadState::Ready(links) => Some(links.clone()),
            _ => None,
        };
        if *self.rendered_links.borrow() != ready_links {
            links::render_group(
                &self.linked_group,
                &mut self.linked_rows.borrow_mut(),
                ready_links.as_ref().map(|links| links.outgoing.as_slice()),
                &gettext("No linked notes yet"),
                dispatcher,
            );
            links::render_group(
                &self.backlinks_group,
                &mut self.backlinks_rows.borrow_mut(),
                ready_links.as_ref().map(|links| links.backlinks.as_slice()),
                &gettext("No backlinks yet"),
                dispatcher,
            );
            self.rendered_links.replace(ready_links.clone());
        }

        let headings = document.analysis.headings().len();
        let media_count = document.analysis.media().len();
        let outgoing = ready_links.as_ref().map_or(0, |links| links.outgoing.len());
        let backlinks = ready_links
            .as_ref()
            .map_or(0, |links| links.backlinks.len());
        set_badge(&self.outline_stack_page, headings);
        set_badge(&self.media_stack_page, media_count);
        set_badge(&self.links_stack_page, outgoing + backlinks);
        self.outline_page.show_empty(headings == 0);
        self.media_page.show_empty(media_count == 0);

        self.outline.select(document.selected_heading, visible);
        self.media_page.select(
            &self.media_list,
            document.selected_media.as_ref().and_then(|range| {
                document
                    .analysis
                    .media()
                    .iter()
                    .position(|item| &item.range == range)
            }),
            visible,
        );
    }

    fn update_thumbnails(
        &self,
        files: &std::collections::BTreeMap<String, Option<crate::mvu::MediaFile>>,
    ) {
        self.thumbnails
            .borrow_mut()
            .retain(|path, _| files.contains_key(path));
        for (path, file) in files {
            let Some(bytes) = file.as_ref().and_then(|file| file.preview.as_ref()) else {
                self.thumbnails.borrow_mut().remove(path);
                continue;
            };
            if self
                .thumbnails
                .borrow()
                .get(path)
                .is_some_and(|(cached, _)| std::sync::Arc::ptr_eq(cached, bytes))
            {
                continue;
            }
            if let Ok(texture) =
                gtk::gdk::Texture::from_bytes(&glib::Bytes::from_owned(bytes.as_ref().clone()))
            {
                self.thumbnails
                    .borrow_mut()
                    .insert(path.clone(), (std::sync::Arc::clone(bytes), texture));
            }
        }
    }
}

/// Sets a page count badge, clearing it when the count is zero.
fn set_badge(page: &adw::ViewStackPage, count: usize) {
    let badge = u32::try_from(count).unwrap_or(u32::MAX);
    page.set_badge_number(badge);
}

/// One switcher page: either its content or a compact empty hint.
struct Page {
    pages: gtk::Stack,
    scroller: gtk::ScrolledWindow,
}

impl Page {
    fn new(name: &str, hint: &str, content: &gtk::Widget) -> Self {
        let scroller = gtk::ScrolledWindow::new();
        scroller.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
        scroller.set_vexpand(true);
        scroller.set_child(Some(content));
        let empty = gtk::Box::new(gtk::Orientation::Vertical, 0);
        empty.set_widget_name(&format!("editor-{name}-empty"));
        empty.set_valign(gtk::Align::Center);
        let hint = gtk::Label::new(Some(hint));
        hint.set_wrap(true);
        hint.set_max_width_chars(28);
        hint.set_justify(gtk::Justification::Center);
        hint.add_css_class("dim-label");
        empty.append(&hint);
        let pages = gtk::Stack::new();
        pages.set_widget_name(&format!("editor-{name}-pages"));
        pages.set_hhomogeneous(false);
        pages.set_vhomogeneous(false);
        pages.set_vexpand(true);
        pages.add_named(&scroller, Some("files"));
        pages.add_named(&empty, Some("empty"));
        Self { pages, scroller }
    }

    fn show_empty(&self, empty: bool) {
        self.pages
            .set_visible_child_name(if empty { "empty" } else { "files" });
    }

    fn select(&self, list: &gtk::ListBox, index: Option<usize>, reveal: bool) {
        let row = index
            .and_then(|index| i32::try_from(index).ok())
            .and_then(|index| list.row_at_index(index));
        let changed = list.selected_row() != row;
        list.select_row(row.as_ref());
        if reveal
            && changed
            && let Some(row) = row
        {
            let list = list.clone();
            let scroller = self.scroller.clone();
            glib::idle_add_local_once(move || {
                if let Some(bounds) = row.compute_bounds(&list) {
                    let adjustment = scroller.vadjustment();
                    adjustment.clamp_page(
                        f64::from(bounds.y()),
                        f64::from(bounds.y() + bounds.height()),
                    );
                }
            });
        }
    }
}

fn connect_activation(
    button: &gtk::Button,
    dispatcher: &AppDispatcher,
    session: EditorSessionId,
    generation: u64,
    target: DocumentTarget,
) {
    let dispatcher = dispatcher.clone();
    button.connect_clicked(move |_| {
        let _ = dispatcher.dispatch(AppMsg::Editor(EditorMsg::FocusDocumentTarget {
            session,
            generation,
            target: target.clone(),
        }));
    });
}
