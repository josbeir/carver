//! Immutable document-sidebar projection, shared section layout and navigation.
use crate::mvu::{AppDispatcher, AppMsg, EditorDocument, EditorMsg, EditorSessionId};
use carver_editor_protocol::DocumentTarget;
use gtk::prelude::*;
use libadwaita::{self as adw, prelude::*};
use std::cell::RefCell;

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
    outline: outline::Outline,
    outline_section: Section,
    media_list: gtk::ListBox,
    media: Section,
    thumbnails: RefCell<ThumbnailCache>,
    rendered_media_files:
        RefCell<std::collections::BTreeMap<String, Option<crate::mvu::MediaFile>>>,
    rendered_document: RefCell<Option<(EditorSessionId, u64)>>,
}

impl DocumentSidebar {
    pub fn new(
        content: &gtk::Stack,
        toggle: gtk::ToggleButton,
        dispatcher: &AppDispatcher,
    ) -> Self {
        let root = gtk::Box::new(gtk::Orientation::Vertical, 12);
        root.set_widget_name("editor-document-sidebar");
        root.set_margin_top(12);
        root.set_margin_bottom(12);
        root.set_margin_start(12);
        root.set_margin_end(12);
        let add_files = gtk::Button::from_icon_name("list-add-symbolic");
        add_files.set_widget_name("editor-media-add-files");
        add_files.add_css_class("flat");
        add_files.set_tooltip_text(Some("Add files…"));
        add_files.update_property(&[gtk::accessible::Property::Label("Add files")]);
        let outline = outline::Outline::new(dispatcher);
        let outline_section = Section::new(
            "Outline",
            "outline",
            "No headings yet",
            "Add headings to navigate your document.",
            None,
            outline.view.upcast_ref(),
        );
        let media_list = gtk::ListBox::new();
        media_list.set_widget_name("editor-media-list");
        media_list.set_selection_mode(gtk::SelectionMode::Single);
        media_list.set_valign(gtk::Align::Start);
        media_list.add_css_class("boxed-list");
        let media = Section::new(
            "Media",
            "media",
            "No media yet",
            "Use + to add files, or drag them into the document.",
            Some(&add_files),
            media_list.upcast_ref(),
        );
        root.append(&outline_section.root);
        root.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
        root.append(&media.root);
        content.set_hexpand(true);
        content.set_vexpand(true);
        let split = adw::OverlaySplitView::new();
        split.set_widget_name("editor-document-sidebar-split-view");
        split.set_sidebar_position(gtk::PackType::End);
        split.set_min_sidebar_width(240.0);
        split.set_max_sidebar_width(320.0);
        split.set_sidebar_width_fraction(0.25);
        split.set_pin_sidebar(true);
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
            outline,
            outline_section,
            media_list,
            media,
            thumbnails: RefCell::default(),
            rendered_media_files: RefCell::default(),
            rendered_document: RefCell::default(),
        }
    }

    pub fn render(&self, document: &EditorDocument, dispatcher: &AppDispatcher) {
        let visible = document.document_sidebar.is_visible();
        self.toggle.set_active(visible);
        self.split.set_show_sidebar(visible);
        let label = if visible {
            "Hide document sidebar"
        } else {
            "Show document sidebar"
        };
        self.toggle.set_tooltip_text(Some(label));
        self.toggle
            .update_property(&[gtk::accessible::Property::Label(label)]);
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
        self.outline_section
            .show_empty(document.analysis.headings().is_empty());
        self.media.show_empty(document.analysis.media().is_empty());
        self.outline.select(document.selected_heading, visible);
        self.media.select(
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

struct Section {
    root: gtk::Box,
    pages: gtk::Stack,
    scroller: gtk::ScrolledWindow,
}

impl Section {
    fn new(
        title: &str,
        name: &str,
        empty_title: &str,
        hint: &str,
        action: Option<&gtk::Button>,
        content: &gtk::Widget,
    ) -> Self {
        let root = gtk::Box::new(gtk::Orientation::Vertical, 8);
        root.set_vexpand(true);
        let header = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        let label = gtk::Label::new(Some(title));
        label.add_css_class("title-4");
        label.set_halign(gtk::Align::Start);
        label.set_hexpand(true);
        header.append(&label);
        if let Some(action) = action {
            header.append(action);
        }
        root.append(&header);
        let scroller = gtk::ScrolledWindow::new();
        scroller.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
        scroller.set_vexpand(true);
        scroller.set_child(Some(content));
        let empty = gtk::Box::new(gtk::Orientation::Vertical, 8);
        empty.set_widget_name(&format!("editor-{name}-empty"));
        empty.set_valign(gtk::Align::Center);
        let title = gtk::Label::new(Some(empty_title));
        title.add_css_class("heading");
        let hint = gtk::Label::new(Some(hint));
        hint.set_wrap(true);
        hint.set_max_width_chars(28);
        hint.set_justify(gtk::Justification::Center);
        hint.add_css_class("dim-label");
        empty.append(&title);
        empty.append(&hint);
        let pages = gtk::Stack::new();
        pages.set_widget_name(&format!("editor-{name}-pages"));
        pages.set_hhomogeneous(false);
        pages.set_vhomogeneous(false);
        pages.set_vexpand(true);
        pages.add_named(&scroller, Some("files"));
        pages.add_named(&empty, Some("empty"));
        root.append(&pages);
        Self {
            root,
            pages,
            scroller,
        }
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
