//! Category sidebar construction and snapshot rendering.

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use carver_sdk::{Category, CategoryId, CategorySummary};
use gtk::prelude::*;
use libadwaita as adw;

use crate::{
    dialogs::{show_category_name_dialog, show_category_trash_confirmation},
    mvu::{ActionMsg, AppDispatcher, AppModel, AppMsg, EditorMsg, LoadState, NavigationMsg, Route},
};

/// Responsive category sidebar and its snapshot renderer.
#[derive(Clone)]
pub(crate) struct SidebarSurface {
    pub(crate) widget: gtk::Widget,
    pub(crate) list: gtk::ListBox,
    dispatcher: AppDispatcher,
    split_view: adw::NavigationSplitView,
    rendering: Rc<Cell<bool>>,
    route: Rc<RefCell<Route>>,
}

/// Builds the responsive category sidebar.
pub(crate) fn build_sidebar(
    dispatcher: &AppDispatcher,
    split_view: &adw::NavigationSplitView,
) -> SidebarSurface {
    let container = gtk::Box::new(gtk::Orientation::Vertical, 0);
    container.add_css_class("sidebar");
    let header = adw::HeaderBar::new();
    let new_category = gtk::Button::from_icon_name("folder-new-symbolic");
    new_category.set_widget_name("new-category-button");
    new_category.set_tooltip_text(Some("New Category"));
    header.pack_start(&new_category);
    header.pack_end(&settings_menu_button());
    container.append(&header);

    let list = gtk::ListBox::new();
    list.set_widget_name("category-list");
    list.set_selection_mode(gtk::SelectionMode::Single);
    list.add_css_class("navigation-sidebar");
    let rendering = Rc::new(Cell::new(false));
    let route = Rc::new(RefCell::new(Route::Browser));
    connect_selection(dispatcher, split_view, &list, &rendering);
    connect_new_category(dispatcher, &new_category);

    let scroll = gtk::ScrolledWindow::new();
    scroll.set_child(Some(&list));
    scroll.set_vexpand(true);
    container.append(&scroll);
    container.append(&trash_footer(dispatcher, split_view));
    SidebarSurface {
        widget: container.upcast(),
        list,
        dispatcher: dispatcher.clone(),
        split_view: split_view.clone(),
        rendering,
        route,
    }
}

/// Builds the window-level settings menu shown in the persistent sidebar.
fn settings_menu_button() -> gtk::MenuButton {
    let menu = gtk::gio::Menu::new();
    menu.append(Some("Connect an agent"), Some("win.connect-agent"));
    menu.append(Some("Preferences"), Some("win.preferences"));
    menu.append(
        Some("Keyboard Shortcuts"),
        Some(crate::dialogs::KEYBOARD_SHORTCUTS_ACTION),
    );
    menu.append(Some("About Carver"), Some("win.about"));
    let settings = gtk::MenuButton::new();
    settings.set_widget_name("sidebar-settings-menu-button");
    settings.set_icon_name("open-menu-symbolic");
    settings.set_tooltip_text(Some("Settings"));
    settings.set_menu_model(Some(&menu));
    settings
}

