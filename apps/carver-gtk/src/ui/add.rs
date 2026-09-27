//! Shared sidebar creation dialog and transient forms.

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use super::dialogs::category_form;
use crate::mvu::{ActionMsg, AppDispatcher, AppMsg, BasesMsg, RequestId};
use carver_sdk::CategoryAppearance;
use gettextrs::{gettext, pgettext};
use gtk::prelude::*;
use libadwaita::{self as adw, prelude::*};

/// The open Add dialog, shared with the view so Base-configuration effects can
/// mount the Base form into its dedicated tab.
pub(crate) struct AddDialogHost {
    pub(crate) dialog: adw::Dialog,
    pub(crate) stack: adw::ViewStack,
    pub(crate) base_slot: gtk::Box,
    pub(crate) base_footer: gtk::Box,
    pub(crate) base_requested: Cell<bool>,
    pub(crate) base_ready: Cell<bool>,
    pub(crate) base_dialog_id: Cell<Option<RequestId>>,
    pub(crate) form: RefCell<Option<crate::ui::bases::actions::BaseConfigurationForm>>,
}

impl AddDialogHost {
    /// Keeps the dialog width fixed while matching the visible tab's height.
    ///
    /// `AdwDialog` cannot follow only the content height once `content-width` is
    /// set, so the preferred height is measured from the visible page instead.
    pub(crate) fn sync_height(&self) {
        let Some(page) = self.stack.visible_child() else {
            return;
        };
        let width = if page.width() > 0 {
            page.width()
        } else {
            self.dialog.content_width().max(1)
        };
        let (_, natural, _, _) = page.measure(gtk::Orientation::Vertical, width);
        let available = self
            .dialog
            .root()
            .and_downcast::<gtk::Window>()
            .map_or(natural, |window| window.height());
        self.dialog
            .set_content_height(natural.clamp(1, i32::max(available - 48, 1)));
    }
}

/// Slot holding the currently open Add dialog, if any.
pub(crate) type AddDialogSlot = Rc<RefCell<Option<Rc<AddDialogHost>>>>;

