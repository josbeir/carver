//! Category sidebar construction and interaction.

use std::rc::Rc;

use carver_sdk::{Category, CategoryId};
use gtk::prelude::*;
use libadwaita as adw;

use crate::{
    browser::refresh_browser,
    controller::{AppState, create_category, rename_category},
    dialogs::show_category_name_dialog,
};

/// Builds the responsive category sidebar.
pub(crate) fn build_sidebar(
    state: &Rc<AppState>,
    split_view: &adw::NavigationSplitView,
) -> gtk::Widget {
    let container = gtk::Box::new(gtk::Orientation::Vertical, 0);
    container.add_css_class("sidebar");

    let header = adw::HeaderBar::new();
    let collapse_sidebar = gtk::Button::from_icon_name("sidebar-hide-symbolic");
    collapse_sidebar.set_widget_name("hide-categories-button");
    collapse_sidebar.set_tooltip_text(Some("Hide Categories"));
    header.pack_start(&collapse_sidebar);
    let new_category = gtk::Button::from_icon_name("folder-new-symbolic");
    new_category.set_widget_name("new-category-button");
    new_category.set_tooltip_text(Some("New Category"));
    header.pack_end(&new_category);
    container.append(&header);

    let split_for_collapse = split_view.clone();
    collapse_sidebar.connect_clicked(move |_| {
        split_for_collapse.set_collapsed(true);
        split_for_collapse.set_show_content(true);
    });

    let list = gtk::ListBox::new();
    list.set_widget_name("category-list");
    list.set_selection_mode(gtk::SelectionMode::Single);
    list.add_css_class("navigation-sidebar");
    let home = gtk::ListBoxRow::new();
    home.set_selectable(true);
    home.set_child(Some(&sidebar_row("go-home-symbolic", "All notes")));
    list.append(&home);

    if let Ok(categories) = state.client.categories() {
        for category in categories {
            list.append(&category_sidebar_row(state, &category));
        }
    }
    list.select_row(Some(&home));

    let state_for_selection = Rc::clone(state);
    let split_for_selection = split_view.clone();
    list.connect_row_selected(move |_list, row| {
        let Some(row) = row else {
            return;
        };
        let selected = row
            .widget_name()
            .strip_prefix("category:")
            .and_then(|value| uuid::Uuid::parse_str(value).ok())
            .map(CategoryId::from_uuid);
        state_for_selection.selected_category.set(selected);
        let selected_name = selected.and_then(|category_id| {
            state_for_selection
                .client
                .categories()
                .ok()?
                .into_iter()
                .find(|category| category.id == category_id)
                .map(|category| category.name)
        });
        state_for_selection
            .selected_category_name
            .replace(selected_name);
        refresh_browser(&state_for_selection);
        if split_for_selection.is_collapsed() {
            split_for_selection.set_show_content(true);
        }
    });

    let state_for_new = Rc::clone(state);
    let list_for_new = list.clone();
    new_category.connect_clicked(move |button| {
        let parent = button
            .root()
            .and_then(|root| root.downcast::<gtk::Window>().ok());
        let state = Rc::clone(&state_for_new);
        let list = list_for_new.clone();
        show_category_name_dialog(parent.as_ref(), "New Category", "", move |name| {
            if let Ok(category) = create_category(&state, &name) {
                list.append(&category_sidebar_row(&state, &category));
            }
        });
    });

    let scroll = gtk::ScrolledWindow::new();
    scroll.set_child(Some(&list));
    scroll.set_vexpand(true);
    container.append(&scroll);
    container.upcast()
}

fn sidebar_row(icon_name: &str, label: &str) -> gtk::Widget {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    row.add_css_class("category-card-content");
    row.set_margin_start(12);
    row.set_margin_end(12);
    row.set_margin_top(8);
    row.set_margin_bottom(8);
    row.append(&gtk::Image::from_icon_name(icon_name));
    let text = gtk::Label::new(Some(label));
    text.add_css_class("category-card-title");
    text.set_xalign(0.0);
    row.append(&text);
    row.upcast()
}

fn category_sidebar_row(state: &Rc<AppState>, category: &Category) -> gtk::ListBoxRow {
    let row = gtk::ListBoxRow::new();
    row.set_selectable(true);
    row.set_widget_name(&format!("category:{}", category.id));
    row.add_css_class("category-card");
    let content = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    content.set_margin_start(12);
    content.set_margin_end(6);
    content.set_margin_top(6);
    content.set_margin_bottom(6);
    content.append(&gtk::Image::from_icon_name("folder-symbolic"));
    let text = gtk::Box::new(gtk::Orientation::Vertical, 2);
    text.set_hexpand(true);
    let name = gtk::Label::new(Some(&category.name));
    name.set_xalign(0.0);
    name.set_ellipsize(gtk::pango::EllipsizeMode::End);
    name.set_single_line_mode(true);
    name.add_css_class("category-card-title");
    text.append(&name);
    let note_count = state.client.note_count(category.id).unwrap_or_default();
    let count_label = gtk::Label::new(Some(&note_count_label(note_count)));
    count_label.set_widget_name(&format!("category-count:{}", category.id));
    count_label.set_xalign(0.0);
    count_label.add_css_class("category-card-count");
    text.append(&count_label);
    content.append(&text);
    let rename = gtk::Button::from_icon_name("document-edit-symbolic");
    rename.set_widget_name(&format!("rename-category:{}", category.id));
    rename.set_tooltip_text(Some("Rename Category"));
    rename.add_css_class("flat");
    let state_for_rename = Rc::clone(state);
    let name_for_rename = name.clone();
    let category_id = category.id;
    rename.connect_clicked(move |button| {
        let parent = button
            .root()
            .and_then(|root| root.downcast::<gtk::Window>().ok());
        let state = Rc::clone(&state_for_rename);
        let name_label = name_for_rename.clone();
        let current_name = name_for_rename.text().to_string();
        show_category_name_dialog(
            parent.as_ref(),
            "Rename Category",
            &current_name,
            move |name| {
                if let Ok(category) = rename_category(&state, category_id, &name) {
                    if state.selected_category.get() == Some(category.id) {
                        state
                            .selected_category_name
                            .replace(Some(category.name.clone()));
                        refresh_browser(&state);
                    }
                    name_label.set_text(&category.name);
                }
            },
        );
    });
    content.append(&rename);
    let trash = gtk::Button::from_icon_name("user-trash-symbolic");
    trash.set_widget_name(&format!("delete-category:{}", category.id));
    trash.set_tooltip_text(Some("Move Category to Trash"));
    trash.add_css_class("flat");
    let state_for_trash = Rc::clone(state);
    let row_for_trash = row.clone();
    let category_id = category.id;
    trash.connect_clicked(move |_| {
        if state_for_trash.client.trash_category(category_id).is_ok() {
            if state_for_trash.selected_category.get() == Some(category_id) {
                state_for_trash.selected_category.set(None);
                state_for_trash.selected_category_name.replace(None);
            }
            if let Some(parent) = row_for_trash.parent()
                && let Ok(list) = parent.downcast::<gtk::ListBox>()
            {
                list.remove(&row_for_trash);
            }
            refresh_browser(&state_for_trash);
        }
    });
    content.append(&trash);
    row.set_child(Some(&content));
    row
}

fn note_count_label(note_count: usize) -> String {
    if note_count == 1 {
        "1 note".to_owned()
    } else {
        format!("{note_count} notes")
    }
}
