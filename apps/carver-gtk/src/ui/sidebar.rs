//! Category sidebar construction and snapshot rendering.

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use adw::prelude::*;
use carver_sdk::{BaseDefinition, BaseId, CategoryId, CategorySummary};
use gettextrs::{gettext, ngettext};
use gtk::prelude::*;
use libadwaita as adw;

use super::dialogs::{category_color_css_class, category_icon_name};
use crate::mvu::{
    AppDispatcher, AppModel, AppMsg, BasesMsg, LoadState, NavigationMsg, SidebarSelection,
};

/// One navigation destination rendered by the sidebar.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SidebarKey {
    /// All notes.
    AllNotes,
    /// One category.
    Category(CategoryId),
    /// One saved Base.
    Base(BaseId),
}

/// Responsive category sidebar and its snapshot renderer.
#[derive(Clone)]
pub(crate) struct SidebarSurface {
    pub(crate) widget: gtk::Widget,
    pub(crate) sidebar: adw::Sidebar,
    rendering: Rc<Cell<bool>>,
    category_section: adw::SidebarSection,
    bases_section: adw::SidebarSection,
    category_keys: Rc<RefCell<Vec<SidebarKey>>>,
    base_keys: Rc<RefCell<Vec<SidebarKey>>>,
    rendered_categories: Rc<RefCell<Option<Vec<CategorySummary>>>>,
    rendered_bases: Rc<RefCell<Vec<BaseDefinition>>>,
}

pub(crate) type CompactNavigation = Rc<Cell<bool>>;

/// Builds a consistently styled Back control that dispatches one MVU message.
pub(crate) fn back_to_notes_button(
    dispatcher: &AppDispatcher,
    widget_name: &str,
    message: AppMsg,
) -> gtk::Button {
    let back = gtk::Button::from_icon_name("go-previous-symbolic");
    back.set_widget_name(widget_name);
    back.set_tooltip_text(Some(&gettext("Back to notes")));
    let dispatcher = dispatcher.clone();
    back.connect_clicked(move |_| {
        let _ = dispatcher.dispatch(message.clone());
    });
    back
}

/// Builds the responsive category sidebar.
pub(crate) fn build_sidebar(
    dispatcher: &AppDispatcher,
    split_view: &adw::NavigationSplitView,
) -> SidebarSurface {
    let container = adw::ToolbarView::new();
    container.set_widget_name("sidebar-surface");
    let header = adw::HeaderBar::new();
    header.pack_start(&super::add::button(dispatcher));
    header.pack_end(&settings_menu_button());
    container.add_top_bar(&header);
    install_sidebar_search_shortcut(&container, dispatcher);

    let sidebar = adw::Sidebar::new();
    sidebar.set_widget_name("category-sidebar");
    let category_section = adw::SidebarSection::new();
    let bases_section = adw::SidebarSection::new();
    sidebar.append(category_section.clone());
    sidebar.append(bases_section.clone());

    let rendering = Rc::new(Cell::new(false));
    let category_keys = Rc::new(RefCell::new(Vec::new()));
    let base_keys = Rc::new(RefCell::new(Vec::new()));
    connect_selection(
        dispatcher,
        split_view,
        &sidebar,
        &rendering,
        &category_keys,
        &base_keys,
    );

    container.set_content(Some(&sidebar));
    container.add_bottom_bar(&trash_footer(dispatcher, split_view));

    SidebarSurface {
        widget: container.upcast(),
        sidebar,
        rendering,
        category_section,
        bases_section,
        category_keys,
        base_keys,
        rendered_categories: Rc::new(RefCell::new(None)),
        rendered_bases: Rc::new(RefCell::new(Vec::new())),
    }
}

