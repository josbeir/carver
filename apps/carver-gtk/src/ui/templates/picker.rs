//! Responsive native picker with explicit creation after a read-only preview.
use super::{ListHandle, properties::PropertiesHandle};
use crate::mvu::{AppDispatcher, AppMsg, RequestId, TemplatePreview, TemplatesMsg, UiError};
use carver_sdk::NoteTemplate;
use gettextrs::gettext;
use gtk::prelude::*;
use libadwaita::{self as adw, prelude::*};

pub(super) struct PreviewHandle {
    create: gtk::Button,
    view: webkit6::WebView,
    properties: PropertiesHandle,
    error: gtk::Label,
    split: adw::NavigationSplitView,
}
impl PreviewHandle {
    pub(super) fn render(&self, preview: Option<&Result<TemplatePreview, UiError>>, reveal: bool) {
        self.create.set_sensitive(matches!(preview, Some(Ok(_))));
        self.error.set_visible(matches!(preview, Some(Err(_))));
        if let Some(preview) = preview {
            self.properties.render(preview.as_ref());
            match preview {
                Ok(preview) => {
                    crate::ui::editor::load_template_preview(&self.view, &preview.source);
                }
                Err(error) => self.error.set_text(&error.message),
            }
        }
        self.properties.group.set_visible(preview.is_some());
        self.view.set_visible(matches!(preview, Some(Ok(_))));
        if reveal && preview.is_some() {
            self.split.set_show_content(true);
        }
    }
}

