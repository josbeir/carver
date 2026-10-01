//! Compact native template choice and effective new-note properties.
use super::{ListHandle, properties::PropertiesHandle};
use crate::mvu::{AppDispatcher, AppMsg, RequestId, TemplatePreview, TemplatesMsg, UiError};
use carver_sdk::NoteTemplate;
use gettextrs::gettext;
use gtk::prelude::*;
use libadwaita::{self as adw, prelude::*};

pub(super) struct PreviewHandle {
    create: gtk::Button,
    properties: PropertiesHandle,
    inserting: bool,
}
impl PreviewHandle {
    pub(super) fn render(&self, preview: Option<&Result<TemplatePreview, UiError>>) {
        self.create.set_sensitive(matches!(preview, Some(Ok(_))));
        if let Some(preview) = preview {
            self.properties.render(preview.as_ref());
        }
        self.properties.group.set_visible(
            preview.is_some()
                && (!self.inserting
                    || preview.is_some_and(|p| p.as_ref().is_ok_and(|p| !p.properties.is_empty()))),
        );
    }
}

pub(super) fn show(
    parent: &gtk::Window,
    dispatcher: &AppDispatcher,
    request_id: RequestId,
    templates: &[NoteTemplate],
    initial_preview: Option<&Result<TemplatePreview, UiError>>,
    selected: Option<carver_sdk::TemplateId>,
    inserting: bool,
) -> ListHandle {
    let dialog = adw::Dialog::builder()
        .title(if inserting {
            gettext("Insert Template")
        } else {
            gettext("Choose Template")
        })
        .content_width(560)
        .content_height(480)
        .build();
    dialog.set_widget_name("templates-dialog");
    let toolbar = adw::ToolbarView::new();
    let header = adw::HeaderBar::new();
    header.set_show_start_title_buttons(false);
    header.set_show_end_title_buttons(false);
    header.pack_start(&cancel_button(&dialog));
    let create = gtk::Button::with_label(&if inserting {
        gettext("Insert")
    } else {
        gettext("Create Note")
    });
    create.set_widget_name("template-create-note");
    create.add_css_class("suggested-action");
    header.pack_end(&create);
    let d = dispatcher.clone();
    let weak = dialog.downgrade();
    create.connect_clicked(move |button| {
        button.set_sensitive(false);
        let _ = d.dispatch(AppMsg::Templates(TemplatesMsg::CreateSelected(request_id)));
        if let Some(dialog) = weak.upgrade() {
            dialog.close();
        }
    });
    toolbar.add_top_bar(&header);
    let content = gtk::Box::new(gtk::Orientation::Vertical, 18);
    super::set_margins(&content);
    let names: Vec<_> = templates.iter().map(|t| t.name.as_str()).collect();
    let selector = adw::ComboRow::builder()
        .title(gettext("Choose Template"))
        .model(&gtk::StringList::new(&names))
        .enable_search(true)
        .build();
    selector.set_use_markup(false);
    selector.set_widget_name("template-picker-choice");
    selector.set_selected(
        selected
            .and_then(|id| templates.iter().position(|t| t.id == id))
            .and_then(|index| u32::try_from(index).ok())
            .unwrap_or(gtk::INVALID_LIST_POSITION),
    );
    let group = adw::PreferencesGroup::new();
    group.add(&selector);
    content.append(&group);
    let properties = PropertiesHandle::new(true);
    if inserting {
        properties.group.set_title(&gettext("Properties to add"));
    }
    content.append(&properties.group);
    content.append(&super::help_label(&if inserting {
        gettext("Inserts at the cursor and adds missing properties. Existing property values stay unchanged.")
    } else { gettext("New notes get their own copy. Editing a template won’t change existing notes.") }));
    content.append(&manage_button(&dialog, dispatcher));
    let scroll = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .child(&content)
        .build();
    scroll.set_widget_name("template-picker-scroll");
    toolbar.set_content(Some(&scroll));
    dialog.set_child(Some(&toolbar));
    let ids: Vec<_> = templates.iter().map(|t| t.id).collect();
    let d = dispatcher.clone();
    selector.connect_selected_notify(move |selector| {
        let id = usize::try_from(selector.selected())
            .ok()
            .and_then(|index| ids.get(index))
            .copied();
        let _ = d.dispatch(AppMsg::Templates(TemplatesMsg::SelectPreview {
            request_id,
            id,
        }));
    });
    let preview = PreviewHandle {
        create,
        properties,
        inserting,
    };
    preview.render(initial_preview);
    let d = dispatcher.clone();
    dialog.connect_closed(move |_| {
        let _ = d.dispatch(AppMsg::Templates(TemplatesMsg::PickerClosed(request_id)));
    });
    dialog.present(Some(parent));
    ListHandle {
        id: request_id,
        dialog,
        preview: Some(preview),
    }
}

fn manage_button(dialog: &adw::Dialog, dispatcher: &AppDispatcher) -> gtk::Button {
    let manage = gtk::Button::with_label(&gettext("Manage Templates…"));
    manage.set_widget_name("picker-manage-templates");
    manage.set_halign(gtk::Align::Start);
    manage.add_css_class("flat");
    let weak = dialog.downgrade();
    let d = dispatcher.clone();
    manage.connect_clicked(move |_| {
        if let Some(dialog) = weak.upgrade() {
            dialog.close();
        }
        let _ = d.dispatch(AppMsg::Templates(TemplatesMsg::Manage));
    });
    manage
}

fn cancel_button(dialog: &adw::Dialog) -> gtk::Button {
    let cancel = gtk::Button::with_label(&gettext("Cancel"));
    let weak = dialog.downgrade();
    cancel.connect_clicked(move |_| {
        if let Some(dialog) = weak.upgrade() {
            dialog.close();
        }
    });
    cancel
}
