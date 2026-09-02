//! Shared note-category move UI and asynchronous move actions.

use std::{cell::RefCell, rc::Rc};

use adw::prelude::*;
use carver_sdk::{Category, CategoryId, NoteId};
use gtk::prelude::*;
use libadwaita as adw;

use crate::{
    browser::refresh_browser, controller::AppState, dialogs::show_category_name_dialog,
    sidebar::refresh_sidebar,
};

/// Opens the category picker for one note.
pub(crate) fn show_move_note_dialog(
    state: &Rc<AppState>,
    note_id: NoteId,
    current_category_id: CategoryId,
    note_title: &str,
    toast_overlay: &adw::ToastOverlay,
    parent: Option<&gtk::Window>,
) -> adw::Dialog {
    let dialog = adw::Dialog::builder()
        .title(format!("Move “{note_title}”"))
        .content_width(420)
        .content_height(460)
        .build();
    dialog.set_widget_name("move-note-dialog");
    let content = gtk::Box::new(gtk::Orientation::Vertical, 12);
    content.set_spacing(12);
    content.set_margin_start(18);
    content.set_margin_end(18);
    content.set_margin_top(18);
    content.set_margin_bottom(18);
    let title = gtk::Label::new(Some(&format!("Move “{note_title}”")));
    title.add_css_class("title-3");
    title.set_xalign(0.0);
    content.append(&title);
    let search = gtk::SearchEntry::new();
    search.set_widget_name("move-note-search");
    search.set_placeholder_text(Some("Search categories"));
    content.append(&search);
    let list = gtk::ListBox::new();
    list.set_widget_name("move-note-category-list");
    list.set_selection_mode(gtk::SelectionMode::None);
    list.add_css_class("boxed-list");
    let scroll = gtk::ScrolledWindow::new();
    scroll.set_vexpand(true);
    scroll.set_child(Some(&list));
    content.append(&scroll);
    let new_category = gtk::Button::with_label("New Category…");
    new_category.set_widget_name("move-note-new-category-button");
    new_category.add_css_class("flat");
    new_category.set_halign(gtk::Align::Start);
    content.append(&new_category);
    let cancel = gtk::Button::with_label("Cancel");
    cancel.set_halign(gtk::Align::End);
    let dialog_for_cancel = dialog.clone();
    cancel.connect_clicked(move |_| {
        dialog_for_cancel.close();
    });
    content.append(&cancel);
    dialog.set_child(Some(&content));

    let categories = Rc::new(RefCell::new(Vec::<Category>::new()));
    connect_search(
        &search,
        &list,
        &categories,
        state,
        note_id,
        current_category_id,
        toast_overlay,
        &dialog,
    );
    connect_new_category(
        &new_category,
        state,
        note_id,
        current_category_id,
        toast_overlay,
        &dialog,
        parent,
    );
    load_categories(
        state,
        &list,
        &categories,
        note_id,
        current_category_id,
        toast_overlay,
        &dialog,
    );
    dialog.present(parent);
    dialog
}

#[expect(
    clippy::too_many_arguments,
    reason = "the search callback needs the picker widgets and move context"
)]
fn connect_search(
    search: &gtk::SearchEntry,
    list: &gtk::ListBox,
    categories: &Rc<RefCell<Vec<Category>>>,
    state: &Rc<AppState>,
    note_id: NoteId,
    current_category_id: CategoryId,
    toast_overlay: &adw::ToastOverlay,
    dialog: &adw::Dialog,
) {
    let list = list.clone();
    let categories = Rc::clone(categories);
    let state = Rc::clone(state);
    let toast_overlay = toast_overlay.clone();
    let dialog = dialog.clone();
    search.connect_search_changed(move |search| {
        populate_categories(
            &list,
            &categories.borrow(),
            search.text().as_str(),
            &state,
            note_id,
            current_category_id,
            &toast_overlay,
            &dialog,
        );
    });
}

fn connect_new_category(
    button: &gtk::Button,
    state: &Rc<AppState>,
    note_id: NoteId,
    current_category_id: CategoryId,
    toast_overlay: &adw::ToastOverlay,
    dialog: &adw::Dialog,
    parent: Option<&gtk::Window>,
) {
    let state_for_new = Rc::clone(state);
    let toast_for_new = toast_overlay.clone();
    let parent = parent.cloned();
    let dialog = dialog.clone();
    button.connect_clicked(move |_| {
        dialog.close();
        let state = Rc::clone(&state_for_new);
        let toast = toast_for_new.clone();
        show_category_name_dialog(parent.as_ref(), "New Category", "", move |name| {
            let state = Rc::clone(&state);
            let toast = toast.clone();
            let client = state.client.clone();
            glib::spawn_future_local(async move {
                match client.create_category_async(name).await {
                    Ok(category) => {
                        refresh_sidebar(&state);
                        move_note_to_category(
                            &state,
                            note_id,
                            current_category_id,
                            category,
                            &toast,
                        );
                    }
                    Err(error) => toast.add_toast(adw::Toast::new(&format!(
                        "Could not create category: {error}"
                    ))),
                }
            });
        });
    });
}

