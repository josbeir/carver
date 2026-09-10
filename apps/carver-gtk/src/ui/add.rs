//! Shared sidebar creation chooser and transient forms.

use super::dialogs::category_form;
use crate::mvu::{ActionMsg, AppDispatcher, AppMsg, BasesMsg};
use carver_sdk::{BaseColumn, CategoryAppearance};
use gtk::prelude::*;
use libadwaita::{self as adw, prelude::*};
use std::{cell::Cell, rc::Rc};

pub(crate) fn button(dispatcher: &AppDispatcher) -> gtk::Button {
    let button = gtk::Button::builder()
        .icon_name("list-add-symbolic")
        .tooltip_text("Add")
        .build();
    button.set_widget_name("sidebar-add-button");
    button.update_property(&[gtk::accessible::Property::Label("Add")]);
    let dispatcher = dispatcher.clone();
    button.connect_clicked(move |button| {
        let Some(parent) = button.root().and_downcast::<gtk::Window>() else {
            return;
        };
        let dialog = adw::Dialog::builder()
            .title("Add")
            .follows_content_size(true)
            .build();
        dialog.set_widget_name("sidebar-add-dialog");
        let toolbar = adw::ToolbarView::new();
        toolbar.add_top_bar(&adw::HeaderBar::new());
        toolbar.set_content(Some(&content(&dialog, &dispatcher)));
        dialog.set_child(Some(&toolbar));
        let weak_button = button.downgrade();
        dialog.connect_closed(move |_| {
            if let Some(button) = weak_button.upgrade() {
                button.grab_focus();
            }
        });
        dialog.present(Some(&parent));
    });
    button
}

fn content(dialog: &adw::Dialog, dispatcher: &AppDispatcher) -> gtk::ScrolledWindow {
    let stack = gtk::Stack::builder()
        .transition_type(gtk::StackTransitionType::SlideLeftRight)
        .transition_duration(180)
        .vhomogeneous(false)
        .hhomogeneous(false)
        .build();
    stack.set_widget_name("add-pages");
    let chooser = gtk::Box::new(gtk::Orientation::Vertical, 12);
    let heading = gtk::Label::new(Some("What would you like to add?"));
    heading.add_css_class("heading");
    heading.set_wrap(true);
    chooser.append(&heading);
    stack.add_named(&chooser, Some("choose"));
    let category = category_form("", CategoryAppearance::default());
    let base_entry = gtk::Entry::builder().placeholder_text("Base name").build();
    base_entry.set_widget_name("base-name-entry");
    let base_content = gtk::Box::new(gtk::Orientation::Vertical, 12);
    base_content.append(&base_entry);
    let submitted = Rc::new(Cell::new(false));
    for (name, title, icon, description, example, form, entry) in [
        (
            "category",
            "Category",
            "folder-symbolic",
            "Keep related notes together in one place.",
            "Examples: Work, Personal, Research",
            &category.content,
            &category.entry,
        ),
        (
            "base",
            "Base",
            "carver-database-symbolic",
            "Build a custom view of your notes with chosen fields, a query, and a sort order.",
            "Examples: Project tracker, Reading list",
            &base_content,
            &base_entry,
        ),
    ] {
        let choice = choice_card(name, title, icon, description, example);
        chooser.append(&choice);
        let page = gtk::Box::new(gtk::Orientation::Vertical, 14);
        let back = gtk::Button::from_icon_name("go-previous-symbolic");
        back.set_widget_name(&format!("add-{name}-back"));
        back.set_tooltip_text(Some("Back"));
        back.add_css_class("flat");
        let header = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        header.append(&back);
        let title = gtk::Label::new(Some(&format!("New {title}")));
        title.add_css_class("heading");
        header.append(&title);
        page.append(&header);
        page.append(form);
        let create = gtk::Button::with_label("Create");
        create.set_widget_name(&format!("add-{name}-create"));
        create.add_css_class("suggested-action");
        create.set_sensitive(false);
        page.append(&create);
        stack.add_named(&page, Some(name));
        connect_form_navigation(&stack, name, &choice, &back, entry, &create);
        let entry = entry.clone();
        let icon = Rc::clone(&category.icon);
        let color = Rc::clone(&category.color);
        let submitted = Rc::clone(&submitted);
        let dispatcher = dispatcher.clone();
        let weak_dialog = dialog.downgrade();
        create.connect_clicked(move |_| {
            let value = entry.text().trim().to_owned();
            if value.is_empty() || submitted.replace(true) {
                return;
            }
            if let Some(dialog) = weak_dialog.upgrade() {
                dialog.close();
            }
            let message = if name == "category" {
                AppMsg::Action(ActionMsg::CreateCategoryWithAppearance {
                    name: value,
                    appearance: CategoryAppearance {
                        icon: icon.get(),
                        color: color.get(),
                    },
                })
            } else {
                AppMsg::Bases(BasesMsg::Create {
                    name: value,
                    columns: vec![BaseColumn::Category, BaseColumn::Updated],
                })
            };
            let _ = dispatcher.dispatch(message);
        });
    }
    let scroll = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vscrollbar_policy(gtk::PolicyType::Automatic)
        .propagate_natural_height(true)
        .propagate_natural_width(true)
        .child(&stack)
        .build();
    scroll.add_css_class("sidebar-add-panel");
    scroll
}