/// Builds the sidebar's Add button and its tabbed creation dialog.
pub(crate) fn button(dispatcher: &AppDispatcher, slot: AddDialogSlot) -> gtk::Button {
    let button = gtk::Button::builder()
        .icon_name("list-add-symbolic")
        .tooltip_text(gettext("Add"))
        .build();
    button.set_widget_name("sidebar-add-button");
    button.update_property(&[gtk::accessible::Property::Label(&gettext("Add"))]);
    let dispatcher = dispatcher.clone();
    button.connect_clicked(move |button| {
        let Some(parent) = button.root().and_downcast::<gtk::Window>() else {
            return;
        };
        let dialog = adw::Dialog::builder()
            .title(gettext("Add"))
            .content_width(560)
            .build();
        dialog.set_widget_name("sidebar-add-dialog");
        let stack = adw::ViewStack::new();
        stack.set_widget_name("add-stack");
        stack.set_hhomogeneous(true);
        stack.set_vhomogeneous(false);
        let host = Rc::new(AddDialogHost {
            dialog: dialog.clone(),
            stack: stack.clone(),
            base_slot: gtk::Box::new(gtk::Orientation::Vertical, 0),
            base_footer: gtk::Box::new(gtk::Orientation::Horizontal, 0),
            base_requested: Cell::new(false),
            base_ready: Cell::new(false),
            base_dialog_id: Cell::new(None),
            form: RefCell::new(None),
        });
        dialog.set_child(Some(&dialog_content(&dispatcher, &host)));
        *slot.borrow_mut() = Some(Rc::clone(&host));

        let slot_for_close = Rc::clone(&slot);
        let dispatcher_for_close = dispatcher.clone();
        dialog.connect_closed(move |_| {
            let host = slot_for_close.borrow_mut().take();
            let Some(host) = host else {
                return;
            };
            if let Some(dialog_id) = host.base_dialog_id.get() {
                let _ = dispatcher_for_close
                    .dispatch(AppMsg::Bases(BasesMsg::ConfigurationDismissed(dialog_id)));
            } else if host.base_requested.get() {
                // The Base tab was still loading; drop its pending request so it
                // cannot reappear as the standalone dialog.
                let _ =
                    dispatcher_for_close.dispatch(AppMsg::Bases(BasesMsg::CancelNewConfiguration));
            }
        });
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

fn dialog_content(dispatcher: &AppDispatcher, host: &Rc<AddDialogHost>) -> adw::ToolbarView {
    let stack = &host.stack;
    let switcher = adw::ViewSwitcher::new();
    switcher.set_widget_name("add-switcher");
    switcher.set_policy(adw::ViewSwitcherPolicy::Wide);
    switcher.set_stack(Some(stack));

    let category = category_page(&host.dialog, dispatcher);
    stack.add_titled(
        &category,
        Some("category"),
        &pgettext("add dialog", "Category"),
    );
    stack.page(&category).set_icon_name(Some("folder-symbolic"));
    let base = base_page(host);
    stack.add_titled(&base, Some("base"), &pgettext("add dialog", "Base"));
    stack
        .page(&base)
        .set_icon_name(Some("carver-database-symbolic"));

    let weak_host = Rc::downgrade(host);
    let dispatcher_for_tab = dispatcher.clone();
    stack.connect_visible_child_name_notify(move |stack| {
        if stack.visible_child_name().as_deref() == Some("base")
            && let Some(host) = weak_host.upgrade()
            && !host.base_requested.get()
        {
            host.base_requested.set(true);
            let _ = dispatcher_for_tab.dispatch(AppMsg::Bases(BasesMsg::ConfigureNew));
        }
        if let Some(host) = weak_host.upgrade() {
            host.sync_height();
        }
    });

    let header = adw::HeaderBar::new();
    header.set_title_widget(Some(&switcher));

    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&header);
    toolbar.set_content(Some(stack));
    toolbar
}

/// Builds the New Category tab with its Create action.
fn category_page(dialog: &adw::Dialog, dispatcher: &AppDispatcher) -> adw::ToolbarView {
    let category = category_form("", CategoryAppearance::default());
    category.content.add_css_class("sidebar-add-panel");
    let submitted = Rc::new(Cell::new(false));
    let create = gtk::Button::with_label(&gettext("Create"));
    create.set_widget_name("add-category-create");
    create.add_css_class("suggested-action");
    create.set_sensitive(!category.entry.text().trim().is_empty());
    {
        let create = create.clone();
        category.entry.connect_changed(move |entry| {
            create.set_sensitive(!entry.text().trim().is_empty());
        });
    }
    {
        let create = create.clone();
        category.entry.connect_entry_activated(move |_| {
            create.emit_clicked();
        });
    }
    {
        let entry = category.entry.clone();
        let icon = Rc::clone(&category.icon);
        let color = Rc::clone(&category.color);
        let submitted = Rc::clone(&submitted);
        let dispatcher = dispatcher.clone();
        let weak_dialog = dialog.downgrade();
        create.connect_clicked(move |_| {
            let name = entry.text().trim().to_owned();
            if name.is_empty() || submitted.replace(true) {
                return;
            }
            if let Some(dialog) = weak_dialog.upgrade() {
                dialog.close();
            }
            let _ = dispatcher.dispatch(AppMsg::Action(ActionMsg::CreateCategoryWithAppearance {
                name,
                appearance: CategoryAppearance {
                    icon: icon.get(),
                    color: color.get(),
                },
            }));
        });
    }
    let toolbar = adw::ToolbarView::new();
    toolbar.set_content(Some(&category.content));
    let footer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    footer.set_halign(gtk::Align::End);
    footer.set_margin_top(6);
    footer.set_margin_bottom(6);
    footer.set_margin_start(12);
    footer.set_margin_end(12);
    footer.append(&create);
    toolbar.add_bottom_bar(&footer);
    toolbar
}

/// Builds the New Base tab, which shows a spinner until its form is mounted.
fn base_page(host: &Rc<AddDialogHost>) -> adw::ToolbarView {
    let spinner = gtk::Spinner::new();
    spinner.set_widget_name("add-base-spinner");
    spinner.start();
    let placeholder = gtk::Box::new(gtk::Orientation::Vertical, 0);
    placeholder.set_height_request(360);
    placeholder.set_vexpand(true);
    placeholder.set_valign(gtk::Align::Center);
    placeholder.set_halign(gtk::Align::Center);
    placeholder.append(&spinner);
    host.base_slot.set_widget_name("add-base-slot");
    host.base_slot.append(&placeholder);
    let toolbar = adw::ToolbarView::new();
    toolbar.set_content(Some(&host.base_slot));
    host.base_footer.set_halign(gtk::Align::End);
    host.base_footer.set_margin_top(6);
    host.base_footer.set_margin_bottom(6);
    host.base_footer.set_margin_start(12);
    host.base_footer.set_margin_end(12);
    toolbar.add_bottom_bar(&host.base_footer);
    toolbar
}
