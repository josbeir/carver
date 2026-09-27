//! Inline boolean cells and the anchored popover editor for Base grid cells.
//!
//! A boolean is always visible as a toggle. Every other editable value opens an anchored
//! [`gtk::Popover`] with the appropriate control, Done/Cancel (and Clear where it applies), which
//! keeps the grid read-only and does not depend on fragile focus-out handling.

use std::{cell::Cell, rc::Rc};

use carver_config::{DocumentProperty, ResolvedProperty, resolve_property};
use carver_domain::{FrontmatterValue, PropertyKind, PropertyType, is_reserved_key};
use carver_sdk::{BaseColumn, NoteId, PropertyDescriptor, Revision};
use gettextrs::gettext;
use gtk::prelude::*;

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
    /// Configured list options, rendered as a dropdown or a checklist.
    List {
        /// Available choices.
        options: Vec<String>,
        /// Whether more than one option may be selected.
        multiple: bool,
    },
    /// A list without configured options, edited as comma-separated text.
    ListText,
    /// A calendar date.
    Date,
    /// A calendar date and time.
    DateTime,
}

/// Resolves the editor for a column from observed descriptors and configured defaults.
///
/// A configured default is authoritative for its key, matching the properties dialog: its type
/// decides the editor even when the observed value is a scalar (a single-select list is stored as
/// text, so the descriptor alone would wrongly report `Text`). Only properties without a
/// configured default fall back to the observed descriptor, and then to free text. A mixed value
/// with no configured default stays read-only so the grid never flattens what it cannot represent.
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
            let descriptor = descriptors
                .iter()
                .find(|descriptor| descriptor.path == *path);
            if configured.is_none() && descriptor.is_some_and(|d| d.kind == PropertyKind::Mixed) {
                return None;
            }
            let observed = descriptor.map_or(PropertyType::Text, |d| d.property_type);
            Some(editor_for_resolved(&resolve_property(configured, observed)))
        }
    }
}

/// Maps the shared resolved property shape onto the grid's editor.
fn editor_for_resolved(resolved: &ResolvedProperty) -> CellEditor {
    match resolved.field_type {
        PropertyType::Text | PropertyType::LongText => CellEditor::Text,
        PropertyType::Number => CellEditor::Number,
        PropertyType::Boolean => CellEditor::Boolean,
        PropertyType::Date => CellEditor::Date,
        PropertyType::DateTime => CellEditor::DateTime,
        PropertyType::List if resolved.options.is_empty() => CellEditor::ListText,
        PropertyType::List => CellEditor::List {
            options: resolved.options.clone(),
            multiple: resolved.multiple,
        },
    }
}

