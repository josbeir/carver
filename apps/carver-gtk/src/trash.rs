//! In-app trash browser and recovery actions.

use std::rc::Rc;

use carver_sdk::{TrashedCategorySummary, TrashedNoteSummary};
use gtk::prelude::*;
use libadwaita as adw;

use crate::{browser::refresh_browser, controller::AppState, sidebar::refresh_sidebar};

/// Builds the recoverable in-app trash page.
pub(crate) fn build_trash(
    state: &Rc<AppState>,
    stack: &gtk::Stack,
    toast_overlay: &adw::ToastOverlay,
) -> gtk::Widget {
    let view = adw::ToolbarView::new();
    let header = adw::HeaderBar::new();
    let back = gtk::Button::from_icon_name("go-previous-symbolic");
    back.set_widget_name("back-from-trash-button");
    back.set_tooltip_text(Some("Back to notes"));
    let stack_for_back = stack.clone();
    back.connect_clicked(move |_| stack_for_back.set_visible_child_name("browser"));
    header.pack_start(&back);
    header.set_title_widget(Some(&adw::WindowTitle::new(
        "Trash",
        "Restore notes and categories",
    )));
    let empty = gtk::Button::with_label("Empty Trash");
    empty.set_widget_name("empty-trash-button");
    empty.add_css_class("destructive-action");
    header.pack_end(&empty);
    view.add_top_bar(&header);

    let pages = gtk::Stack::new();
    let status = adw::StatusPage::builder()
        .title("Trash is empty")
        .description("Deleted notes and categories can be restored here.")
        .icon_name("user-trash-symbolic")
        .build();
    pages.add_named(&status, Some("empty"));
    let list = gtk::ListBox::new();
    list.set_widget_name("trash-list");
    list.set_selection_mode(gtk::SelectionMode::None);
    list.add_css_class("note-feed");
    let scroll = gtk::ScrolledWindow::new();
    scroll.set_vexpand(true);
    scroll.set_child(Some(&list));
    let clamp = adw::Clamp::new();
    clamp.set_maximum_size(720);
    clamp.set_tightening_threshold(520);
    clamp.set_child(Some(&scroll));
    pages.add_named(&clamp, Some("contents"));
    view.set_content(Some(&pages));

    state.trash_list.replace(Some(list));
    state.trash_content_stack.replace(Some(pages));
    state.trash_status.replace(Some(status));
    state.empty_trash_button.replace(Some(empty.clone()));
    connect_empty_action(state, toast_overlay, &empty);
    refresh_trash(state);
    view.upcast()
}

/// Refreshes visible trash data after a recovery or deletion action.
pub(crate) fn refresh_trash(state: &Rc<AppState>) {
    let (Some(list), Some(pages), Some(status), Some(empty_button)) = (
        state.trash_list.borrow().clone(),
        state.trash_content_stack.borrow().clone(),
        state.trash_status.borrow().clone(),
        state.empty_trash_button.borrow().clone(),
    ) else {
        return;
    };
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }
    match state.client.trash_contents() {
        Ok(contents) if contents.is_empty() => {
            status.set_title("Trash is empty");
            status.set_description(Some("Deleted notes and categories can be restored here."));
            empty_button.set_sensitive(false);
            pages.set_visible_child_name("empty");
        }
        Ok(contents) => {
            empty_button.set_sensitive(true);
            if !contents.categories.is_empty() {
                append_section_heading(&list, "Categories");
                for category in &contents.categories {
                    list.append(&trashed_category_row(state, category));
                }
            }
            if !contents.notes.is_empty() {
                append_section_heading(&list, "Notes");
                for note in &contents.notes {
                    list.append(&trashed_note_row(state, note));
                }
            }
            pages.set_visible_child_name("contents");
        }
        Err(error) => {
            status.set_title("Trash could not be loaded");
            status.set_description(Some(&error.to_string()));
            empty_button.set_sensitive(false);
            pages.set_visible_child_name("empty");
        }
    }
}

fn connect_empty_action(
    state: &Rc<AppState>,
    toast_overlay: &adw::ToastOverlay,
    button: &gtk::Button,
) {
    let state_for_empty = Rc::clone(state);
    let toast_for_empty = toast_overlay.clone();
    button.connect_clicked(move |button| {
        let dialog = gtk::Dialog::builder()
            .modal(true)
            .title("Empty Trash?")
            .build();
        if let Some(parent) = button
            .root()
            .and_then(|root| root.downcast::<gtk::Window>().ok())
        {
            dialog.set_transient_for(Some(&parent));
        }
        dialog.add_button("Cancel", gtk::ResponseType::Cancel);
        let empty_button = dialog.add_button("Empty Trash", gtk::ResponseType::Accept);
        empty_button.add_css_class("destructive-action");
        let warning = gtk::Label::new(Some(
            "All trashed notes, categories, and unreferenced images will be permanently deleted.",
        ));
        warning.set_wrap(true);
        warning.set_xalign(0.0);
        warning.set_margin_start(18);
        warning.set_margin_end(18);
        warning.set_margin_top(12);
        warning.set_margin_bottom(12);
        dialog.content_area().append(&warning);
        let state = Rc::clone(&state_for_empty);
        let toast = toast_for_empty.clone();
        dialog.connect_response(move |dialog, response| {
            if response == gtk::ResponseType::Accept {
                match state.client.empty_trash() {
                    Ok(_) => {
                        refresh_sidebar(&state);
                        refresh_browser(&state);
                        refresh_trash(&state);
                        toast.add_toast(adw::Toast::new("Trash emptied"));
                    }
                    Err(error) => {
                        toast
                            .add_toast(adw::Toast::new(&format!("Could not empty Trash: {error}")));
                    }
                }
            }
            dialog.close();
        });
        dialog.present();
    });
}

