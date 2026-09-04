//! In-app trash browser and recovery actions.

use std::rc::Rc;

use adw::prelude::*;
use gtk::prelude::*;
use libadwaita as adw;

use crate::{
    controller::AppState,
    mvu::{AppMsg, NavigationMsg, TrashMsg},
};

/// Builds the recoverable in-app trash page.
pub(crate) fn build_trash(state: &Rc<AppState>, stack: &gtk::Stack) -> gtk::Widget {
    let view = adw::ToolbarView::new();
    let header = adw::HeaderBar::new();
    let back = gtk::Button::from_icon_name("go-previous-symbolic");
    back.set_widget_name("back-from-trash-button");
    back.set_tooltip_text(Some("Back to notes"));
    let stack_for_back = stack.clone();
    let state_for_back = Rc::clone(state);
    back.connect_clicked(move |_| {
        if !state_for_back.dispatch_mvu(AppMsg::Navigation(NavigationMsg::ShowBrowser)) {
            stack_for_back.set_visible_child_name("browser");
        }
    });
    header.pack_start(&back);
    header.set_title_widget(Some(&adw::WindowTitle::new(
        "Trash",
        "Restore notes and categories",
    )));
    let empty = gtk::Button::with_label("Empty Trash");
    empty.set_widget_name("empty-trash-button");
    empty.add_css_class("destructive-action");
    header.pack_end(&empty);
    view.add_top_bar(&header);

    let pages = gtk::Stack::new();
    let status = adw::StatusPage::builder()
        .title("Trash is empty")
        .description("Deleted notes and categories can be restored here.")
        .icon_name("user-trash-symbolic")
        .build();
    pages.add_named(&status, Some("empty"));
    let list = gtk::ListBox::new();
    list.set_widget_name("trash-list");
    list.set_selection_mode(gtk::SelectionMode::None);
    list.add_css_class("note-feed");
    let scroll = gtk::ScrolledWindow::new();
    scroll.set_vexpand(true);
    scroll.set_child(Some(&list));
    let clamp = adw::Clamp::new();
    clamp.set_maximum_size(720);
    clamp.set_tightening_threshold(520);
    clamp.set_child(Some(&scroll));
    pages.add_named(&clamp, Some("contents"));
    view.set_content(Some(&pages));

    state.trash_list.replace(Some(list));
    state.trash_content_stack.replace(Some(pages));
    state.trash_status.replace(Some(status));
    state.empty_trash_button.replace(Some(empty.clone()));
    connect_empty_action(state, &empty);
    refresh_trash(state);
    view.upcast()
}

/// Refreshes visible trash data after a recovery or deletion action.
pub(crate) fn refresh_trash(state: &Rc<AppState>) {
    let _ = state.dispatch_mvu(AppMsg::Trash(TrashMsg::Reload));
}

fn connect_empty_action(state: &Rc<AppState>, button: &gtk::Button) {
    let state_for_empty = Rc::clone(state);
    button.connect_clicked(move |button| {
        let dialog = adw::AlertDialog::new(
            Some("Empty Trash?"),
            Some(
            "All trashed notes, categories, and unreferenced images will be permanently deleted.",
            ),
        );
        dialog.add_responses(&[("cancel", "Cancel"), ("empty", "Empty Trash")]);
        dialog.set_response_appearance("empty", adw::ResponseAppearance::Destructive);
        dialog.set_default_response(Some("cancel"));
        dialog.set_close_response("cancel");
        let state = Rc::clone(&state_for_empty);
        dialog.connect_response(None, move |_dialog, response| {
            if response == "empty" {
                let _ = state.dispatch_mvu(AppMsg::Trash(TrashMsg::Empty));
            }
        });
        dialog.present(button.root().as_ref());
    });
}
