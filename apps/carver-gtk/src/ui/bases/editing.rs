//! Inline boolean cells and the modal value editor for Base grid cells.
//!
//! A boolean is always visible as a toggle. Every other editable value opens a small dialog with
//! the appropriate control and Save/Cancel actions, which keeps the grid read-only and matches the
//! platform's editing pattern.

use std::rc::Rc;

use carver_config::DocumentProperty;
use carver_domain::{FrontmatterValue, PropertyKind, PropertyType, is_reserved_key};
use carver_sdk::{BaseColumn, NoteId, PropertyDescriptor, Revision};
use gettextrs::gettext;
use gtk::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;

use crate::mvu::{AppDispatcher, AppMsg, BasesMsg};
use crate::ui::property::{
    DatePicker, frontmatter_text, parse_number, picker_value, property_path,
};

/// The value a committed cell writes: `None` clears the property.
pub(crate) type CellValue = Option<serde_json::Value>;

/// The result of reading an editor: `Err` means the input is not a valid value.
pub(crate) type ReadResult = Result<CellValue, ()>;

/// The editor a Base column offers, or `None` for a read-only column.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum CellEditor {
    /// Single-line text, also used for the reserved title.
    Text,
    /// A numeric value edited as text.
    Number,
    /// A boolean toggle shown directly in the cell.
    Boolean,
    /// A single-select list of configured options.
    List(Vec<String>),
    /// A calendar date.
    Date,
    /// A calendar date and time.
    DateTime,
}

/// Resolves the editor for a column from observed descriptors and configured defaults.
///
/// An observed descriptor wins over a configured default for the same path. A property with no
/// metadata is treated as free text; a mixed or option-less list is read-only so the grid never
/// flattens a value it cannot represent.
#[must_use]
pub(crate) fn resolve_editor(
    column: &BaseColumn,
    descriptors: &[PropertyDescriptor],
    defaults: &[DocumentProperty],
) -> Option<CellEditor> {
    match column {
        BaseColumn::Name => Some(CellEditor::Text),
        BaseColumn::Category | BaseColumn::Updated => None,
        BaseColumn::Property(path) => {
            if path.0 == "/title" {
                return Some(CellEditor::Text);
            }
            let configured = defaults.iter().find(|property| {
                !is_reserved_key(&property.key) && property_path(&property.key) == *path
            });
            if let Some(descriptor) = descriptors
                .iter()
                .find(|descriptor| descriptor.path == *path)
            {
                if descriptor.kind == PropertyKind::Mixed {
                    return None;
                }
                return editor_for_type(descriptor.property_type, configured);
            }
            if let Some(property) = configured {
                return editor_for_type(property.field_type, Some(property));
            }
            Some(CellEditor::Text)
        }
    }
}

fn editor_for_type(
    field_type: PropertyType,
    configured: Option<&DocumentProperty>,
) -> Option<CellEditor> {
    match field_type {
        PropertyType::Text | PropertyType::LongText => Some(CellEditor::Text),
        PropertyType::Number => Some(CellEditor::Number),
        PropertyType::Boolean => Some(CellEditor::Boolean),
        PropertyType::Date => Some(CellEditor::Date),
        PropertyType::DateTime => Some(CellEditor::DateTime),
        PropertyType::List => {
            let options = configured
                .map(DocumentProperty::options)
                .unwrap_or_default();
            if options.is_empty() || configured.is_some_and(|property| property.multiple) {
                None
            } else {
                Some(CellEditor::List(options))
            }
        }
    }
}

/// The seed value an editor starts from, matching what [`CellEditorWidget::value`] returns when the
/// user saves without changing anything.
#[must_use]
pub(crate) fn seed_value(editor: &CellEditor, seed: &FrontmatterValue) -> CellValue {
    match editor {
        CellEditor::Text => text_cell_value(seed),
        CellEditor::Number => number_cell_value(seed),
        CellEditor::List(options) => match seed {
            FrontmatterValue::Text(text) if options.contains(text) => {
                Some(serde_json::Value::String(text.clone()))
            }
            _ => None,
        },
        CellEditor::Date => picker_value(PropertyType::Date, seed).map(serde_json::Value::String),
        CellEditor::DateTime => {
            picker_value(PropertyType::DateTime, seed).map(serde_json::Value::String)
        }
        CellEditor::Boolean => None,
    }
}

/// A seeded editor widget and the value it currently holds.
pub(crate) struct CellEditorWidget {
    widget: gtk::Widget,
    read: Rc<dyn Fn() -> ReadResult>,
}

