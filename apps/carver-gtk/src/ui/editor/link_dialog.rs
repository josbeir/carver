//! Unified note-link dialog shared by the rich and source editor toolbars.
//!
//! The dialog searches notes for internal link targets and accepts arbitrary external URLs in the
//! same destination field. It follows the GNOME pattern of an `AdwDialog` with a header bar and
//! `AdwPreferencesGroup` rows, showing note matches inline below the fields rather than in a
//! floating popover. Entries stay local to GTK; only the note query and its results are modeled, so
//! the SDK search runs through typed MVU effects.
use crate::mvu::{AppDispatcher, AppMsg, EditorMsg, LinkDialogOrigin, RequestId};
use gettextrs::gettext;
use gtk::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;
use std::cell::Cell;
use std::rc::Rc;

/// Native widgets retained to update and dismiss one link dialog session.
pub(crate) struct LinkDialogHandle {
    dialog: adw::Dialog,
    text: adw::EntryRow,
    address: adw::EntryRow,
    list: gtk::ListBox,
    results: adw::PreferencesGroup,
    dialog_id: RequestId,
    suppress_query: Rc<Cell<bool>>,
}

impl LinkDialogHandle {
    /// Returns whether this handle still represents the requested dialog session.
    pub(crate) fn matches(&self, dialog_id: RequestId) -> bool {
        self.dialog_id == dialog_id
    }

    /// Closes the dialog.
    pub(crate) fn close(&self) {
        self.dialog.close();
    }
}

/// Presents the unified link dialog and returns its live handle.
pub(crate) fn show(
    parent: &gtk::Window,
    dispatcher: &AppDispatcher,
    dialog_id: RequestId,
    origin: &LinkDialogOrigin,
) -> LinkDialogHandle {
    let initial = match origin {
        LinkDialogOrigin::Rich { text, destination } => (text.clone(), destination.clone()),
        LinkDialogOrigin::Source { text, .. } => (text.clone(), String::new()),
    };

    let text = adw::EntryRow::new();
    text.set_title(&gettext("Text"));
    text.set_widget_name("link-dialog-text");
    text.set_show_apply_button(false);
    text.set_text(&initial.0);
    let address = adw::EntryRow::new();
    address.set_title(&gettext("Address"));
    address.set_widget_name("link-dialog-address");
    address.set_show_apply_button(false);
    address.set_text(&initial.1);

    let group = adw::PreferencesGroup::new();
    group.set_title(&gettext("Link"));
    group.add(&text);
    group.add(&address);

    let list = gtk::ListBox::new();
    list.set_widget_name("link-dialog-candidates");
    list.set_selection_mode(gtk::SelectionMode::None);
    list.add_css_class("boxed-list");
    let results = gtk::ScrolledWindow::new();
    results.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    results.set_max_content_height(220);
    results.set_propagate_natural_height(true);
    results.set_child(Some(&list));

    let results_group = adw::PreferencesGroup::new();
    results_group.set_visible(false);
    let results_row = adw::ActionRow::builder()
        .title(gettext("Suggestions"))
        .activatable(false)
        .build();
    results_row.set_child(Some(&results));
    results_group.add(&results_row);

    let page = adw::PreferencesPage::new();
    page.add(&group);
    page.add(&results_group);

    let header = adw::HeaderBar::new();
    let cancel = gtk::Button::with_label(&gettext("Cancel"));
    cancel.set_widget_name("link-dialog-cancel");
    let insert = gtk::Button::with_label(&gettext("Insert"));
    insert.add_css_class("suggested-action");
    insert.set_widget_name("link-dialog-insert");
    header.pack_start(&cancel);
    header.pack_end(&insert);

    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&header);
    toolbar.set_content(Some(&page));

    let dialog = adw::Dialog::builder()
        .title(gettext("Insert Link"))
        .follows_content_size(true)
        .content_width(420)
        .build();
    dialog.set_widget_name("link-dialog");
    dialog.set_child(Some(&toolbar));

    let suppress_query = Rc::new(Cell::new(false));
    let query_dispatcher = dispatcher.clone();
    let query_suppress = Rc::clone(&suppress_query);
    address.connect_changed(move |row| {
        if query_suppress.get() {
            return;
        }
        let _ = query_dispatcher.dispatch(AppMsg::Editor(EditorMsg::LinkDialogQueryChanged(
            row.text().to_string(),
        )));
    });

    let handle = LinkDialogHandle {
        dialog,
        text,
        address,
        list,
        results: results_group,
        dialog_id,
        suppress_query,
    };

    let cancel_dispatcher = dispatcher.clone();
    cancel.connect_clicked(move |_| {
        let _ =
            cancel_dispatcher.dispatch(AppMsg::Editor(EditorMsg::LinkDialogDismissed(dialog_id)));
    });

    let insert_dispatcher = dispatcher.clone();
    let insert_text = handle.text.clone();
    let insert_address = handle.address.clone();
    insert.connect_clicked(move |_| {
        let _ = insert_dispatcher.dispatch(AppMsg::Editor(EditorMsg::LinkDialogConfirmed {
            dialog_id,
            text: insert_text.text().to_string(),
            destination: insert_address.text().to_string(),
        }));
    });

    handle.dialog.present(Some(parent));
    handle.address.grab_focus();
    handle
}

/// Rebuilds the inline candidate list from note-search results.
pub(crate) fn render_candidates(handle: &LinkDialogHandle, notes: &[carver_sdk::NoteSummary]) {
    while let Some(child) = handle.list.first_child() {
        handle.list.remove(&child);
    }
    handle.results.set_visible(!notes.is_empty());
    for note in notes {
        handle.list.append(&candidate_row(handle, note));
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
    button.set_widget_name("link-dialog-candidate");
    button.add_css_class("flat");
    button.set_child(Some(&content));
    button.set_hexpand(true);
    let address = handle.address.clone();
    let text = handle.text.clone();
    let list = handle.list.clone();
    let results = handle.results.clone();
    let suppress = Rc::clone(&handle.suppress_query);
    let target = carver_domain::note_link_destination(note.id);
    let note_title = note.title.clone();
    button.connect_clicked(move |_| {
        suppress.set(true);
        address.set_text(&target);
        if text.text().trim().is_empty() {
            text.set_text(&note_title);
        }
        suppress.set(false);
        while let Some(child) = list.first_child() {
            list.remove(&child);
        }
        results.set_visible(false);
    });
    row.set_child(Some(&button));
    row
}