fn connect_form_navigation(
    stack: &gtk::Stack,
    name: &'static str,
    choice: &gtk::Button,
    back: &gtk::Button,
    entry: &gtk::Entry,
    create: &gtk::Button,
) {
    let weak_stack = stack.downgrade();
    let entry_for_focus = entry.clone();
    choice.connect_clicked(move |_| {
        if let Some(stack) = weak_stack.upgrade() {
            stack.set_visible_child_name(name);
        }
        entry_for_focus.grab_focus();
    });
    let weak_stack = stack.downgrade();
    let weak_choice = choice.downgrade();
    back.connect_clicked(move |_| {
        if let Some(stack) = weak_stack.upgrade() {
            stack.set_visible_child_name("choose");
        }
        if let Some(choice) = weak_choice.upgrade() {
            choice.grab_focus();
        }
    });
    let weak_create = create.downgrade();
    entry.connect_changed(move |entry| {
        if let Some(create) = weak_create.upgrade() {
            create.set_sensitive(!entry.text().trim().is_empty());
        }
    });
    let weak_create = create.downgrade();
    entry.connect_activate(move |_| {
        if let Some(create) = weak_create.upgrade().filter(WidgetExt::is_sensitive) {
            create.emit_clicked();
        }
    });
}

fn choice_card(
    name: &str,
    title: &str,
    icon: &str,
    description: &str,
    example: &str,
) -> gtk::Button {
    let button = gtk::Button::new();
    button.set_widget_name(&format!("add-{name}-choice"));
    button.add_css_class("add-choice");
    button.update_property(&[
        gtk::accessible::Property::Label(title),
        gtk::accessible::Property::Description(description),
    ]);
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    let image = gtk::Image::from_icon_name(icon);
    image.set_pixel_size(24);
    image.set_valign(gtk::Align::Start);
    image.add_css_class("add-choice-icon");
    row.append(&image);
    let text = gtk::Box::new(gtk::Orientation::Vertical, 6);
    text.set_hexpand(true);
    for (value, class) in [
        (title, "heading"),
        (description, "add-description"),
        (example, "dim-label"),
    ] {
        let label = gtk::Label::new(Some(value));
        label.set_xalign(0.0);
        label.set_wrap(true);
        label.set_wrap_mode(gtk::pango::WrapMode::WordChar);
        label.set_max_width_chars(34);
        label.add_css_class(class);
        text.append(&label);
    }
    row.append(&text);
    row.append(&gtk::Image::from_icon_name("go-next-symbolic"));
    button.set_child(Some(&row));
    button
}