impl SidebarSurface {
    /// Renders category rows from the current application snapshot.
    pub(crate) fn render(&self, model: &AppModel) {
        *self.route.borrow_mut() = model.route.clone();
        self.rendering.set(true);
        let LoadState::Ready(categories) = &model.sidebar.state else {
            clear_list(&self.list);
            self.rendering.set(false);
            return;
        };
        populate_sidebar(
            &self.list,
            &self.dispatcher,
            &self.split_view,
            &self.route,
            categories,
            model.selected_category,
        );
        self.rendering.set(false);
    }
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

fn connect_selection(
    dispatcher: &AppDispatcher,
    split_view: &adw::NavigationSplitView,
    list: &gtk::ListBox,
    rendering: &Rc<Cell<bool>>,
) {
    let dispatcher = dispatcher.clone();
    let split_view = split_view.clone();
    let rendering = Rc::clone(rendering);
    list.connect_row_selected(move |_list, row| {
        if rendering.get() {
            return;
        }
        let selected = row.and_then(category_id_from_row);
        let _ = dispatcher.dispatch(AppMsg::Navigation(NavigationMsg::SelectCategory(selected)));
        if split_view.is_collapsed() {
            split_view.set_show_content(true);
        }
    });
}

fn connect_new_category(dispatcher: &AppDispatcher, button: &gtk::Button) {
    let dispatcher = dispatcher.clone();
    button.connect_clicked(move |button| {
        let parent = button
            .root()
            .and_then(|root| root.downcast::<gtk::Window>().ok());
        let dispatcher = dispatcher.clone();
        show_category_name_dialog(parent.as_ref(), "New Category", "", move |name| {
            let _ = dispatcher.dispatch(AppMsg::Action(ActionMsg::CreateCategory(name)));
        });
    });
}

fn trash_footer(dispatcher: &AppDispatcher, split_view: &adw::NavigationSplitView) -> gtk::Widget {
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
    let dispatcher = dispatcher.clone();
    let split_view = split_view.clone();
    trash.connect_clicked(move |_| {
        let _ = dispatcher.dispatch(AppMsg::Navigation(NavigationMsg::ShowTrash));
        if split_view.is_collapsed() {
            split_view.set_show_content(true);
        }
    });
    let footer = gtk::Box::new(gtk::Orientation::Vertical, 0);
    footer.add_css_class("sidebar-footer");
    footer.append(&trash);
    footer.upcast()
}

fn populate_sidebar(
    list: &gtk::ListBox,
    dispatcher: &AppDispatcher,
    split_view: &adw::NavigationSplitView,
    route: &Rc<RefCell<Route>>,
    categories: &[CategorySummary],
    selected_category: Option<CategoryId>,
) {
    clear_list(list);
    let home = gtk::ListBoxRow::new();
    home.set_selectable(true);
    home.add_css_class("category-card");
    let all_notes_count = categories.iter().map(|summary| summary.note_count).sum();
    let home_content = sidebar_row(
        "go-home-symbolic",
        "All notes",
        all_notes_count,
        Some("all-notes-count"),
    );
    install_active_row_navigation(&home_content, dispatcher, split_view, route, None);
    home.set_child(Some(&home_content));
    list.append(&home);
    let mut selected_row = None;
    for summary in categories {
        let row = category_sidebar_row(
            dispatcher,
            split_view,
            route,
            &summary.category,
            summary.note_count,
        );
        if Some(summary.category.id) == selected_category {
            selected_row = Some(row.clone());
        }
        list.append(&row);
    }
    list.select_row(selected_row.as_ref().or(Some(&home)));
}

fn clear_list(list: &gtk::ListBox) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }
}

fn category_id_from_row(row: &gtk::ListBoxRow) -> Option<CategoryId> {
    row.widget_name()
        .strip_prefix("category:")
        .and_then(|value| uuid::Uuid::parse_str(value).ok())
        .map(CategoryId::from_uuid)
}

fn sidebar_row(
    icon_name: &str,
    label: &str,
    note_count: usize,
    count_widget_name: Option<&str>,
) -> gtk::Widget {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    row.add_css_class("category-card-content");
    row.set_margin_start(16);
    row.set_margin_end(16);
    row.set_margin_top(5);
    row.set_margin_bottom(5);
    row.append(&gtk::Image::from_icon_name(icon_name));
    let text = gtk::Box::new(gtk::Orientation::Vertical, 2);
    let title = gtk::Label::new(Some(label));
    title.add_css_class("category-card-title");
    title.set_xalign(0.0);
    text.append(&title);
    let count = gtk::Label::new(Some(&note_count_label(note_count)));
    if let Some(widget_name) = count_widget_name {
        count.set_widget_name(widget_name);
    }
    count.add_css_class("category-card-count");
    count.set_xalign(0.0);
    text.append(&count);
    row.append(&text);
    row.upcast()
}

fn category_sidebar_row(
    dispatcher: &AppDispatcher,
    split_view: &adw::NavigationSplitView,
    route: &Rc<RefCell<Route>>,
    category: &Category,
    note_count: usize,
) -> gtk::ListBoxRow {
    let row = gtk::ListBoxRow::new();
    row.set_selectable(true);
    row.set_widget_name(&format!("category:{}", category.id));
    row.add_css_class("category-card");
    let content = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    content.set_margin_start(16);
    content.set_margin_end(16);
    content.set_margin_top(5);
    content.set_margin_bottom(5);
    let (primary_content, name) = category_primary_content(category, note_count);
    install_active_row_navigation(
        &primary_content,
        dispatcher,
        split_view,
        route,
        Some(category.id),
    );
    content.append(&primary_content);
    let rename = gtk::Button::from_icon_name("document-edit-symbolic");
    rename.set_widget_name(&format!("rename-category:{}", category.id));
    rename.set_tooltip_text(Some("Rename Category"));
    rename.add_css_class("flat");
    let dispatcher_for_rename = dispatcher.clone();
    let category_id = category.id;
    rename.connect_clicked(move |button| {
        let parent = button
            .root()
            .and_then(|root| root.downcast::<gtk::Window>().ok());
        let current_name = name.text().to_string();
        let dispatcher = dispatcher_for_rename.clone();
        show_category_name_dialog(
            parent.as_ref(),
            "Rename Category",
            &current_name,
            move |name| {
                let _ = dispatcher.dispatch(AppMsg::Action(ActionMsg::RenameCategory {
                    category_id,
                    name,
                }));
            },
        );
    });
    let trash = gtk::Button::from_icon_name("user-trash-symbolic");
    trash.set_widget_name(&format!("delete-category:{}", category.id));
    trash.set_tooltip_text(Some("Move Category to Trash"));
    trash.add_css_class("flat");
    let dispatcher_for_trash = dispatcher.clone();
    let category_id = category.id;
    let category_name = category.name.clone();
    trash.connect_clicked(move |button| {
        let parent = button
            .root()
            .and_then(|root| root.downcast::<gtk::Window>().ok());
        let dispatcher = dispatcher_for_trash.clone();
        show_category_trash_confirmation(parent.as_ref(), &category_name, move || {
            let _ = dispatcher.dispatch(AppMsg::Action(ActionMsg::TrashCategory(category_id)));
        });
    });
    let actions = gtk::Box::new(gtk::Orientation::Horizontal, 2);
    actions.set_widget_name(&format!("category-actions:{}", category.id));
    actions.add_css_class("category-actions");
    actions.append(&rename);
    actions.append(&trash);
    install_category_action_visibility(&row, &actions);
    content.append(&actions);
    row.set_child(Some(&content));
    row
}