/// Captures note search from the sidebar without intercepting editor shortcuts.
fn install_sidebar_search_shortcut(container: &impl IsA<gtk::Widget>, dispatcher: &AppDispatcher) {
    let controller = gtk::EventControllerKey::new();
    controller.set_name(Some("sidebar-search-shortcut"));
    controller.set_propagation_phase(gtk::PropagationPhase::Capture);
    let dispatcher = dispatcher.clone();
    controller.connect_key_pressed(move |_, key, _, modifiers| {
        if key != gtk::gdk::Key::f
            || !modifiers.contains(gtk::gdk::ModifierType::CONTROL_MASK)
            || modifiers.contains(gtk::gdk::ModifierType::SHIFT_MASK)
        {
            return glib::Propagation::Proceed;
        }
        let _ = dispatcher.dispatch(AppMsg::Navigation(NavigationMsg::SearchShortcutRequested));
        glib::Propagation::Stop
    });
    container.add_controller(controller);
}

/// Builds the window-level settings menu shown in the persistent sidebar.
fn settings_menu_button() -> gtk::MenuButton {
    let menu = gtk::gio::Menu::new();
    menu.append(
        Some(&gettext("Connect an agent")),
        Some("win.connect-agent"),
    );
    let settings_section = gtk::gio::Menu::new();
    settings_section.append(Some(&gettext("Preferences")), Some("win.preferences"));
    settings_section.append(
        Some(&gettext("Keyboard Shortcuts")),
        Some(super::dialogs::KEYBOARD_SHORTCUTS_ACTION),
    );
    settings_section.append(Some(&gettext("About Carver")), Some("win.about"));
    menu.append_section(None, &settings_section);
    let settings = gtk::MenuButton::new();
    settings.set_widget_name("sidebar-settings-menu-button");
    settings.set_icon_name("open-menu-symbolic");
    settings.set_tooltip_text(Some(&gettext("Settings")));
    settings.set_menu_model(Some(&menu));
    settings
}

impl SidebarSurface {
    /// Renders sidebar rows from the current application snapshot.
    pub(crate) fn render(&self, model: &AppModel) {
        self.rendering.set(true);
        let LoadState::Ready(categories) = &model.sidebar.state else {
            self.category_section.remove_all();
            self.category_keys.borrow_mut().clear();
            self.rendered_categories.replace(None);
            self.apply_selection(model.sidebar_selection());
            self.rendering.set(false);
            return;
        };

        if self.rendered_categories.borrow().as_ref() != Some(categories) {
            rebuild_category_section(&self.category_section, &self.category_keys, categories);
            self.rendered_categories.replace(Some(categories.clone()));
        }

        if let LoadState::Ready(bases) = &model.bases.definitions.state {
            let changed = *self.rendered_bases.borrow() != *bases;
            if changed {
                rebuild_bases_section(&self.bases_section, &self.base_keys, bases);
                self.rendered_bases.replace(bases.clone());
            }
        }

        self.apply_selection(model.sidebar_selection());
        self.rendering.set(false);
    }

    /// Highlights the destination derived from the current navigation route.
    fn apply_selection(&self, selection: SidebarSelection) {
        self.sidebar.set_selected(self.selection_index(selection));
    }

    /// Maps a model selection onto the flat index used by `AdwSidebar`.
    fn selection_index(&self, selection: SidebarSelection) -> u32 {
        let category_keys = self.category_keys.borrow();
        let base_keys = self.base_keys.borrow();
        let position = match selection {
            SidebarSelection::Category(category) => {
                let key = category.map_or(SidebarKey::AllNotes, SidebarKey::Category);
                category_keys.iter().position(|candidate| *candidate == key)
            }
            SidebarSelection::Base(id) => base_keys
                .iter()
                .position(|candidate| *candidate == SidebarKey::Base(id))
                .map(|index| category_keys.len() + index),
            SidebarSelection::None => None,
        };
        position.map_or(gtk::INVALID_LIST_POSITION, |index| {
            u32::try_from(index).unwrap_or(gtk::INVALID_LIST_POSITION)
        })
    }
}