/// The seed value an editor starts from, matching what [`CellEditorWidget::value`] returns when the
/// user saves without changing anything.
#[must_use]
pub(crate) fn seed_value(editor: &CellEditor, seed: &FrontmatterValue) -> CellValue {
    match editor {
        CellEditor::Text => text_cell_value(seed),
        CellEditor::Number => number_cell_value(seed),
        CellEditor::List { options, multiple } => list_cell_value(seed, options, *multiple),
        CellEditor::ListText => list_text_cell_value(seed),
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
            CellEditor::List { options, multiple } => Some(if *multiple {
                Self::build_multiple_list(options, seed, name)
            } else {
                Self::build_single_list(options, seed, name)
            }),
            CellEditor::ListText => {
                let entry = gtk::Entry::new();
                entry.set_widget_name(name);
                entry.set_text(&selected_options(seed).join(", "));
                entry.set_hexpand(true);
                let read = {
                    let entry = entry.clone();
                    Rc::new(move || Ok(read_list(&entry)))
                };
                Some(Self {
                    widget: entry.upcast(),
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

    fn build_single_list(options: &[String], seed: &FrontmatterValue, name: &str) -> Self {
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
            let options = options.to_vec();
            let dropdown = dropdown.clone();
            Rc::new(move || {
                let option = options.get(dropdown.selected() as usize).ok_or(())?;
                Ok(Some(serde_json::Value::String(option.clone())))
            })
        };
        Self {
            widget: dropdown.upcast(),
            read,
        }
    }

    fn build_multiple_list(options: &[String], seed: &FrontmatterValue, name: &str) -> Self {
        let selected = selected_options(seed);
        let container = gtk::Box::new(gtk::Orientation::Vertical, 6);
        container.set_widget_name(name);
        let mut checks = Vec::new();
        for option in options {
            let check = gtk::CheckButton::with_label(option);
            check.set_active(selected.contains(option));
            container.append(&check);
            checks.push((option.clone(), check));
        }
        let read = {
            let checks = checks.clone();
            Rc::new(move || {
                let values: Vec<serde_json::Value> = checks
                    .iter()
                    .filter(|(_, check)| check.is_active())
                    .map(|(option, _)| serde_json::Value::String(option.clone()))
                    .collect();
                Ok(Some(serde_json::Value::Array(values)))
            })
        };
        Self {
            widget: container.upcast(),
            read,
        }
    }

    /// Returns the control placed in the popover.
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
    switch.set_cursor_from_name(Some("pointer"));
    switch
}

/// The identity and seed of one Base cell edit.
#[derive(Clone, Copy)]
pub(crate) struct CellEdit<'a> {
    /// Note whose property is edited.
    pub(crate) note_id: NoteId,
    /// Row revision captured when the edit began.
    pub(crate) revision: Revision,
    /// JSON Pointer path of the edited property.
    pub(crate) path: &'a str,
    /// Dialog heading, usually the property key.
    pub(crate) title: &'a str,
    /// Editor resolved for the column.
    pub(crate) editor: &'a CellEditor,
    /// Value the editor starts from.
    pub(crate) seed: &'a FrontmatterValue,
}

/// The footer buttons of a cell editor popover.
struct CellActions {
    clear: gtk::Button,
    cancel: gtk::Button,
    done: gtk::Button,
}

/// Builds the always-visible single-select dropdown for a cell.
#[must_use]
pub(crate) fn build_select_cell(
    seed: &FrontmatterValue,
    options: &[String],
    name: &str,
) -> gtk::DropDown {
    let labels: Vec<&str> = options.iter().map(String::as_str).collect();
    let dropdown = gtk::DropDown::from_strings(&labels);
    dropdown.set_widget_name(name);
    dropdown.set_hexpand(true);
    dropdown.set_cursor_from_name(Some("pointer"));
    if let FrontmatterValue::Text(text) = seed
        && let Some(index) = options.iter().position(|option| option == text)
    {
        dropdown.set_selected(u32::try_from(index).unwrap_or(0));
    }
    dropdown
}

/// Returns the option list including an authored value that is not one of the configured options.
#[must_use]
pub(crate) fn options_with_current(options: &[String], current: &FrontmatterValue) -> Vec<String> {
    let mut options = options.to_vec();
    if let FrontmatterValue::Text(text) = current
        && !text.trim().is_empty()
        && !options.iter().any(|option| option == text)
    {
        options.push(text.clone());
    }
    options
}

/// Opens the anchored editor popover for one cell.
///
/// Done, Clear, and clicking away commit a changed value; Cancel and Escape dismiss it unchanged.
pub(crate) fn show_cell_editor(
    anchor: &gtk::Widget,
    dispatcher: &AppDispatcher,
    edit: CellEdit<'_>,
) {
    let CellEdit {
        note_id,
        revision,
        path,
        title,
        editor,
        seed,
    } = edit;
    let Some(field) = CellEditorWidget::build(editor, seed, "base-cell-editor") else {
        return;
    };
    let field = Rc::new(field);
    let unchanged = seed_value(editor, seed);
    // A list value is a selection, so clearing it is not offered.
    let clearable = !matches!(editor, CellEditor::List { .. });

    let content = gtk::Box::new(gtk::Orientation::Vertical, 8);
    content.set_margin_start(12);
    content.set_margin_end(12);
    content.set_margin_top(12);
    content.set_margin_bottom(12);
    content.set_width_request(280);
    let heading = gtk::Label::new(Some(title));
    heading.set_xalign(0.0);
    heading.add_css_class("heading");
    content.append(&heading);
    content.append(field.widget());

    let footer = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    let clear = gtk::Button::with_label(&gettext("Clear"));
    clear.add_css_class("flat");
    clear.set_widget_name("base-cell-editor-clear");
    clear.set_halign(gtk::Align::Start);
    clear.set_hexpand(true);
    let cancel = gtk::Button::with_label(&gettext("Cancel"));
    cancel.add_css_class("flat");
    cancel.set_widget_name("base-cell-editor-cancel");
    let done = gtk::Button::with_label(&gettext("Done"));
    done.add_css_class("suggested-action");
    done.set_widget_name("base-cell-editor-done");
    // Enter in a text or number entry submits, matching the Done button.
    if let Some(entry) = field.widget().downcast_ref::<gtk::Entry>() {
        let done = done.clone();
        entry.connect_activate(move |_| done.emit_clicked());
    }
    footer.append(&clear);
    footer.append(&cancel);
    footer.append(&done);
    content.append(&footer);

    let popover = gtk::Popover::new();
    popover.set_widget_name("base-cell-editor-popover");
    popover.set_has_arrow(true);
    popover.set_autohide(true);
    popover.set_position(gtk::PositionType::Bottom);
    popover.set_child(Some(&content));
    popover.set_parent(anchor);

    let dispatcher = dispatcher.clone();
    let path = path.to_owned();
    let commit: Rc<dyn Fn(CellValue)> = Rc::new(move |value| {
        let _ = dispatcher.dispatch(AppMsg::Bases(BasesMsg::CommitCellEdit {
            note_id,
            path: path.clone(),
            revision,
            value,
        }));
    });
    connect_cell_actions(
        &popover,
        &field,
        CellActions {
            clear,
            cancel,
            done,
        },
        clearable,
        unchanged,
        &commit,
    );
    popover.popup();
}

/// Wires Done, Clear, Cancel, click-away, and Escape into one commit policy.
fn connect_cell_actions(
    popover: &gtk::Popover,
    field: &Rc<CellEditorWidget>,
    actions: CellActions,
    clearable: bool,
    unchanged: CellValue,
    commit: &Rc<dyn Fn(CellValue)>,
) {
    let CellActions {
        clear,
        cancel,
        done,
    } = actions;
    clear.set_visible(clearable);
    let settled = Rc::new(Cell::new(false));
    {
        let settled = Rc::clone(&settled);
        let commit = Rc::clone(commit);
        let field = Rc::clone(field);
        let unchanged = unchanged.clone();
        let popover = popover.clone();
        done.connect_clicked(move |_| {
            let Ok(value) = field.value() else {
                // Invalid input keeps the popover open so the user can correct it.
                return;
            };
            settled.set(true);
            if value != unchanged {
                commit(value);
            }
            popover.popdown();
        });
    }
    {
        let settled = Rc::clone(&settled);
        let commit = Rc::clone(commit);
        let popover = popover.clone();
        clear.connect_clicked(move |_| {
            settled.set(true);
            commit(None);
            popover.popdown();
        });
    }
    {
        let settled = Rc::clone(&settled);
        let popover = popover.clone();
        cancel.connect_clicked(move |_| {
            settled.set(true);
            popover.popdown();
        });
    }
    {
        let settled = Rc::clone(&settled);
        let commit = Rc::clone(commit);
        let field = Rc::clone(field);
        popover.connect_closed(move |popover| {
            // A click-away commits a changed value; an explicit action already settled.
            if !settled.replace(true)
                && let Ok(value) = field.value()
                && value != unchanged
            {
                commit(value);
            }
            popover.unparent();
        });
    }
    // Escape cancels rather than falling through to the click-away commit.
    let key = gtk::EventControllerKey::new();
    key.set_propagation_phase(gtk::PropagationPhase::Capture);
    let popover_for_key = popover.clone();
    key.connect_key_pressed(move |_, key, _, _| {
        if key == gtk::gdk::Key::Escape {
            settled.set(true);
            popover_for_key.popdown();
            glib::Propagation::Stop
        } else {
            glib::Propagation::Proceed
        }
    });
    popover.add_controller(key);
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

/// Reads a comma-separated entry into a JSON string array, or `None` when it is empty.
fn read_list(entry: &gtk::Entry) -> CellValue {
    let values: Vec<serde_json::Value> = entry
        .text()
        .split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(|item| serde_json::Value::String(item.to_owned()))
        .collect();
    (!values.is_empty()).then_some(serde_json::Value::Array(values))
}

fn list_text_cell_value(seed: &FrontmatterValue) -> CellValue {
    let values: Vec<serde_json::Value> = selected_options(seed)
        .into_iter()
        .map(serde_json::Value::String)
        .collect();
    (!values.is_empty()).then_some(serde_json::Value::Array(values))
}

fn number_cell_value(seed: &FrontmatterValue) -> CellValue {
    match seed {
        FrontmatterValue::Number(number) => Some(serde_json::Value::Number(number.clone())),
        _ => None,
    }
}

fn selected_options(seed: &FrontmatterValue) -> Vec<String> {
    match seed {
        FrontmatterValue::List(items) => items
            .iter()
            .filter_map(|item| match item {
                FrontmatterValue::Text(text) => Some(text.clone()),
                _ => None,
            })
            .collect(),
        FrontmatterValue::Text(text) => vec![text.clone()],
        _ => Vec::new(),
    }
}

fn list_cell_value(seed: &FrontmatterValue, options: &[String], multiple: bool) -> CellValue {
    let selected: Vec<String> = selected_options(seed)
        .into_iter()
        .filter(|text| options.contains(text))
        .collect();
    if multiple {
        Some(serde_json::Value::Array(
            selected
                .into_iter()
                .map(serde_json::Value::String)
                .collect(),
        ))
    } else {
        selected.into_iter().next().map(serde_json::Value::String)
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

    fn list(options: &[&str], multiple: bool) -> CellEditor {
        CellEditor::List {
            options: options.iter().map(|option| (*option).to_owned()).collect(),
            multiple,
        }
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
            Some(list(&["draft", "done"], false))
        );
        // A list with no configured options is edited as comma-separated text.
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
            Some(CellEditor::ListText)
        );
        // A multi-select list uses a checklist.
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
            Some(list(&["draft", "done"], true))
        );
    }

    #[test]
    fn resolve_editor_should_prefer_a_configured_list_over_scalar_observation() {
        // A single-select list is stored as text, so the descriptor says `Text`; the configured
        // options must still win so the grid offers the dropdown, like the properties dialog.
        let defaults = vec![property(
            "type",
            PropertyType::List,
            serde_json::json!(["draft", "open", "new", "closed"]),
        )];
        assert_eq!(
            resolve_editor(
                &property_column("/type"),
                &[descriptor("/type", PropertyKind::Text, PropertyType::Text)],
                &defaults
            ),
            Some(list(&["draft", "open", "new", "closed"], false))
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
            seed_value(&list(&["draft", "done"], false), &text.clone()),
            None
        );
        assert_eq!(
            seed_value(
                &list(&["draft", "done"], false),
                &FrontmatterValue::Text("done".to_owned())
            ),
            Some(serde_json::json!("done"))
        );
        assert_eq!(
            seed_value(
                &list(&["draft", "done"], true),
                &FrontmatterValue::List(vec![
                    FrontmatterValue::Text("draft".to_owned()),
                    FrontmatterValue::Text("done".to_owned()),
                ])
            ),
            Some(serde_json::json!(["draft", "done"]))
        );
        assert_eq!(
            seed_value(
                &CellEditor::ListText,
                &FrontmatterValue::List(vec![FrontmatterValue::Text("a".to_owned())])
            ),
            Some(serde_json::json!(["a"]))
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
