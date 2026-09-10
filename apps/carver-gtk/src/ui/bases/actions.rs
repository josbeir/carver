//! Confirmed deletion from the current Base's header.
use crate::mvu::{AppDispatcher, AppMsg, BasesMsg};
use carver_sdk::BaseDefinition;
use gtk::prelude::*;
use libadwaita::{self as adw, prelude::*};

pub(crate) fn render_delete(
    button: &gtk::Button,
    base: Option<&BaseDefinition>,
    dispatcher: &AppDispatcher,
) {
    let group = gtk::gio::SimpleActionGroup::new();
    let action = gtk::gio::SimpleAction::new("delete", None);
    action.set_enabled(base.is_some());
    if let Some(base) = base {
        let id = base.id;
        let name = base.name.clone();
        let dispatcher = dispatcher.clone();
        let weak_button = button.downgrade();
        action.connect_activate(move |_, _| {
            let Some(parent) = weak_button.upgrade().and_then(|button| button.root()).and_downcast::<gtk::Window>() else { return; };
            let dialog = adw::AlertDialog::builder().heading(format!("Delete “{name}”?"))
                .body("Only this Base will be deleted. Your notes and their properties will not be changed.")
                .close_response("cancel").default_response("cancel").build();
            dialog.set_widget_name("delete-base-confirmation");
            dialog.add_responses(&[("cancel", "Cancel"), ("delete", "Delete Base")]);
            dialog.set_response_appearance("delete", adw::ResponseAppearance::Destructive);
            let dispatcher = dispatcher.clone();
            dialog.connect_response(None, move |_, response| {
                if response == "delete" { let _ = dispatcher.dispatch(AppMsg::Bases(BasesMsg::Delete(id))); }
            });
            dialog.present(Some(&parent));
        });
    }
    group.add_action(&action);
    button.insert_action_group("base", Some(&group));
}