pub(super) fn show(
    parent: &gtk::Window,
    dispatcher: &AppDispatcher,
    request_id: RequestId,
    templates: &[NoteTemplate],
    initial_preview: Option<&Result<TemplatePreview, UiError>>,
    selected: Option<carver_sdk::TemplateId>,
) -> ListHandle {
    let dialog = adw::Dialog::builder()
        .title(gettext("Choose Template"))
        .content_width(820)
        .width_request(300)
        .height_request(300)
        .content_height(680)
        .build();
    dialog.set_widget_name("templates-dialog");
    let toolbar = adw::ToolbarView::new();
    let header = adw::HeaderBar::new();
    header.set_show_start_title_buttons(false);
    header.set_show_end_title_buttons(false);
    header.pack_start(&cancel_button(&dialog));
    let create = gtk::Button::with_label(&gettext("Create Note"));
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
    let split = adw::NavigationSplitView::new();
    split.set_widget_name("template-preview-split");
    split.set_min_sidebar_width(180.0);
    split.set_max_sidebar_width(260.0);
    let sidebar = sidebar(dispatcher, request_id, templates, selected, &split, &dialog);
    let properties = PropertiesHandle::new(true);
    let (overlay, view) = crate::ui::editor::build_template_preview(dispatcher);
    view.set_widget_name("template-content-preview");
    overlay.set_height_request(250);
    let content = gtk::Box::new(gtk::Orientation::Vertical, 12);
    super::set_margins(&content);
    content.append(&super::help_label(&gettext(
        "New notes get their own copy. Editing a template won’t change existing notes.",
    )));
    content.append(&overlay);
    content.append(&properties.group);
    let error = gtk::Label::builder()
        .xalign(0.0)
        .wrap(true)
        .visible(false)
        .build();
    error.add_css_class("error");
    error.set_widget_name("template-preview-error");
    content.append(&error);
    let scroll = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .child(&content)
        .build();
    let content_toolbar = adw::ToolbarView::new();
    let content_header = adw::HeaderBar::new();
    content_header.set_show_start_title_buttons(false);
    content_header.set_show_end_title_buttons(false);
    content_header.set_visible(false);
    content_toolbar.add_top_bar(&content_header);
    content_toolbar.set_content(Some(&scroll));
    split.set_sidebar(Some(&adw::NavigationPage::new(
        &sidebar,
        &gettext("Templates"),
    )));
    split.set_content(Some(&adw::NavigationPage::new(
        &content_toolbar,
        &gettext("Preview"),
    )));
    let breakpoint = adw::Breakpoint::new(adw::BreakpointCondition::new_length(
        adw::BreakpointConditionLengthType::MaxWidth,
        600.0,
        adw::LengthUnit::Sp,
    ));
    breakpoint.add_setter(&split, "collapsed", Some(&true.to_value()));
    breakpoint.add_setter(&content_header, "visible", Some(&true.to_value()));
    dialog.add_breakpoint(breakpoint);
    toolbar.set_content(Some(&split));
    dialog.set_child(Some(&toolbar));
    let preview = PreviewHandle {
        create,
        view,
        properties,
        error,
        split,
    };
    preview.render(initial_preview, false);
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

fn sidebar(
    dispatcher: &AppDispatcher,
    request_id: RequestId,
    templates: &[NoteTemplate],
    selected: Option<carver_sdk::TemplateId>,
    split: &adw::NavigationSplitView,
    dialog: &adw::Dialog,
) -> gtk::Box {
    let content = gtk::Box::new(gtk::Orientation::Vertical, 12);
    super::set_margins(&content);
    let search = gtk::SearchEntry::new();
    search.set_widget_name("templates-search");
    search.set_placeholder_text(Some(&gettext("Search templates…")));
    content.append(&search);
    let list = gtk::ListBox::new();
    list.set_widget_name("template-picker-list");
    list.set_selection_mode(gtk::SelectionMode::Single);
    list.add_css_class("boxed-list");
    let mut rows = Vec::new();
    for template in templates {
        let row = adw::ActionRow::builder()
            .title(&template.name)
            .activatable(true)
            .build();
        row.set_use_markup(false);
        row.set_selectable(true);
        row.set_widget_name(&format!("template-row-{}", template.id));
        list.append(&row);
        if Some(template.id) == selected {
            list.select_row(Some(&row));
        }
        rows.push((template.name.to_lowercase(), row));
    }
    let ids: Vec<_> = templates.iter().map(|t| t.id).collect();
    let d = dispatcher.clone();
    list.connect_row_selected(move |_, row| {
        let id = row
            .and_then(|row| usize::try_from(row.index()).ok())
            .and_then(|i| ids.get(i))
            .copied();
        let _ = d.dispatch(AppMsg::Templates(TemplatesMsg::SelectPreview {
            request_id,
            id,
        }));
    });
    let weak = split.downgrade();
    list.connect_row_activated(move |list, row| {
        list.select_row(Some(row));
        if let Some(split) = weak.upgrade() {
            split.set_show_content(true);
        }
    });
    let scroll = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .child(&list)
        .vexpand(true)
        .build();
    content.append(&scroll);
    let empty = super::empty_state(dialog, dispatcher, templates.is_empty());
    content.append(&empty);
    let list_for_search = list.clone();
    let templates_empty = templates.is_empty();
    search.connect_search_changed(move |entry| {
        let query = entry.text().to_lowercase();
        let mut any = false;
        for (name, row) in &rows {
            let visible = name.contains(&query);
            row.set_visible(visible);
            any |= visible;
            if !visible
                && list_for_search
                    .selected_row()
                    .is_some_and(|selected| selected == *row)
            {
                list_for_search.unselect_all();
            }
        }
        empty.set_visible(!any);
        if !templates_empty {
            empty.set_title(&gettext("No matching templates"));
            empty.set_description(Some(&gettext("Try a different search.")));
            empty.set_child(gtk::Widget::NONE);
        }
    });
    let manage = gtk::Button::with_label(&gettext("Manage Templates…"));
    let weak = dialog.downgrade();
    let d = dispatcher.clone();
    manage.connect_clicked(move |_| {
        if let Some(dialog) = weak.upgrade() {
            dialog.close();
        }
        let _ = d.dispatch(AppMsg::Templates(TemplatesMsg::Manage));
    });
    content.append(&manage);
    content
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