fn connect_selection(
    dispatcher: &AppDispatcher,
    split_view: &adw::NavigationSplitView,
    sidebar: &adw::Sidebar,
    rendering: &Rc<Cell<bool>>,
    category_keys: &Rc<RefCell<Vec<SidebarKey>>>,
    base_keys: &Rc<RefCell<Vec<SidebarKey>>>,
) {
    let dispatcher = dispatcher.clone();
    let split_view_for_selection = split_view.clone();
    let rendering = Rc::clone(rendering);
    let category_keys = Rc::clone(category_keys);
    let base_keys = Rc::clone(base_keys);
    sidebar.connect_selected_notify(move |sidebar| {
        if rendering.get() {
            return;
        }
        let mut keys = category_keys.borrow().clone();
        keys.extend(base_keys.borrow().iter().copied());
        let Some(key) = keys.get(sidebar.selected() as usize).copied() else {
            return;
        };
        match key {
            SidebarKey::AllNotes => {
                let _ =
                    dispatcher.dispatch(AppMsg::Navigation(NavigationMsg::SelectCategory(None)));
            }
            SidebarKey::Category(id) => {
                let _ = dispatcher
                    .dispatch(AppMsg::Navigation(NavigationMsg::SelectCategory(Some(id))));
            }
            SidebarKey::Base(id) => {
                let _ = dispatcher.dispatch(AppMsg::Bases(BasesMsg::Open(id)));
            }
        }
        if split_view_for_selection.is_collapsed() {
            split_view_for_selection.set_show_content(true);
        }
    });

    let split_view_for_activation = split_view.clone();
    sidebar.connect_activated(move |_, _| {
        if split_view_for_activation.is_collapsed() {
            split_view_for_activation.set_show_content(true);
        }
    });
}

fn rebuild_category_section(
    section: &adw::SidebarSection,
    keys: &Rc<RefCell<Vec<SidebarKey>>>,
    categories: &[CategorySummary],
) {
    section.remove_all();
    let all_notes_count = categories.iter().map(|summary| summary.note_count).sum();
    section.append(all_notes_item(all_notes_count));
    let mut new_keys = Vec::with_capacity(categories.len() + 1);
    new_keys.push(SidebarKey::AllNotes);
    for summary in categories {
        section.append(category_item(&summary.category, summary.note_count));
        new_keys.push(SidebarKey::Category(summary.category.id));
    }
    *keys.borrow_mut() = new_keys;
}

fn rebuild_bases_section(
    section: &adw::SidebarSection,
    keys: &Rc<RefCell<Vec<SidebarKey>>>,
    bases: &[BaseDefinition],
) {
    section.remove_all();
    let mut new_keys = Vec::with_capacity(bases.len());
    for base in bases {
        section.append(base_item(base));
        new_keys.push(SidebarKey::Base(base.id));
    }
    *keys.borrow_mut() = new_keys;
}

fn all_notes_item(note_count: usize) -> adw::SidebarItem {
    let item = adw::SidebarItem::new(&gettext("All notes"));
    item.set_icon_name(Some("go-home-symbolic"));
    item.set_suffix(Some(&count_badge(
        "all-notes-count",
        note_count,
        Some("all-notes-icon"),
    )));
    item
}

fn category_item(category: &carver_sdk::Category, note_count: usize) -> adw::SidebarItem {
    let item = adw::SidebarItem::new(&category.name);
    item.set_icon_name(Some(category_icon_name(category.appearance.icon)));
    let color = category.appearance.color.resolved_for(category.id);
    item.set_suffix(Some(&count_badge(
        &format!("category-count:{}", category.id),
        note_count,
        Some(category_color_css_class(color)),
    )));
    item.set_tooltip(Some(&note_count_label(note_count)));
    item
}

