//! Outgoing-link and backlink rows for the document sidebar.
use crate::mvu::{AppDispatcher, AppMsg, NavigationMsg};
use gtk::prelude::*;
use libadwaita::{self as adw, prelude::*};

/// Rebuilds one link group from loaded note summaries, using a boxed Adwaita list.
pub(super) fn render_group(
    group: &adw::PreferencesGroup,
    rows: &mut Vec<adw::ActionRow>,
    notes: Option<&[carver_sdk::NoteSummary]>,
    empty_text: &str,
    dispatcher: &AppDispatcher,
) {
    for row in rows.drain(..) {
        group.remove(&row);
    }
    let notes = notes.unwrap_or_default();
    if notes.is_empty() {
        let empty = adw::ActionRow::builder()
            .title(empty_text)
            .activatable(false)
            .build();
        empty.add_css_class("dim-label");
        group.add(&empty);
        rows.push(empty);
        return;
    }
    for note in notes {
        let row = adw::ActionRow::builder()
            .title(gtk::glib::markup_escape_text(&note.title))
            .subtitle(gtk::glib::markup_escape_text(&note.category_name))
            .activatable(true)
            .build();
        let note_id = note.id;
        let dispatcher = dispatcher.clone();
        row.connect_activated(move |_| {
            let _ = dispatcher.dispatch(AppMsg::Navigation(NavigationMsg::OpenNote(note_id)));
        });
        group.add(&row);
        rows.push(row);
    }
}