fn append_section_heading(list: &gtk::ListBox, text: &str) {
    let row = gtk::ListBoxRow::new();
    row.set_selectable(false);
    row.add_css_class("date-heading");
    let label = gtk::Label::new(Some(text));
    label.set_xalign(0.0);
    label.add_css_class("date-heading-label");
    row.set_child(Some(&label));
    list.append(&row);
}

fn trashed_category_row(
    state: &Rc<AppState>,
    category: &TrashedCategorySummary,
) -> gtk::ListBoxRow {
    let row = gtk::ListBoxRow::new();
    row.set_widget_name(&format!("trashed-category:{}", category.category.id));
    row.add_css_class("note-card");
    let content = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    content.set_margin_start(12);
    content.set_margin_end(12);
    content.set_margin_top(10);
    content.set_margin_bottom(10);
    content.append(&gtk::Image::from_icon_name("folder-symbolic"));
    let text = gtk::Box::new(gtk::Orientation::Vertical, 4);
    text.set_hexpand(true);
    let title = gtk::Label::new(Some(&category.category.name));
    title.set_xalign(0.0);
    title.set_ellipsize(gtk::pango::EllipsizeMode::End);
    title.set_single_line_mode(true);
    title.add_css_class("note-card-title");
    text.append(&title);
    let metadata = gtk::Label::new(Some(&format!(
        "{} • Deleted {}",
        note_count_label(category.recoverable_note_count),
        category
            .category
            .trashed_at
            .map_or_else(String::new, |time| time.date().to_string())
    )));
    metadata.set_xalign(0.0);
    metadata.add_css_class("note-card-excerpt");
    text.append(&metadata);
    content.append(&text);
    let restore = gtk::Button::with_label("Restore");
    restore.set_widget_name(&format!("restore-category:{}", category.category.id));
    let state_for_restore = Rc::clone(state);
    let category_id = category.category.id;
    restore.connect_clicked(move |_| {
        if let Ok(()) = state_for_restore.client.restore_category(category_id) {
            refresh_sidebar(&state_for_restore);
            refresh_browser(&state_for_restore);
            refresh_trash(&state_for_restore);
        }
    });
    content.append(&restore);
    row.set_child(Some(&content));
    row
}

fn trashed_note_row(state: &Rc<AppState>, note: &TrashedNoteSummary) -> gtk::ListBoxRow {
    let row = gtk::ListBoxRow::new();
    row.set_widget_name(&format!("trashed-note:{}", note.id));
    row.add_css_class("note-card");
    let content = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    content.set_margin_start(12);
    content.set_margin_end(12);
    content.set_margin_top(10);
    content.set_margin_bottom(10);
    let text = gtk::Box::new(gtk::Orientation::Vertical, 4);
    text.set_hexpand(true);
    let title = gtk::Label::new(Some(&note.title));
    title.set_xalign(0.0);
    title.set_ellipsize(gtk::pango::EllipsizeMode::End);
    title.set_single_line_mode(true);
    title.add_css_class("note-card-title");
    text.append(&title);
    let excerpt = gtk::Label::new(Some(&note.excerpt));
    excerpt.set_xalign(0.0);
    excerpt.set_ellipsize(gtk::pango::EllipsizeMode::End);
    excerpt.set_single_line_mode(true);
    excerpt.add_css_class("note-card-excerpt");
    text.append(&excerpt);
    let metadata = gtk::Label::new(Some(&format!(
        "From {} • Deleted {}",
        note.category_name,
        note.trashed_at.date()
    )));
    metadata.set_xalign(0.0);
    metadata.add_css_class("dim-label");
    text.append(&metadata);
    content.append(&text);
    let restore = gtk::Button::with_label("Restore");
    restore.set_widget_name(&format!("restore-note:{}", note.id));
    let state_for_restore = Rc::clone(state);
    let note_id = note.id;
    restore.connect_clicked(move |_| {
        if let Ok(()) = state_for_restore.client.restore_note(note_id) {
            refresh_sidebar(&state_for_restore);
            refresh_browser(&state_for_restore);
            refresh_trash(&state_for_restore);
        }
    });
    content.append(&restore);
    row.set_child(Some(&content));
    row
}

fn note_count_label(note_count: usize) -> String {
    if note_count == 1 {
        "1 recoverable note".to_owned()
    } else {
        format!("{note_count} recoverable notes")
    }
}