fn base_item(base: &BaseDefinition) -> adw::SidebarItem {
    let item = adw::SidebarItem::new(&base.name);
    item.set_icon_name(Some("carver-database-symbolic"));
    item.set_suffix(Some(&count_badge(
        &format!("base-count:{}", base.id),
        base.row_count,
        None,
    )));
    item
}

/// Builds the note-count badge, optionally tinted with a category colour.
fn count_badge(widget_name: &str, note_count: usize, color_class: Option<&str>) -> gtk::Label {
    let badge = gtk::Label::new(Some(&badge_text(note_count)));
    badge.set_widget_name(widget_name);
    badge.set_tooltip_text(Some(&note_count_label(note_count)));
    badge.set_valign(gtk::Align::Center);
    badge.set_halign(gtk::Align::Center);
    badge.add_css_class("category-count-badge");
    if note_count > 99 {
        badge.add_css_class("capped");
    }
    if let Some(color_class) = color_class {
        badge.add_css_class(color_class);
    }
    badge
}

/// Caps the badge text so the pill stays a circle at every note count.
fn badge_text(note_count: usize) -> String {
    if note_count > 99 {
        "99+".to_owned()
    } else {
        note_count.to_string()
    }
}

/// Builds the shared control that expands or collapses the category sidebar.
pub(crate) fn sidebar_toggle_button(
    split_view: &adw::NavigationSplitView,
    compact_navigation: &CompactNavigation,
    widget_name: &str,
) -> gtk::ToggleButton {
    let toggle = gtk::ToggleButton::new();
    toggle.set_icon_name("sidebar-show-symbolic");
    toggle.set_widget_name(widget_name);
    toggle.set_tooltip_text(Some(&gettext("Hide Categories")));
    toggle.set_active(!split_view.is_collapsed());
    let split = split_view.clone();
    let compact_navigation = Rc::clone(compact_navigation);
    let resetting = Rc::new(Cell::new(false));
    let resetting_for_toggle = Rc::clone(&resetting);
    toggle.connect_toggled(move |button| {
        if resetting_for_toggle.get() {
            return;
        }
        if compact_navigation.get() {
            resetting_for_toggle.set(true);
            button.set_active(false);
            resetting_for_toggle.set(false);
            split.set_show_content(false);
            return;
        }
        if button.is_active() {
            split.set_collapsed(false);
        } else {
            split.set_collapsed(true);
            split.set_show_content(true);
        }
    });
    let toggle_for_state = toggle.clone();
    let resetting_for_state = Rc::clone(&resetting);
    split_view.connect_collapsed_notify(move |split| {
        resetting_for_state.set(true);
        if split.is_collapsed() {
            toggle_for_state.set_active(false);
            toggle_for_state.set_tooltip_text(Some(&gettext("Show Categories")));
        } else {
            toggle_for_state.set_active(true);
            toggle_for_state.set_tooltip_text(Some(&gettext("Hide Categories")));
        }
        resetting_for_state.set(false);
    });
    toggle
}

fn trash_footer(dispatcher: &AppDispatcher, split_view: &adw::NavigationSplitView) -> gtk::Widget {
    let trash = gtk::Button::new();
    trash.set_widget_name("open-trash-button");
    trash.set_tooltip_text(Some(&gettext("Open Trash")));
    trash.add_css_class("flat");
    let trash_content = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    trash_content.set_margin_start(12);
    trash_content.set_margin_end(12);
    trash_content.set_margin_top(8);
    trash_content.set_margin_bottom(8);
    trash_content.append(&gtk::Image::from_icon_name("user-trash-symbolic"));
    let trash_label = gtk::Label::new(Some(&gettext("Trash")));
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

fn note_count_label(note_count: usize) -> String {
    tr_fmt!(
        ngettext(
            "{count} note",
            "{count} notes",
            u32::try_from(note_count).unwrap_or(u32::MAX),
        ),
        count = note_count
    )
}

#[cfg(test)]
mod tests;
