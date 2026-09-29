//! Outgoing-link and backlink cards for the document sidebar.
use crate::mvu::{AppDispatcher, AppMsg, NavigationMsg};
use gtk::prelude::*;

/// Rebuilds one sidebar link list from loaded note summaries.
pub(super) fn render_note_list(
    list: &gtk::ListBox,
    notes: Option<&[carver_sdk::NoteSummary]>,
    name: &str,
    dispatcher: &AppDispatcher,
) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }
    let Some(notes) = notes else {
        return;
    };
    for note in notes {
        list.append(&note_row(note, name, dispatcher));
    }
}

fn note_row(
    note: &carver_sdk::NoteSummary,
    name: &str,
    dispatcher: &AppDispatcher,
) -> gtk::ListBoxRow {
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
    category.set_ellipsize(gtk::pango::EllipsizeMode::End);
    content.append(&title);
    content.append(&category);
    let button = gtk::Button::new();
    button.set_widget_name(name);
    button.add_css_class("flat");
    button.set_child(Some(&content));
    button.set_hexpand(true);
    let note_id = note.id;
    let dispatcher = dispatcher.clone();
    button.connect_clicked(move |_| {
        let _ = dispatcher.dispatch(AppMsg::Navigation(NavigationMsg::OpenNote(note_id)));
    });
    row.set_child(Some(&button));
    row
}