impl CellEditorWidget {
    /// Builds the control for `editor`, seeded from the row's value.
    ///
    /// Returns `None` for the always-visible boolean toggle, which is built separately.
    #[must_use]
    pub(crate) fn build(editor: &CellEditor, seed: &FrontmatterValue, name: &str) -> Option<Self> {
        match editor {
            CellEditor::Boolean => None,
            CellEditor::Text => {
                let entry = gtk::Entry::new();
                entry.set_widget_name(name);
                entry.set_text(&frontmatter_text(seed));
                entry.set_hexpand(true);
                let read = {
                    let entry = entry.clone();
                    Rc::new(move || Ok(read_text(&entry)))
                };
                Some(Self {
                    widget: entry.upcast(),
                    read,
                })
            }
            CellEditor::Number => {
                let entry = gtk::Entry::new();
                entry.set_widget_name(name);
                entry.set_text(&frontmatter_text(seed));
                entry.set_hexpand(true);
                let read = {
                    let entry = entry.clone();
                    Rc::new(move || read_number(&entry))
                };
                Some(Self {
                    widget: entry.upcast(),
                    read,
                })
            }
            CellEditor::List(options) => {
                let labels: Vec<&str> = options.iter().map(String::as_str).collect();
                let dropdown = gtk::DropDown::from_strings(&labels);
                dropdown.set_widget_name(name);
                dropdown.set_hexpand(true);
                if let FrontmatterValue::Text(text) = seed
                    && let Some(index) = options.iter().position(|option| option == text)
                {
                    dropdown.set_selected(u32::try_from(index).unwrap_or(0));
                }
                let read = {
                    let options = options.clone();
                    let dropdown = dropdown.clone();
                    Rc::new(move || {
                        let option = options.get(dropdown.selected() as usize).ok_or(())?;
                        Ok(Some(serde_json::Value::String(option.clone())))
                    })
                };
                Some(Self {
                    widget: dropdown.upcast(),
                    read,
                })
            }
            CellEditor::Date | CellEditor::DateTime => {
                let field_type = if matches!(editor, CellEditor::Date) {
                    PropertyType::Date
                } else {
                    PropertyType::DateTime
                };
                let picker = DatePicker::new(field_type, seed, name);
                let read = {
                    let picker = picker.clone();
                    Rc::new(move || Ok(picker.value().map(serde_json::Value::String)))
                };
                Some(Self {
                    widget: picker.button().clone().upcast(),
                    read,
                })
            }
        }
    }

    /// Returns the control placed in the dialog.
    pub(crate) fn widget(&self) -> &gtk::Widget {
        &self.widget
    }

    /// Reads the current value, or `Err` when the input is invalid.
    pub(crate) fn value(&self) -> ReadResult {
        (self.read)()
    }
}

/// Builds the always-visible boolean toggle for a cell.
#[must_use]
pub(crate) fn build_boolean_cell(seed: bool, name: &str) -> gtk::Switch {
    let switch = gtk::Switch::new();
    switch.set_widget_name(name);
    switch.set_active(seed);
    switch.set_halign(gtk::Align::Center);
    switch.set_valign(gtk::Align::Center);
    switch.set_hexpand(true);
    switch
}

/// Opens the modal value editor for one cell and saves on confirmation.
#[expect(
    clippy::too_many_arguments,
    reason = "the dialog carries the cell identity and its resolved editor"
)]
pub(crate) fn show_cell_editor(
    parent: &gtk::Window,
    dispatcher: &AppDispatcher,
    note_id: NoteId,
    revision: Revision,
    path: &str,
    title: &str,
    editor: &CellEditor,
    seed: &FrontmatterValue,
) {
    let Some(field) = CellEditorWidget::build(editor, seed, "base-cell-editor") else {
        return;
    };
    let dialog = adw::Dialog::builder()
        .title(title)
        .follows_content_size(true)
        .content_width(420)
        .build();
    dialog.set_widget_name("base-cell-editor-dialog");

    let header = adw::HeaderBar::new();
    let cancel = gtk::Button::with_label(&gettext("Cancel"));
    cancel.set_widget_name("base-cell-editor-cancel");
    let save = gtk::Button::with_label(&gettext("Save"));
    save.add_css_class("suggested-action");
    save.set_widget_name("base-cell-editor-save");
    header.pack_start(&cancel);
    header.pack_end(&save);

    let group = adw::PreferencesGroup::new();
    let row = adw::ActionRow::builder()
        .title(title)
        .child(field.widget())
        .build();
    group.add(&row);
    let page = adw::PreferencesPage::new();
    page.add(&group);
    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&header);
    toolbar.set_content(Some(&page));
    dialog.set_child(Some(&toolbar));

    {
        let dialog = dialog.clone();
        cancel.connect_clicked(move |_| {
            let _ = dialog.close();
        });
    }
    {
        let dialog = dialog.clone();
        let dispatcher = dispatcher.clone();
        let path = path.to_owned();
        let unchanged = seed_value(editor, seed);
        save.connect_clicked(move |_| {
            let Ok(value) = field.value() else {
                // Invalid input keeps the dialog open so the user can correct it.
                return;
            };
            let _ = dialog.close();
            if value == unchanged {
                return;
            }
            let _ = dispatcher.dispatch(AppMsg::Bases(BasesMsg::CommitCellEdit {
                note_id,
                path: path.clone(),
                revision,
                value,
            }));
        });
    }
    dialog.present(Some(parent));
}

