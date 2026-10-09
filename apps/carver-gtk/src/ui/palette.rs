//! Native adaptive command palette; callbacks only translate input into messages.

use crate::mvu::{
    AppDispatcher, AppModel, AppMsg, LoadState, RequestId,
    palette::{Activation, PaletteMsg, Row, Section, Target},
};
use gettextrs::gettext;
use gtk::prelude::*;
use libadwaita::{self as adw, prelude::*};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

mod catalog;

pub(crate) const ACTION: &str = "win.command-palette";

#[derive(Default)]
pub(crate) struct PaletteView {
    handle: RefCell<Option<Rc<Handle>>>,
    previous_focus: RefCell<Option<gtk::Widget>>,
}

struct Handle {
    id: RequestId,
    dialog: adw::Dialog,
    entry: gtk::SearchEntry,
    list: gtk::ListBox,
    scroll: gtk::ScrolledWindow,
    status: gtk::Label,
    rows: Rc<RefCell<Vec<Row>>>,
    scroll_pending: Cell<bool>,
}

impl PaletteView {
    pub(crate) fn capture(
        &self,
        parent: &adw::ApplicationWindow,
        dispatcher: &AppDispatcher,
        source_selection: std::ops::Range<usize>,
    ) {
        if parent.visible_dialog().is_some() {
            return;
        }
        self.previous_focus
            .replace(gtk::prelude::GtkWindowExt::focus(parent));
        let _ = dispatcher.dispatch(AppMsg::Palette(PaletteMsg::Opened {
            labels: catalog::labels(),
            source_selection,
        }));
    }

    pub(crate) fn render(
        &self,
        parent: &adw::ApplicationWindow,
        dispatcher: &AppDispatcher,
        model: &AppModel,
    ) {
        let Some(palette) = &model.palette else {
            return;
        };
        let current = self.handle.borrow().clone();
        let handle = if let Some(handle) = current.filter(|handle| handle.id == palette.id) {
            handle
        } else {
            let handle = Rc::new(build(parent, dispatcher, palette.id));
            self.handle.replace(Some(Rc::clone(&handle)));
            handle
        };
        if handle.entry.text().as_str() != palette.query {
            handle.entry.set_text(&palette.query);
        }
        let rows = palette.rows();
        if *handle.rows.borrow() != rows {
            handle.rows.replace(rows.clone());
            while let Some(child) = handle.list.first_child() {
                handle.list.remove(&child);
            }
            let mut section = None;
            for row in &rows {
                let widget = result_row(row);
                if section != Some(row.section) {
                    let header = gtk::Label::new(Some(&section_label(row.section)));
                    header.add_css_class("heading");
                    header.set_xalign(0.0);
                    header.set_margin_top(12);
                    header.set_margin_bottom(6);
                    header.set_margin_start(16);
                    header.set_margin_end(16);
                    widget.set_header(Some(&header));
                    section = Some(row.section);
                }
                handle.list.append(&widget);
            }
        }
        let index = rows
            .iter()
            .position(|row| Some(row.target) == palette.selected)
            .and_then(|index| i32::try_from(index).ok());
        let selected = index.and_then(|index| handle.list.row_at_index(index));
        handle.list.select_row(selected.as_ref());
        keep_selection_visible(&handle);
        let status = if matches!(palette.notes, LoadState::Failed(_)) {
            gettext("Could not search notes.")
        } else if !palette.matched || matches!(palette.notes, LoadState::Loading(_)) {
            gettext("Searching…")
        } else if rows.is_empty() {
            gettext("No results")
        } else {
            String::new()
        };
        handle.status.set_text(&status);
        handle.status.set_visible(!status.is_empty());
    }

    pub(crate) fn finish(&self, dispatcher: &AppDispatcher, activation: Option<Activation>) {
        let handle = self.handle.borrow_mut().take();
        let previous = self.previous_focus.borrow_mut().take();
        let Some(handle) = handle else {
            return;
        };
        let dispatcher = dispatcher.clone();
        let complete = move || {
            if let Some(activation) = activation {
                let _ = dispatcher.dispatch(AppMsg::Palette(PaletteMsg::Execute(activation)));
            } else if let Some(previous) = previous.filter(gtk::prelude::WidgetExt::is_mapped) {
                if let Some(root) = previous.root().and_downcast::<gtk::Window>() {
                    gtk::prelude::RootExt::set_focus(&root, Some(&previous));
                }
                previous.grab_focus();
            }
        };
        handle.dialog.force_close();
        glib::idle_add_local_once(complete);
    }
}

