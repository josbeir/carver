//! Unified note-link dialog shared by the rich and source editor toolbars.
//!
//! The dialog searches notes for internal link targets and accepts arbitrary external URLs in the
//! same destination field. Its entries stay local to GTK; only the note query and its results are
//! modeled, so the SDK search runs through typed MVU effects.
use crate::mvu::{AppDispatcher, AppMsg, EditorMsg, LinkDialogOrigin, RequestId};
use gettextrs::{gettext, pgettext};
use gtk::prelude::*;
use libadwaita::prelude::*;
use std::cell::Cell;
use std::rc::Rc;

/// Native widgets retained to update and dismiss one link dialog session.
pub(crate) struct LinkDialogHandle {
    dialog: libadwaita::AlertDialog,
    destination: gtk::Entry,
    text: gtk::Entry,
    list: gtk::ListBox,
    popover: gtk::Popover,
    dialog_id: RequestId,
    suppress_query: Rc<Cell<bool>>,
}

impl LinkDialogHandle {
    /// Returns whether this handle still represents the requested dialog session.
    pub(crate) fn matches(&self, dialog_id: RequestId) -> bool {
        self.dialog_id == dialog_id
    }

    /// Closes the dialog and drops its transient popover.
    pub(crate) fn close(&self) {
        self.popover.popdown();
        self.dialog.force_close();
    }
}

/// Presents the unified link dialog and returns its live handle.
pub(crate) fn show(
    parent: &gtk::Window,
    dispatcher: &AppDispatcher,
    dialog_id: RequestId,
    origin: &LinkDialogOrigin,
) -> LinkDialogHandle {
    let fields = gtk::Box::new(gtk::Orientation::Vertical, 8);
    let text = gtk::Entry::new();
    text.set_placeholder_text(Some(&gettext("Link text")));
    let destination = gtk::Entry::new();
    destination.set_placeholder_text(Some(&gettext("Search notes or enter a URL")));
    destination.set_input_purpose(gtk::InputPurpose::Url);
    match origin {
        LinkDialogOrigin::Rich {
            text: initial,
            destination: initial_destination,
        } => {
            text.set_text(initial);
            destination.set_text(initial_destination);
        }
        LinkDialogOrigin::Source { text: initial, .. } => {
            text.set_text(initial);
        }
    }
    fields.append(&gtk::Label::new(Some(&pgettext("link dialog", "Text"))));
    fields.append(&text);
    fields.append(&gtk::Label::new(Some(&gettext("Address"))));
    fields.append(&destination);

    let list = gtk::ListBox::new();
    list.set_widget_name("editor-link-dialog-candidates");
    list.set_selection_mode(gtk::SelectionMode::None);
    list.add_css_class("boxed-list");
    let scroller = gtk::ScrolledWindow::new();
    scroller.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    scroller.set_max_content_height(220);
    scroller.set_propagate_natural_height(true);
    scroller.set_child(Some(&list));
    let popover = gtk::Popover::new();
    popover.set_widget_name("editor-link-dialog-popover");
    popover.set_autohide(false);
    popover.set_has_arrow(false);
    popover.set_parent(&destination);
    popover.set_child(Some(&scroller));

    let dialog = libadwaita::AlertDialog::builder()
        .heading(gettext("Insert Link"))
        .extra_child(&fields)
        .default_response("insert")
        .close_response("cancel")
        .build();
    let cancel = gettext("Cancel");
    let insert = gettext("Insert");
    dialog.add_responses(&[("cancel", cancel.as_str()), ("insert", insert.as_str())]);

    let suppress_query = Rc::new(Cell::new(false));
    let query_dispatcher = dispatcher.clone();
    let query_suppress = Rc::clone(&suppress_query);
    destination.connect_changed(move |entry| {
        if query_suppress.get() {
            return;
        }
        let _ = query_dispatcher.dispatch(AppMsg::Editor(EditorMsg::LinkDialogQueryChanged(
            entry.text().to_string(),
        )));
    });

    let handle = LinkDialogHandle {
        dialog,
        destination,
        text,
        list,
        popover,
        dialog_id,
        suppress_query,
    };

    let response_dispatcher = dispatcher.clone();
    let response_destination = handle.destination.clone();
    let response_text = handle.text.clone();
    handle.dialog.connect_response(None, move |_, response| {
        if response == "insert" {
            let _ = response_dispatcher.dispatch(AppMsg::Editor(EditorMsg::LinkDialogConfirmed {
                dialog_id,
                text: response_text.text().to_string(),
                destination: response_destination.text().to_string(),
            }));
        } else {
            let _ = response_dispatcher
                .dispatch(AppMsg::Editor(EditorMsg::LinkDialogDismissed(dialog_id)));
        }
    });

    handle.dialog.present(Some(parent));
    handle.destination.grab_focus();
    handle
}

/// Rebuilds the candidate list from note-search results.
pub(crate) fn render_candidates(handle: &LinkDialogHandle, notes: &[carver_sdk::NoteSummary]) {
    while let Some(child) = handle.list.first_child() {
        handle.list.remove(&child);
    }
    if notes.is_empty() {
        handle.popover.popdown();
        return;
    }
    for note in notes {
        handle.list.append(&candidate_row(handle, note));
    }
    if !handle.popover.is_visible() {
        handle.popover.popup();
    }
}

fn candidate_row(handle: &LinkDialogHandle, note: &carver_sdk::NoteSummary) -> gtk::ListBoxRow {
    let row = gtk::ListBoxRow::new();
    row.set_activatable(false);
    let content = gtk::Box::new(gtk::Orientation::Vertical, 2);
    content.set_margin_top(8);
    content.set_margin_bottom(8);
    content.set_margin_start(8);
    content.set_margin_end(8);
    let title = gtk::Label::new(Some(&note.title));
    title.set_halign(gtk::Align::Start);
    title.set_hexpand(true);
    title.set_ellipsize(gtk::pango::EllipsizeMode::End);
    let category = gtk::Label::new(Some(&note.category_name));
    category.add_css_class("dim-label");
    category.add_css_class("caption");
    category.set_halign(gtk::Align::Start);
    content.append(&title);
    content.append(&category);
    let button = gtk::Button::new();
    button.set_widget_name("editor-link-dialog-candidate");
    button.add_css_class("flat");
    button.set_child(Some(&content));
    button.set_hexpand(true);
    let destination = handle.destination.clone();
    let text = handle.text.clone();
    let popover = handle.popover.clone();
    let suppress = Rc::clone(&handle.suppress_query);
    let target = carver_domain::note_link_destination(note.id);
    let title = note.title.clone();
    button.connect_clicked(move |_| {
        suppress.set(true);
        destination.set_text(&target);
        if text.text().trim().is_empty() {
            text.set_text(&title);
        }
        suppress.set(false);
        popover.popdown();
    });
    row.set_child(Some(&button));
    row
}