fn read_text(entry: &gtk::Entry) -> CellValue {
    let text = entry.text().to_string();
    (!text.trim().is_empty()).then_some(serde_json::Value::String(text))
}

fn read_number(entry: &gtk::Entry) -> ReadResult {
    parse_number(&entry.text())
        .map(|number| Some(serde_json::Value::Number(number)))
        .ok_or(())
}

fn text_cell_value(seed: &FrontmatterValue) -> CellValue {
    match seed {
        FrontmatterValue::Text(text) if !text.trim().is_empty() => {
            Some(serde_json::Value::String(text.clone()))
        }
        _ => None,
    }
}

fn number_cell_value(seed: &FrontmatterValue) -> CellValue {
    match seed {
        FrontmatterValue::Number(number) => Some(serde_json::Value::Number(number.clone())),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use carver_sdk::{PropertyPath, PropertyType};

    fn descriptor(
        path: &str,
        kind: PropertyKind,
        property_type: PropertyType,
    ) -> PropertyDescriptor {
        PropertyDescriptor {
            path: PropertyPath(path.to_owned()),
            kind,
            property_type,
            example: None,
        }
    }

    fn property(key: &str, field_type: PropertyType, value: serde_json::Value) -> DocumentProperty {
        DocumentProperty {
            key: key.to_owned(),
            field_type,
            multiple: false,
            value,
        }
    }

    fn property_column(path: &str) -> BaseColumn {
        BaseColumn::Property(PropertyPath(path.to_owned()))
    }

    #[test]
    fn resolve_editor_should_map_builtins_and_typed_properties() {
        assert_eq!(
            resolve_editor(&BaseColumn::Name, &[], &[]),
            Some(CellEditor::Text)
        );
        assert_eq!(resolve_editor(&BaseColumn::Category, &[], &[]), None);
        assert_eq!(resolve_editor(&BaseColumn::Updated, &[], &[]), None);
        assert_eq!(
            resolve_editor(&property_column("/title"), &[], &[]),
            Some(CellEditor::Text)
        );
        assert_eq!(
            resolve_editor(
                &property_column("/done"),
                &[descriptor(
                    "/done",
                    PropertyKind::Boolean,
                    PropertyType::Boolean
                )],
                &[]
            ),
            Some(CellEditor::Boolean)
        );
        assert_eq!(
            resolve_editor(
                &property_column("/when"),
                &[descriptor("/when", PropertyKind::Text, PropertyType::Date)],
                &[]
            ),
            Some(CellEditor::Date)
        );
        assert_eq!(
            resolve_editor(&property_column("/unknown"), &[], &[]),
            Some(CellEditor::Text)
        );
    }

    #[test]
    fn resolve_editor_should_use_configured_options_for_a_list() {
        let defaults = vec![property(
            "status",
            PropertyType::List,
            serde_json::json!(["draft", "done"]),
        )];
        assert_eq!(
            resolve_editor(&property_column("/status"), &[], &defaults),
            Some(CellEditor::List(vec![
                "draft".to_owned(),
                "done".to_owned()
            ]))
        );
        // A list without options, or a multi-select list, is read-only in the grid.
        assert_eq!(
            resolve_editor(
                &property_column("/status"),
                &[descriptor(
                    "/status",
                    PropertyKind::List,
                    PropertyType::List
                )],
                &[]
            ),
            None
        );
        let multiple = vec![DocumentProperty {
            multiple: true,
            ..property(
                "status",
                PropertyType::List,
                serde_json::json!(["draft", "done"]),
            )
        }];
        assert_eq!(
            resolve_editor(&property_column("/status"), &[], &multiple),
            None
        );
    }

    #[test]
    fn resolve_editor_should_treat_mixed_values_as_read_only() {
        assert_eq!(
            resolve_editor(
                &property_column("/mixed"),
                &[descriptor(
                    "/mixed",
                    PropertyKind::Mixed,
                    PropertyType::Text
                )],
                &[]
            ),
            None
        );
    }

    #[test]
    fn seed_value_should_match_an_unchanged_editor_read() {
        let text = FrontmatterValue::Text("ready".to_owned());
        assert_eq!(
            seed_value(&CellEditor::Text, &text),
            Some(serde_json::json!("ready"))
        );
        assert_eq!(seed_value(&CellEditor::Text, &FrontmatterValue::Null), None);
        assert_eq!(
            seed_value(
                &CellEditor::List(vec!["draft".to_owned(), "done".to_owned()]),
                &FrontmatterValue::Text("done".to_owned())
            ),
            Some(serde_json::json!("done"))
        );
        assert_eq!(
            seed_value(
                &CellEditor::DateTime,
                &FrontmatterValue::Text("2026-09-27T10:30:00Z".to_owned())
            ),
            Some(serde_json::json!("2026-09-27T10:30:00Z"))
        );
    }
}