fn build(parent: &adw::ApplicationWindow, dispatcher: &AppDispatcher, id: RequestId) -> Handle {
    let dialog = adw::Dialog::builder()
        .title(gettext("Command Palette"))
        .content_width(600)
        .content_height(480)
        .build();
    dialog.set_widget_name("command-palette-dialog");
    let view = adw::ToolbarView::new();
    let header = adw::HeaderBar::new();
    view.add_top_bar(&header);
    let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let entry = gtk::SearchEntry::new();
    entry.set_widget_name("palette-search-entry");
    entry.set_placeholder_text(Some(&gettext("Search commands and notes…")));
    entry.set_hexpand(true);
    entry.set_width_chars(1);
    header.set_title_widget(Some(&entry));
    let scroll = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vexpand(true)
        .build();
    scroll.set_widget_name("palette-scroller");
    let list = gtk::ListBox::new();
    list.set_widget_name("palette-results");
    list.add_css_class("boxed-list");
    list.add_css_class("command-palette-results");
    list.set_activate_on_single_click(true);
    list.set_can_focus(false);
    list.set_adjustment(Some(&scroll.vadjustment()));
    scroll.set_child(Some(&list));
    content.append(&scroll);
    let status = gtk::Label::new(None);
    status.set_widget_name("palette-status");
    status.add_css_class("dim-label");
    status.set_margin_top(8);
    status.set_margin_bottom(8);
    content.append(&status);
    view.set_content(Some(&content));
    dialog.set_child(Some(&view));
    let dispatch = dispatcher.clone();
    entry.connect_changed(move |entry| {
        let _ = dispatch.dispatch(AppMsg::Palette(PaletteMsg::QueryChanged {
            id,
            query: entry.text().to_string(),
        }));
    });
    let dispatch = dispatcher.clone();
    entry.connect_activate(move |_| {
        let _ = dispatch.dispatch(AppMsg::Palette(PaletteMsg::Activate { id, target: None }));
    });
    let dispatch = dispatcher.clone();
    dialog.connect_closed(move |_| {
        let _ = dispatch.dispatch(AppMsg::Palette(PaletteMsg::Dismissed(id)));
    });
    let controller = gtk::EventControllerKey::new();
    controller.set_name(Some("palette-navigation"));
    controller.set_propagation_phase(gtk::PropagationPhase::Capture);
    let dispatch = dispatcher.clone();
    controller.connect_key_pressed(move |_, key, _, _| {
        let message = match key {
            gtk::gdk::Key::Up => PaletteMsg::Move { id, delta: -1 },
            gtk::gdk::Key::Down => PaletteMsg::Move { id, delta: 1 },
            gtk::gdk::Key::Escape => PaletteMsg::Dismissed(id),
            _ => return glib::Propagation::Proceed,
        };
        let _ = dispatch.dispatch(AppMsg::Palette(message));
        glib::Propagation::Stop
    });
    dialog.add_controller(controller);
    let rows = Rc::new(RefCell::new(Vec::<Row>::new()));
    let projected_rows = Rc::clone(&rows);
    let dispatch = dispatcher.clone();
    list.connect_row_activated(move |_, row| {
        let target = usize::try_from(row.index())
            .ok()
            .and_then(|index| projected_rows.borrow().get(index).map(|row| row.target));
        if let Some(target) = target {
            let _ = dispatch.dispatch(AppMsg::Palette(PaletteMsg::Activate {
                id,
                target: Some(target),
            }));
        }
    });
    dialog.set_focus(Some(&entry));
    dialog.present(Some(parent));
    entry.grab_focus();
    Handle {
        id,
        dialog,
        entry,
        list,
        scroll,
        status,
        rows,
        scroll_pending: Cell::new(false),
    }
}

// A query rebuilds rows before GTK allocates them. Repeat the scroll after layout,
// reading the latest selected row so fast arrow presses cannot scroll an old result.
fn keep_selection_visible(handle: &Rc<Handle>) {
    scroll_to_selection(handle);
    if handle.scroll_pending.replace(true) {
        return;
    }
    let weak_handle = Rc::downgrade(handle);
    handle.list.add_tick_callback(move |_, _| {
        let Some(handle) = weak_handle.upgrade() else {
            return glib::ControlFlow::Break;
        };
        if scroll_to_selection(&handle) {
            handle.scroll_pending.set(false);
            glib::ControlFlow::Break
        } else {
            glib::ControlFlow::Continue
        }
    });
}

