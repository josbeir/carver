//! Category sidebar construction and interaction.

use std::rc::Rc;

use carver_sdk::{Category, CategoryId};
use gtk::prelude::*;
use libadwaita as adw;

use crate::{
    browser::refresh_browser, controller::AppState, dialogs::show_category_name_dialog,
    trash::refresh_trash,
};

/// Builds the responsive category sidebar.
pub(crate) fn build_sidebar(
    state: &Rc<AppState>,
    split_view: &adw::NavigationSplitView,
) -> gtk::Widget {
    let container = gtk::Box::new(gtk::Orientation::Vertical, 0);
    container.add_css_class("sidebar");

    let header = adw::HeaderBar::new();
    let new_category = gtk::Button::from_icon_name("folder-new-symbolic");
    new_category.set_widget_name("new-category-button");
    new_category.set_tooltip_text(Some("New Category"));
    header.pack_end(&new_category);
    container.append(&header);

    let list = gtk::ListBox::new();
    list.set_widget_name("category-list");
    list.set_selection_mode(gtk::SelectionMode::Single);
    list.add_css_class("navigation-sidebar");
    state.sidebar_list.replace(Some(list.clone()));
    refresh_sidebar(state);

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
                .categories
                .borrow()
                .iter()
                .find(|category| category.id == category_id)
                .map(|category| category.name.clone())
        });
        state_for_selection
            .selected_category_name
            .replace(selected_name);
        refresh_browser(&state_for_selection);
        if let Some(stack) = state_for_selection.browser_stack.borrow().clone() {
            stack.set_visible_child_name("browser");
        }
        if split_for_selection.is_collapsed() {
            split_for_selection.set_show_content(true);
        }
    });

    let state_for_new = Rc::clone(state);
    new_category.connect_clicked(move |button| {
        let parent = button
            .root()
            .and_then(|root| root.downcast::<gtk::Window>().ok());
        let state = Rc::clone(&state_for_new);
        show_category_name_dialog(parent.as_ref(), "New Category", "", move |name| {
            let client = state.client.clone();
            let state = Rc::clone(&state);
            glib::spawn_future_local(async move {
                if client.create_category_async(name).await.is_ok() {
                    refresh_sidebar(&state);
                }
            });
        });
    });

    let scroll = gtk::ScrolledWindow::new();
    scroll.set_child(Some(&list));
    scroll.set_vexpand(true);
    container.append(&scroll);
    container.append(&trash_footer(state, split_view));
    container.upcast()
}

/// Builds the shared control that expands or collapses the category sidebar.
pub(crate) fn sidebar_toggle_button(
    split_view: &adw::NavigationSplitView,
    widget_name: &str,
) -> gtk::ToggleButton {
    let toggle = gtk::ToggleButton::new();
    toggle.set_icon_name("sidebar-show-symbolic");
    toggle.set_widget_name(widget_name);
    toggle.set_tooltip_text(Some("Hide Categories"));
    toggle.set_active(!split_view.is_collapsed());

    let split = split_view.clone();
    toggle.connect_toggled(move |button| {
        if button.is_active() {
            split.set_collapsed(false);
        } else {
            split.set_collapsed(true);
            split.set_show_content(true);
        }
    });

    let toggle_for_state = toggle.clone();
    split_view.connect_collapsed_notify(move |split| {
        if split.is_collapsed() {
            toggle_for_state.set_active(false);
            toggle_for_state.set_tooltip_text(Some("Show Categories"));
        } else {
            toggle_for_state.set_active(true);
            toggle_for_state.set_tooltip_text(Some("Hide Categories"));
        }
    });
    toggle
}

fn trash_footer(state: &Rc<AppState>, split_view: &adw::NavigationSplitView) -> gtk::Widget {
    let trash = gtk::Button::new();
    trash.set_widget_name("open-trash-button");
    trash.set_tooltip_text(Some("Open Trash"));
    trash.add_css_class("flat");
    let trash_content = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    trash_content.set_margin_start(12);
    trash_content.set_margin_end(12);
    trash_content.set_margin_top(8);
    trash_content.set_margin_bottom(8);
    trash_content.append(&gtk::Image::from_icon_name("user-trash-symbolic"));
    let trash_label = gtk::Label::new(Some("Trash"));
    trash_label.set_xalign(0.0);
    trash_label.add_css_class("category-card-title");
    trash_content.append(&trash_label);
    trash.set_child(Some(&trash_content));
    let state_for_trash = Rc::clone(state);
    let split_for_trash = split_view.clone();
    trash.connect_clicked(move |_| {
        refresh_trash(&state_for_trash);
        if let Some(stack) = state_for_trash.browser_stack.borrow().clone() {
            stack.set_visible_child_name("trash");
        }
        if split_for_trash.is_collapsed() {
            split_for_trash.set_show_content(true);
        }
    });
    let footer = gtk::Box::new(gtk::Orientation::Vertical, 0);
    footer.add_css_class("sidebar-footer");
    footer.append(&trash);
    footer.upcast()
}