fn category_primary_content(category: &Category, note_count: usize) -> (gtk::Box, gtk::Label) {
    let primary_content = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    primary_content.set_hexpand(true);
    primary_content.append(&gtk::Image::from_icon_name("folder-symbolic"));
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
    primary_content.append(&text);
    (primary_content, name)
}

fn install_active_row_navigation(
    widget: &impl IsA<gtk::Widget>,
    dispatcher: &AppDispatcher,
    split_view: &adw::NavigationSplitView,
    route: &Rc<RefCell<Route>>,
    category_id: Option<CategoryId>,
) {
    let click = gtk::GestureClick::new();
    click.set_button(gtk::gdk::BUTTON_PRIMARY);
    let dispatcher = dispatcher.clone();
    let split_view = split_view.clone();
    let route = Rc::clone(route);
    click.connect_released(move |_, _, _, _| {
        let message = if *route.borrow() == Route::Editor {
            AppMsg::Editor(EditorMsg::BackRequested)
        } else {
            AppMsg::Navigation(NavigationMsg::SelectCategory(category_id))
        };
        let _ = dispatcher.dispatch(message);
        if split_view.is_collapsed() {
            split_view.set_show_content(true);
        }
    });
    widget.add_controller(click);
}

fn install_category_action_visibility(row: &gtk::ListBoxRow, actions: &gtk::Box) {
    actions.set_visible(false);
    let actions_for_selection = actions.clone();
    let row_for_selection = row.clone();
    row.connect_state_flags_changed(move |row, previous| {
        let selected = row.state_flags().contains(gtk::StateFlags::SELECTED);
        let was_selected = previous.contains(gtk::StateFlags::SELECTED);
        if selected == was_selected {
            return;
        }
        actions_for_selection.set_visible(selected);
        if selected {
            row_for_selection.add_css_class("category-actions-visible");
        } else {
            row_for_selection.remove_css_class("category-actions-visible");
        }
    });
    let motion = gtk::EventControllerMotion::new();
    let actions_for_enter = actions.clone();
    let row_for_motion_enter = row.clone();
    motion.connect_enter(move |_, _, _| {
        actions_for_enter.set_visible(true);
        row_for_motion_enter.add_css_class("category-actions-visible");
    });
    let actions_for_leave = actions.clone();
    let row_for_motion_leave = row.clone();
    motion.connect_leave(move |_| {
        if !row_for_motion_leave
            .state_flags()
            .contains(gtk::StateFlags::SELECTED)
        {
            actions_for_leave.set_visible(false);
            row_for_motion_leave.remove_css_class("category-actions-visible");
        }
    });
    row.add_controller(motion);
    let focus = gtk::EventControllerFocus::new();
    let actions_for_focus = actions.clone();
    let row_for_enter = row.clone();
    focus.connect_enter(move |_| {
        actions_for_focus.set_visible(true);
        row_for_enter.add_css_class("category-actions-visible");
    });
    let actions_for_blur = actions.clone();
    let row_for_leave = row.clone();
    focus.connect_leave(move |_| {
        if !row_for_leave
            .state_flags()
            .contains(gtk::StateFlags::SELECTED)
        {
            actions_for_blur.set_visible(false);
            row_for_leave.remove_css_class("category-actions-visible");
        }
    });
    actions.add_controller(focus);
}

fn note_count_label(note_count: usize) -> String {
    if note_count == 1 {
        "1 note".to_owned()
    } else {
        format!("{note_count} notes")
    }
}
