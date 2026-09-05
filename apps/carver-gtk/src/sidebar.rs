//! Category sidebar construction and snapshot rendering.

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use carver_sdk::{Category, CategoryAppearance, CategoryId, CategorySummary};
use gtk::prelude::*;
use libadwaita as adw;

use crate::{
    dialogs::{category_color_css_class, category_icon_name, show_category_dialog},
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
        show_category_dialog(
            parent.as_ref(),
            "New Category",
            "",
            CategoryAppearance::default(),
            move |name, appearance| {
                let _ =
                    dispatcher.dispatch(AppMsg::Action(ActionMsg::CreateCategoryWithAppearance {
                        name,
                        appearance,
                    }));
            },
        );
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
    row.add_css_class("category-row-content");
    row.set_margin_start(10);
    row.set_margin_end(10);
    row.set_margin_top(6);
    row.set_margin_bottom(6);
    row.append(&sidebar_icon_tile(icon_name, "all-notes-icon"));
    let title = gtk::Label::new(Some(label));
    title.add_css_class("category-card-title");
    title.set_xalign(0.0);
    title.set_hexpand(true);
    row.append(&title);
    let count = gtk::Label::new(Some(&note_count.to_string()));
    if let Some(widget_name) = count_widget_name {
        count.set_widget_name(widget_name);
    }
    count.set_tooltip_text(Some(&note_count_label(note_count)));
    count.add_css_class("category-count-badge");
    row.append(&count);
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
    content.add_css_class("category-row-content");
    content.set_margin_start(10);
    content.set_margin_end(10);
    content.set_margin_top(6);
    content.set_margin_bottom(6);
    let primary_content = category_primary_content(category, note_count);
    install_active_row_navigation(
        &primary_content,
        dispatcher,
        split_view,
        route,
        Some(category.id),
    );
    content.append(&primary_content);
    row.set_child(Some(&content));
    row
}

fn category_primary_content(category: &Category, note_count: usize) -> gtk::Box {
    let primary_content = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    primary_content.set_hexpand(true);
    primary_content.append(&category_icon_tile(category));
    let name = gtk::Label::new(Some(&category.name));
    name.set_xalign(0.0);
    name.set_ellipsize(gtk::pango::EllipsizeMode::End);
    name.set_single_line_mode(true);
    name.set_hexpand(true);
    name.add_css_class("category-card-title");
    primary_content.append(&name);
    let count_label = gtk::Label::new(Some(&note_count.to_string()));
    count_label.set_widget_name(&format!("category-count:{}", category.id));
    count_label.set_tooltip_text(Some(&note_count_label(note_count)));
    count_label.add_css_class("category-count-badge");
    primary_content.append(&count_label);
    primary_content
}

fn sidebar_icon_tile(icon_name: &str, color_class: &str) -> gtk::Box {
    let tile = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    tile.add_css_class("category-icon-tile");
    tile.add_css_class(color_class);
    tile.set_halign(gtk::Align::Fill);
    tile.set_valign(gtk::Align::Fill);
    tile.set_homogeneous(true);
    let image = gtk::Image::from_icon_name(icon_name);
    image.set_pixel_size(16);
    image.set_halign(gtk::Align::Center);
    image.set_valign(gtk::Align::Center);
    tile.append(&image);
    tile
}

fn category_icon_tile(category: &Category) -> gtk::Box {
    let color = category.appearance.color.resolved_for(category.id);
    sidebar_icon_tile(
        category_icon_name(category.appearance.icon),
        category_color_css_class(color),
    )
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

fn note_count_label(note_count: usize) -> String {
    if note_count == 1 {
        "1 note".to_owned()
    } else {
        format!("{note_count} notes")
    }
}