fn load_categories(
    state: &Rc<AppState>,
    list: &gtk::ListBox,
    categories: &Rc<RefCell<Vec<Category>>>,
    note_id: NoteId,
    current_category_id: CategoryId,
    toast_overlay: &adw::ToastOverlay,
    dialog: &adw::Dialog,
) {
    let state = Rc::clone(state);
    let list = list.clone();
    let categories_for_load = Rc::clone(categories);
    let toast_overlay = toast_overlay.clone();
    let dialog = dialog.clone();
    let client = state.client.clone();
    glib::spawn_future_local(async move {
        match client.categories_async().await {
            Ok(loaded) => {
                categories_for_load.replace(loaded);
                populate_categories(
                    &list,
                    &categories_for_load.borrow(),
                    "",
                    &state,
                    note_id,
                    current_category_id,
                    &toast_overlay,
                    &dialog,
                );
            }
            Err(error) => {
                let row = gtk::ListBoxRow::new();
                row.set_selectable(false);
                row.set_child(Some(&gtk::Label::new(Some(&format!(
                    "Could not load categories: {error}"
                )))));
                list.append(&row);
            }
        }
    });
}

#[expect(
    clippy::too_many_arguments,
    reason = "each picker row needs the move context when activated"
)]
fn populate_categories(
    list: &gtk::ListBox,
    categories: &[Category],
    query: &str,
    state: &Rc<AppState>,
    note_id: NoteId,
    current_category_id: CategoryId,
    toast_overlay: &adw::ToastOverlay,
    dialog: &adw::Dialog,
) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }
    let query = query.trim().to_lowercase();
    let mut matching_categories = 0;
    for category in categories {
        if !query.is_empty() && !category.name.to_lowercase().contains(&query) {
            continue;
        }
        matching_categories += 1;
        let row = gtk::ListBoxRow::new();
        let button = gtk::Button::new();
        button.add_css_class("flat");
        button.set_hexpand(true);
        let content = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        content.set_margin_start(12);
        content.set_margin_end(12);
        content.set_margin_top(8);
        content.set_margin_bottom(8);
        content.append(&gtk::Image::from_icon_name("folder-symbolic"));
        let label = gtk::Label::new(Some(&category.name));
        label.set_xalign(0.0);
        label.set_hexpand(true);
        content.append(&label);
        if category.id == current_category_id {
            let current = gtk::Label::new(Some("Current"));
            current.add_css_class("dim-label");
            content.append(&current);
            button.set_sensitive(false);
        } else {
            let state = Rc::clone(state);
            let toast_overlay = toast_overlay.clone();
            let dialog = dialog.clone();
            let destination = category.clone();
            button.connect_clicked(move |_| {
                dialog.close();
                move_note_to_category(
                    &state,
                    note_id,
                    current_category_id,
                    destination.clone(),
                    &toast_overlay,
                );
            });
        }
        button.set_child(Some(&content));
        row.set_child(Some(&button));
        list.append(&row);
    }
    if matching_categories == 0 {
        let row = gtk::ListBoxRow::new();
        row.set_selectable(false);
        let label = gtk::Label::new(Some("No categories found"));
        label.add_css_class("dim-label");
        label.set_margin_top(12);
        label.set_margin_bottom(12);
        row.set_child(Some(&label));
        list.append(&row);
    }
}

/// Moves a note, refreshes all dependent views, and offers a one-click undo.
pub(crate) fn move_note_to_category(
    state: &Rc<AppState>,
    note_id: NoteId,
    source_category_id: CategoryId,
    destination: Category,
    toast_overlay: &adw::ToastOverlay,
) {
    if source_category_id == destination.id {
        return;
    }
    let source_category_name = state
        .categories
        .borrow()
        .iter()
        .find(|category| category.id == source_category_id)
        .map_or_else(
            || "previous category".to_owned(),
            |category| category.name.clone(),
        );
    let state = Rc::clone(state);
    let toast_overlay = toast_overlay.clone();
    let client = state.client.clone();
    glib::spawn_future_local(async move {
        match client.move_note_async(note_id, destination.id).await {
            Ok(moved) => {
                if state
                    .current_note
                    .borrow()
                    .as_ref()
                    .is_some_and(|note| note.id == note_id)
                {
                    state.current_note.replace(Some(moved));
                }
                refresh_browser(&state);
                refresh_sidebar(&state);
                let toast = adw::Toast::new(&format!("Moved to {}", destination.name));
                toast.set_button_label(Some("Undo"));
                let state_for_undo = Rc::clone(&state);
                let toast_for_undo = toast_overlay.clone();
                let source_category_name = source_category_name.clone();
                toast.connect_button_clicked(move |_| {
                    let state = Rc::clone(&state_for_undo);
                    let client = state.client.clone();
                    let toast = toast_for_undo.clone();
                    let source_category_name = source_category_name.clone();
                    glib::spawn_future_local(async move {
                        match client.move_note_async(note_id, source_category_id).await {
                            Ok(restored) => {
                                if state
                                    .current_note
                                    .borrow()
                                    .as_ref()
                                    .is_some_and(|note| note.id == note_id)
                                {
                                    state.current_note.replace(Some(restored));
                                }
                                refresh_browser(&state);
                                refresh_sidebar(&state);
                                toast.add_toast(adw::Toast::new(&format!(
                                    "Moved back to {source_category_name}"
                                )));
                            }
                            Err(error) => toast.add_toast(adw::Toast::new(&format!(
                                "Could not undo move: {error}"
                            ))),
                        }
                    });
                });
                toast_overlay.add_toast(toast);
            }
            Err(error) => {
                toast_overlay.add_toast(adw::Toast::new(&format!("Could not move note: {error}")));
            }
        }
    });
}
