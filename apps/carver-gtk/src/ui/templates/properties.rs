//! Native effective-property disclosure shared by template picking and editing.
use crate::mvu::{TemplatePreview, TemplatePropertyOrigin, UiError};
use gettextrs::gettext;
use gtk::prelude::*;
use libadwaita::{self as adw, prelude::*};

pub(super) struct PropertiesHandle {
    pub(super) group: adw::PreferencesGroup,
    list: gtk::ListBox,
}
impl PropertiesHandle {
    pub(super) fn new(initially_open: bool) -> Self {
        let group = adw::PreferencesGroup::new();
        let expander = adw::ExpanderRow::builder()
            .title(gettext("Properties for new notes"))
            .expanded(initially_open)
            .build();
        expander.set_widget_name("template-properties-expander");
        let list = gtk::ListBox::new();
        list.set_selection_mode(gtk::SelectionMode::None);
        list.set_widget_name("template-effective-properties");
        if initially_open {
            group.set_title(&gettext("Properties for new notes"));
            list.add_css_class("boxed-list");
            group.add(&list);
        } else {
            let scroll = gtk::ScrolledWindow::builder()
                .hscrollbar_policy(gtk::PolicyType::Never)
                .propagate_natural_height(true)
                .max_content_height(180)
                .child(&list)
                .build();
            scroll.set_widget_name("template-properties-scroll");
            expander.add_row(&scroll);
            group.add(&expander);
        }
        Self { group, list }
    }
    pub(super) fn render(&self, preview: Result<&TemplatePreview, &UiError>) {
        while let Some(child) = self.list.first_child() {
            self.list.remove(&child);
        }
        match preview {
            Ok(preview) if !preview.properties.is_empty() => {
                for property in &preview.properties {
                    let origin = match &property.origin {
                        TemplatePropertyOrigin::Template => gettext("From template"),
                        TemplatePropertyOrigin::Default => gettext("From your default properties"),
                        TemplatePropertyOrigin::Override(value) => tr_fmt!(
                            gettext("From template · replaces default: {value}"),
                            value = value,
                        ),
                    };
                    let row = adw::ActionRow::builder()
                        .title(&property.key)
                        .subtitle(&origin)
                        .build();
                    row.set_use_markup(false);
                    row.set_widget_name(&format!("template-property:{}", property.key));
                    let value = gtk::Label::builder()
                        .label(&property.value)
                        .ellipsize(gtk::pango::EllipsizeMode::End)
                        .width_chars(8)
                        .hexpand(true)
                        .xalign(1.0)
                        .max_width_chars(40)
                        .selectable(true)
                        .build();
                    value.set_widget_name(&format!("template-property-value:{}", property.key));
                    value.set_tooltip_text(Some(&property.value));
                    row.add_suffix(&value);
                    self.list.append(&row);
                }
            }
            result => {
                let text = result.map_or_else(
                    |error| error.message.clone(),
                    |_| gettext("No properties yet. Add default properties in Preferences."),
                );
                let row = adw::ActionRow::builder().title(&text).build();
                row.set_use_markup(false);
                row.set_widget_name("template-properties-status");
                row.set_title_lines(0);
                self.list.append(&row);
            }
        }
    }
}