/// Rebuilds the category list after a category or trash action.
pub(crate) fn refresh_sidebar(state: &Rc<AppState>) {
    let Some(list) = state.sidebar_list.borrow().clone() else {
        return;
    };
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }
    let generation = state.sidebar_generation.get().saturating_add(1);
    state.sidebar_generation.set(generation);
    let state = Rc::clone(state);
    let client = state.client.clone();
    glib::spawn_future_local(async move {
        let Ok(categories) = client.categories_async().await else {
            return;
        };
        let mut categories_with_counts = Vec::with_capacity(categories.len());
        for category in &categories {
            let count = client
                .note_count_async(category.id)
                .await
                .unwrap_or_default();
            categories_with_counts.push((category.clone(), count));
        }
        if state.sidebar_generation.get() != generation {
            return;
        }
        state.categories.replace(categories);
        populate_sidebar(&list, &state, categories_with_counts);
    });
}

fn populate_sidebar(list: &gtk::ListBox, state: &Rc<AppState>, categories: Vec<(Category, usize)>) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }
    let home = gtk::ListBoxRow::new();
    home.set_selectable(true);
    home.set_child(Some(&sidebar_row("go-home-symbolic", "All notes")));
    list.append(&home);
    let selected_category = state.selected_category.get();
    let mut selected_row = None;
    for (category, note_count) in categories {
        let row = category_sidebar_row(state, &category, note_count);
        if Some(category.id) == selected_category {
            selected_row = Some(row.clone());
        }
        list.append(&row);
    }
    list.select_row(selected_row.as_ref().or(Some(&home)));
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

fn category_sidebar_row(
    state: &Rc<AppState>,
    category: &Category,
    note_count: usize,
) -> gtk::ListBoxRow {
    let row = gtk::ListBoxRow::new();
    row.set_selectable(true);
    row.set_widget_name(&format!("category:{}", category.id));
    row.add_css_class("category-card");
    let content = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    content.set_margin_start(12);
    content.set_margin_end(8);
    content.set_margin_top(4);
    content.set_margin_bottom(4);
    content.append(&gtk::Image::from_icon_name("folder-symbolic"));
    let text = gtk::Box::new(gtk::Orientation::Vertical, 2);
    text.set_hexpand(true);
    let name = gtk::Label::new(Some(&category.name));
    name.set_xalign(0.0);
    name.set_ellipsize(gtk::pango::EllipsizeMode::End);
    name.set_single_line_mode(true);
    name.add_css_class("category-card-title");
    text.append(&name);
    let count_label = gtk::Label::new(Some(&note_count_label(note_count)));
    count_label.set_widget_name(&format!("category-count:{}", category.id));
    count_label.set_xalign(0.0);
    count_label.add_css_class("category-card-count");
    text.append(&count_label);
    content.append(&text);
    let rename = gtk::Button::from_icon_name("document-edit-symbolic");
    rename.set_widget_name(&format!("rename-category:{}", category.id));
    rename.set_tooltip_text(Some("Rename Category"));
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
                let client = state.client.clone();
                let state = Rc::clone(&state);
                let name_label = name_label.clone();
                glib::spawn_future_local(async move {
                    if let Ok(category) = client.rename_category_async(category_id, name).await {
                        if state.selected_category.get() == Some(category.id) {
                            state
                                .selected_category_name
                                .replace(Some(category.name.clone()));
                            refresh_browser(&state);
                        }
                        name_label.set_text(&category.name);
                        refresh_sidebar(&state);
                    }
                });
            },
        );
    });
    let trash = gtk::Button::from_icon_name("user-trash-symbolic");
    trash.set_widget_name(&format!("delete-category:{}", category.id));
    trash.set_tooltip_text(Some("Move Category to Trash"));
    let state_for_trash = Rc::clone(state);
    let category_id = category.id;
    trash.connect_clicked(move |_| {
        let state = Rc::clone(&state_for_trash);
        let client = state.client.clone();
        glib::spawn_future_local(async move {
            if client.trash_category_async(category_id).await.is_ok() {
                if state.selected_category.get() == Some(category_id) {
                    state.selected_category.set(None);
                    state.selected_category_name.replace(None);
                }
                refresh_sidebar(&state);
                refresh_browser(&state);
                refresh_trash(&state);
            }
        });
    });
    let actions = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    actions.add_css_class("linked");
    actions.add_css_class("category-actions");
    actions.append(&rename);
    actions.append(&trash);
    content.append(&actions);
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