fn scroll_to_selection(handle: &Handle) -> bool {
    let Some(row) = handle.list.selected_row() else {
        return true;
    };
    let Some(bounds) = row
        .compute_bounds(&handle.list)
        .filter(|bounds| bounds.height() > 0.0)
    else {
        return false;
    };
    let top = f64::from(bounds.y());
    handle
        .scroll
        .vadjustment()
        .clamp_page(top, top + f64::from(bounds.height()));
    true
}

fn result_row(row: &Row) -> gtk::ListBoxRow {
    let widget = gtk::ListBoxRow::new();
    widget.set_focusable(false);
    widget.set_sensitive(!row.disabled);
    let content = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    content.set_margin_start(10);
    content.set_margin_end(10);
    content.set_margin_top(8);
    content.set_margin_bottom(8);
    content.append(&super::icons::palette_icon(&row.icon));
    let labels = gtk::Box::new(gtk::Orientation::Vertical, 2);
    labels.set_hexpand(true);
    let title = match row.target {
        Target::SearchAll => gettext("Search all notes…"),
        Target::Retry => gettext("Retry search"),
        _ => row.title.clone(),
    };
    let title = gtk::Label::new(Some(&title));
    title.set_xalign(0.0);
    title.set_ellipsize(gtk::pango::EllipsizeMode::End);
    title.set_single_line_mode(true);
    labels.append(&title);
    let subtitle = if row.disabled {
        gettext("Temporarily unavailable")
    } else {
        match row.target {
            Target::Category(_) => gettext("Open category"),
            Target::Base(_) => gettext("Open Base"),
            Target::Note(_) => super::browser::compact_note_excerpt(&row.title, &row.subtitle),
            _ => row.subtitle.clone(),
        }
    };
    if !subtitle.is_empty() {
        let label = gtk::Label::new(Some(&subtitle));
        label.add_css_class("dim-label");
        label.set_xalign(0.0);
        label.set_ellipsize(gtk::pango::EllipsizeMode::End);
        label.set_single_line_mode(true);
        labels.append(&label);
    }
    content.append(&labels);
    if row.active {
        content.append(&gtk::Image::from_icon_name("object-select-symbolic"));
    }
    if !row.shortcut.is_empty() {
        let shortcut = adw::ShortcutLabel::new(&row.shortcut);
        shortcut.add_css_class("dim-label");
        content.append(&shortcut);
    }
    if matches!(row.target, Target::Command(command) if command.destructive()) {
        title.add_css_class("error");
    }
    widget.set_child(Some(&content));
    widget
}

fn section_label(section: Section) -> String {
    match section {
        Section::Commands => gettext("Commands"),
        Section::Destinations => gettext("Categories and Bases"),
        Section::Notes => gettext("Notes"),
    }
}

pub(crate) fn install(window: &adw::ApplicationWindow, dispatcher: &AppDispatcher) {
    let action = gtk::gio::SimpleAction::new("command-palette", None);
    let dispatch = dispatcher.clone();
    action.connect_activate(move |_, _| {
        let _ = dispatch.dispatch(AppMsg::Palette(PaletteMsg::OpenRequested));
    });
    window.add_action(&action);
    if let Some(application) = window.application() {
        application.set_accels_for_action(ACTION, &["<Control><Shift>p"]);
    }
    let controller = gtk::EventControllerKey::new();
    controller.set_propagation_phase(gtk::PropagationPhase::Capture);
    controller.set_name(Some("command-palette-shortcut"));
    let dispatch = dispatcher.clone();
    controller.connect_key_pressed(move |_, key, _, modifiers| {
        if matches!(key, gtk::gdk::Key::p | gtk::gdk::Key::P)
            && modifiers
                .contains(gtk::gdk::ModifierType::CONTROL_MASK | gtk::gdk::ModifierType::SHIFT_MASK)
            && !modifiers.contains(gtk::gdk::ModifierType::ALT_MASK)
        {
            let _ = dispatch.dispatch(AppMsg::Palette(PaletteMsg::OpenRequested));
            glib::Propagation::Stop
        } else {
            glib::Propagation::Proceed
        }
    });
    window.add_controller(controller);
}
