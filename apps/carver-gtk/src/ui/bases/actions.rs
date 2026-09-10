//! Contextual actions for saved Base definitions.
use crate::mvu::{AppDispatcher, AppMsg, BasesMsg};
use carver_sdk::BaseDefinition;
use gtk::prelude::*;
use libadwaita::{self as adw, prelude::*};

pub(crate) fn install(button: &gtk::Button, base: &BaseDefinition, dispatcher: &AppDispatcher) {
    let base = base.clone();
    let dispatcher = dispatcher.clone();
    let weak_button = button.downgrade();
    let show = std::rc::Rc::new(move || {
        if let Some(button) = weak_button.upgrade() {
            show_menu(&button, &base, &dispatcher);
        }
    });
    let click = gtk::GestureClick::new();
    click.set_button(3);
    let show_for_click = show.clone();
    click.connect_pressed(move |gesture, _, _, _| {
        show_for_click();
        gesture.set_state(gtk::EventSequenceState::Claimed);
    });
    button.add_controller(click);
    let keys = gtk::EventControllerKey::new();
    keys.connect_key_pressed(move |_, key, _, modifiers| {
        if key == gtk::gdk::Key::Menu
            || (key == gtk::gdk::Key::F10 && modifiers.contains(gtk::gdk::ModifierType::SHIFT_MASK))
        {
            show();
            glib::Propagation::Stop
        } else {
            glib::Propagation::Proceed
        }
    });
    button.add_controller(keys);
    button.connect_unrealize(|button| {
        if let Some(menu) = button.last_child().and_downcast::<gtk::Popover>() {
            menu.popdown();
        }
    });
}

fn show_menu(button: &gtk::Button, base: &BaseDefinition, dispatcher: &AppDispatcher) {
    let menu = gtk::Popover::new();
    menu.set_parent(button);
    let delete = gtk::Button::with_label("Delete Base…");
    delete.set_widget_name("delete-base-action");
    delete.add_css_class("flat");
    menu.set_child(Some(&delete));
    menu.connect_closed(WidgetExt::unparent);
    let weak_menu = menu.downgrade();
    let weak_button = button.downgrade();
    let base = base.clone();
    let dispatcher = dispatcher.clone();
    delete.connect_clicked(move |_| {
        if let Some(menu) = weak_menu.upgrade() { menu.popdown(); }
        let Some(parent) = weak_button.upgrade().and_then(|button| button.root()).and_downcast::<gtk::Window>() else { return; };
        let dialog = adw::AlertDialog::builder().heading(format!("Delete “{}”?", base.name))
            .body("Only this Base will be deleted. Your notes and their properties will not be changed.")
            .close_response("cancel").default_response("cancel").build();
        dialog.set_widget_name("delete-base-confirmation");
        dialog.add_responses(&[("cancel", "Cancel"), ("delete", "Delete Base")]);
        dialog.set_response_appearance("delete", adw::ResponseAppearance::Destructive);
        let dispatcher = dispatcher.clone();
        let id = base.id;
        dialog.connect_response(None, move |_, response| {
            if response == "delete" { let _ = dispatcher.dispatch(AppMsg::Bases(BasesMsg::Delete(id))); }
        });
        dialog.present(Some(&parent));
    });
    menu.popup();
}
