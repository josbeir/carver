//! Native document-properties dialog for a note's frontmatter.
//!
//! The dialog projects the editor's canonical frontmatter into native widgets. It never owns
//! document state: saving dispatches [`EditorMsg::ApplyFrontmatter`] and the MVU runtime splices
//! the result into canonical Carve source.

use std::{cell::RefCell, rc::Rc};

use carver_config::DocumentProperty;
use carver_domain::{
    FrontmatterDocument, FrontmatterField, FrontmatterFormat, FrontmatterValue, PropertyKind,
};
use gettextrs::gettext;
use gtk::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;

use crate::mvu::{AppDispatcher, AppMsg, EditorMsg, EditorPropertiesRequest, FrontmatterEdit};

/// The reserved, always-present title field key.
const TITLE_KEY: &str = "title";

/// The value kinds offered by the document-properties dialog.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PropertyKindChoice {
    Text,
    LongText,
    Number,
    Boolean,
    List,
}

impl PropertyKindChoice {
    /// Returns the choice represented by a drop-down row index.
    pub(crate) const fn from_index(index: u32) -> Self {
        match index {
            1 => Self::LongText,
            2 => Self::Number,
            3 => Self::Boolean,
            4 => Self::List,
            _ => Self::Text,
        }
    }

    /// Returns the drop-down row index for this choice.
    pub(crate) const fn index(self) -> u32 {
        match self {
            Self::Text => 0,
            Self::LongText => 1,
            Self::Number => 2,
            Self::Boolean => 3,
            Self::List => 4,
        }
    }

    /// Infers a choice from an existing frontmatter value.
    pub(crate) fn for_value(value: &FrontmatterValue) -> Self {
        match value {
            FrontmatterValue::Text(_) | FrontmatterValue::Null | FrontmatterValue::Object(_) => {
                Self::Text
            }
            FrontmatterValue::Number(_) => Self::Number,
            FrontmatterValue::Boolean(_) => Self::Boolean,
            FrontmatterValue::List(_) => Self::List,
        }
    }

    /// Infers a choice from a configured default property.
    pub(crate) fn for_property(property: &DocumentProperty) -> Self {
        match property.kind {
            PropertyKind::Number => Self::Number,
            PropertyKind::Boolean => Self::Boolean,
            PropertyKind::List => Self::List,
            PropertyKind::Text if property.multiline => Self::LongText,
            PropertyKind::Text | PropertyKind::Null | PropertyKind::Mixed => Self::Text,
        }
    }

    /// Returns the persisted property kind for this choice.
    pub(crate) const fn property_kind(self) -> PropertyKind {
        match self {
            Self::Number => PropertyKind::Number,
            Self::Boolean => PropertyKind::Boolean,
            Self::List => PropertyKind::List,
            Self::Text | Self::LongText => PropertyKind::Text,
        }
    }

    /// Returns whether text values use the multi-line editor.
    pub(crate) const fn is_multiline(self) -> bool {
        matches!(self, Self::LongText)
    }
}

/// Builds a value-kind selector with its labels translated at the call site.
pub(crate) fn property_kind_dropdown(selected: PropertyKindChoice) -> gtk::DropDown {
    let labels = [
        gettext("Text"),
        gettext("Long text"),
        gettext("Number"),
        gettext("Boolean"),
        gettext("List"),
    ];
    let labels: Vec<&str> = labels.iter().map(String::as_str).collect();
    let dropdown = gtk::DropDown::from_strings(&labels);
    dropdown.set_selected(selected.index());
    dropdown
}

/// The value widgets shared by the note and defaults editors.
#[derive(Clone)]
pub(crate) struct PropertyValueFields {
    stack: gtk::Stack,
    text: gtk::Entry,
    long: gtk::TextView,
    boolean: gtk::Switch,
}

impl PropertyValueFields {
    /// Builds the stacked value widgets.
    pub(crate) fn new() -> Self {
        let text = gtk::Entry::new();
        text.set_hexpand(true);
        let long = gtk::TextView::new();
        long.set_wrap_mode(gtk::WrapMode::WordChar);
        long.set_hexpand(true);
        let long_scroll = gtk::ScrolledWindow::new();
        long_scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
        long_scroll.set_min_content_height(72);
        long_scroll.set_child(Some(&long));
        let boolean = gtk::Switch::new();
        boolean.set_halign(gtk::Align::Start);
        let stack = gtk::Stack::new();
        stack.set_hexpand(true);
        stack.add_named(&text, Some("text"));
        stack.add_named(&long_scroll, Some("long"));
        stack.add_named(&boolean, Some("boolean"));
        Self {
            stack,
            text,
            long,
            boolean,
        }
    }

    /// Returns the stacked value widget.
    pub(crate) fn widget(&self) -> &gtk::Stack {
        &self.stack
    }

    /// Shows the editor matching the selected kind.
    pub(crate) fn set_choice(&self, choice: PropertyKindChoice) {
        self.stack.set_visible_child_name(match choice {
            PropertyKindChoice::LongText => "long",
            PropertyKindChoice::Boolean => "boolean",
            PropertyKindChoice::Text | PropertyKindChoice::Number | PropertyKindChoice::List => {
                "text"
            }
        });
    }

    /// Displays an existing frontmatter value.
    pub(crate) fn set_frontmatter(&self, choice: PropertyKindChoice, value: &FrontmatterValue) {
        match choice {
            PropertyKindChoice::LongText => self.set_long(&frontmatter_text(value)),
            PropertyKindChoice::Boolean => self
                .boolean
                .set_active(matches!(value, FrontmatterValue::Boolean(true))),
            PropertyKindChoice::List => self.text.set_text(&frontmatter_list_text(value)),
            PropertyKindChoice::Text | PropertyKindChoice::Number => {
                self.text.set_text(&frontmatter_text(value));
            }
        }
    }

    /// Reads the edited value for the selected kind.
    pub(crate) fn frontmatter(&self, choice: PropertyKindChoice) -> FrontmatterValue {
        match choice {
            PropertyKindChoice::LongText => FrontmatterValue::Text(self.long_text()),
            PropertyKindChoice::Number => parse_number(&self.text.text()).map_or_else(
                || FrontmatterValue::Text(self.text.text().to_string()),
                FrontmatterValue::Number,
            ),
            PropertyKindChoice::Boolean => FrontmatterValue::Boolean(self.boolean.is_active()),
            PropertyKindChoice::List => FrontmatterValue::List(self.list_value()),
            PropertyKindChoice::Text => FrontmatterValue::Text(self.text.text().to_string()),
        }
    }

    /// Displays a configured default property value.
    pub(crate) fn set_json(&self, choice: PropertyKindChoice, value: &serde_json::Value) {
        match choice {
            PropertyKindChoice::LongText => self.set_long(value.as_str().unwrap_or_default()),
            PropertyKindChoice::Boolean => {
                self.boolean.set_active(value.as_bool().unwrap_or(false));
            }
            PropertyKindChoice::List => self.text.set_text(&json_list_text(value)),
            PropertyKindChoice::Text | PropertyKindChoice::Number => {
                self.text.set_text(&json_scalar_text(value));
            }
        }
    }

    /// Reads the edited configured default property value.
    pub(crate) fn json(&self, choice: PropertyKindChoice) -> serde_json::Value {
        match choice {
            PropertyKindChoice::LongText => serde_json::Value::String(self.long_text()),
            PropertyKindChoice::Number => parse_number(&self.text.text())
                .map_or_else(|| serde_json::Value::from(0), serde_json::Value::Number),
            PropertyKindChoice::Boolean => serde_json::Value::Bool(self.boolean.is_active()),
            PropertyKindChoice::List => serde_json::Value::Array(
                self.list_value()
                    .into_iter()
                    .map(|value| to_json(&value))
                    .collect(),
            ),
            PropertyKindChoice::Text => serde_json::Value::String(self.text.text().to_string()),
        }
    }

    fn set_long(&self, text: &str) {
        self.long.buffer().set_text(text);
    }

    fn long_text(&self) -> String {
        let buffer = self.long.buffer();
        buffer
            .text(&buffer.start_iter(), &buffer.end_iter(), false)
            .to_string()
    }

    fn list_value(&self) -> Vec<FrontmatterValue> {
        self.text
            .text()
            .split(',')
            .map(str::trim)
            .filter(|item| !item.is_empty())
            .map(|item| FrontmatterValue::Text(item.to_owned()))
            .collect()
    }
}

/// One editable row in the note-properties form.
struct PropertyRow {
    root: gtk::Box,
    key: gtk::Entry,
    kind: gtk::DropDown,
    fields: PropertyValueFields,
}

/// A captured row value used while rebuilding the form.
#[derive(Clone)]
struct RowDraft {
    key: String,
    choice: PropertyKindChoice,
    value: FrontmatterValue,
    fixed_key: bool,
}

impl RowDraft {
    fn blank() -> Self {
        Self {
            key: String::new(),
            choice: PropertyKindChoice::Text,
            value: FrontmatterValue::Text(String::new()),
            fixed_key: false,
        }
    }

    fn from_property(property: &DocumentProperty) -> Self {
        Self {
            key: property.key.clone(),
            choice: PropertyKindChoice::for_property(property),
            value: FrontmatterValue::from_json(&property.value),
            fixed_key: false,
        }
    }
}

/// Presents the native document-properties dialog for an immutable editor snapshot.
#[expect(
    clippy::too_many_lines,
    reason = "the dialog keeps its widget graph and save closure together for modal state"
)]
pub(crate) fn show(
    parent: &gtk::Window,
    dispatcher: &AppDispatcher,
    request: &EditorPropertiesRequest,
) {
    let dialog = adw::Dialog::builder()
        .title(gettext("Document Properties"))
        .follows_content_size(true)
        .content_width(560)
        .build();
    dialog.set_widget_name("document-properties-dialog");
    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&adw::HeaderBar::new());
    let content = gtk::Box::new(gtk::Orientation::Vertical, 12);
    content.set_margin_start(18);
    content.set_margin_end(18);
    content.set_margin_top(12);
    content.set_margin_bottom(18);

    let initial_format = request
        .document
        .as_ref()
        .map_or(FrontmatterFormat::Yaml, |document| document.format);
    let format = format_dropdown(initial_format);
    let format_row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    let format_label = gtk::Label::new(Some(&gettext("Format")));
    format_label.set_xalign(0.0);
    format_label.set_hexpand(true);
    format_row.append(&format_label);
    format_row.append(&format);
    content.append(&format_row);

    let raw_mode = request.document.as_ref().is_some_and(|document| {
        document.error.is_some()
            || document
                .fields
                .iter()
                .any(|field| matches!(field.value, FrontmatterValue::Object(_)))
    });

    let raw_view = raw_mode.then(|| {
        let view = gtk::TextView::new();
        view.set_widget_name("document-properties-raw");
        view.set_wrap_mode(gtk::WrapMode::WordChar);
        view.set_monospace(true);
        view.buffer()
            .set_text(request.raw.as_deref().unwrap_or_default());
        let scroll = gtk::ScrolledWindow::new();
        scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
        scroll.set_min_content_height(220);
        scroll.set_child(Some(&view));
        content.append(&scroll);
        view
    });

    let rows: Rc<RefCell<Vec<PropertyRow>>> = Rc::new(RefCell::new(Vec::new()));
    let fields_container = gtk::Box::new(gtk::Orientation::Vertical, 8);
    fields_container.set_widget_name("document-properties-fields");
    if !raw_mode {
        let scroll = gtk::ScrolledWindow::new();
        scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
        scroll.set_propagate_natural_height(true);
        scroll.set_propagate_natural_width(true);
        scroll.set_min_content_width(520);
        scroll.set_max_content_height(420);
        scroll.set_child(Some(&fields_container));
        content.append(&scroll);
        rebuild_rows(&fields_container, &rows, &initial_drafts(request));
        let actions = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let add_property = gtk::Button::with_label(&gettext("Add property"));
        add_property.set_widget_name("document-properties-add");
        connect_add_property(&add_property, &rows, &fields_container);
        actions.append(&add_property);
        if request.defaults_enabled && !request.defaults.is_empty() {
            let add_defaults = gtk::Button::with_label(&gettext("Add default properties"));
            add_defaults.set_widget_name("document-properties-add-defaults");
            connect_add_defaults(&add_defaults, &rows, &fields_container, &request.defaults);
            actions.append(&add_defaults);
        }
        content.append(&actions);
    }

    let cancel = gtk::Button::with_label(&gettext("Cancel"));
    let save = gtk::Button::with_label(&gettext("Save"));
    save.add_css_class("suggested-action");
    save.set_widget_name("document-properties-save");
    let footer = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    footer.set_halign(gtk::Align::End);
    footer.set_margin_top(6);
    footer.append(&cancel);
    footer.append(&save);
    content.append(&footer);
    toolbar.set_content(Some(&content));
    dialog.set_child(Some(&toolbar));

    let dialog_for_cancel = dialog.clone();
    cancel.connect_clicked(move |_| {
        let _ = dialog_for_cancel.close();
    });

    let session = request.session;
    let revision = request.revision;
    let dispatcher = dispatcher.clone();
    let rows_for_save = Rc::clone(&rows);
    let dialog_for_save = dialog.clone();
    let format_for_save = format.clone();
    save.connect_clicked(move |_| {
        let edit = raw_view.as_ref().map_or_else(
            || {
                FrontmatterEdit::Parsed(build_document(
                    selected_format(&format_for_save),
                    &capture_drafts(&rows_for_save),
                ))
            },
            |view| {
                let buffer = view.buffer();
                FrontmatterEdit::Raw {
                    format: selected_format(&format_for_save),
                    content: buffer
                        .text(&buffer.start_iter(), &buffer.end_iter(), false)
                        .to_string(),
                }
            },
        );
        dialog_for_save.close();
        let _ = dispatcher.dispatch(AppMsg::Editor(EditorMsg::ApplyFrontmatter {
            session,
            revision,
            edit,
        }));
    });

    format.grab_focus();
    dialog.present(Some(parent));
}

fn initial_drafts(request: &EditorPropertiesRequest) -> Vec<RowDraft> {
    let mut title: Option<FrontmatterValue> = None;
    let mut note_drafts = Vec::new();
    if let Some(document) = &request.document {
        for field in &document.fields {
            if field.key == TITLE_KEY && title.is_none() {
                title = Some(field.value.clone());
                continue;
            }
            note_drafts.push(RowDraft {
                key: field.key.clone(),
                choice: PropertyKindChoice::for_value(&field.value),
                value: field.value.clone(),
                fixed_key: false,
            });
        }
    }
    let mut catalog = vec![RowDraft {
        key: TITLE_KEY.to_owned(),
        choice: PropertyKindChoice::Text,
        value: title.unwrap_or(FrontmatterValue::Text(String::new())),
        fixed_key: true,
    }];
    catalog.extend(note_drafts);
    if request.defaults_enabled {
        for property in &request.defaults {
            if property.key == TITLE_KEY || catalog.iter().any(|draft| draft.key == property.key) {
                continue;
            }
            catalog.push(RowDraft::from_property(property));
        }
    }
    catalog
}

fn build_document(format: FrontmatterFormat, drafts: &[RowDraft]) -> FrontmatterDocument {
    let fields = drafts
        .iter()
        .filter_map(|draft| {
            let key = draft.key.trim();
            if key.is_empty() {
                return None;
            }
            // An empty text value writes no key, so clearing the reserved title row (or an
            // unfilled default) leaves the source untouched instead of persisting `key: ""`.
            if matches!(&draft.value, FrontmatterValue::Text(text) if text.trim().is_empty())
                || draft.value == FrontmatterValue::Null
            {
                return None;
            }
            Some(FrontmatterField::new(key, draft.value.clone()))
        })
        .collect();
    FrontmatterDocument {
        format,
        fields,
        error: None,
    }
}

fn rebuild_rows(container: &gtk::Box, rows: &Rc<RefCell<Vec<PropertyRow>>>, drafts: &[RowDraft]) {
    while let Some(child) = container.first_child() {
        container.remove(&child);
    }
    rows.borrow_mut().clear();
    for (index, draft) in drafts.iter().enumerate() {
        let row = build_row(draft, index, container, rows);
        container.append(&row.root);
        rows.borrow_mut().push(row);
    }
}

fn build_row(
    draft: &RowDraft,
    index: usize,
    container: &gtk::Box,
    rows: &Rc<RefCell<Vec<PropertyRow>>>,
) -> PropertyRow {
    let root = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    root.set_widget_name(&format!("document-property-row-{index}"));
    let key = gtk::Entry::new();
    key.set_text(&draft.key);
    key.set_placeholder_text(Some(&gettext("Property name")));
    key.set_width_chars(14);
    key.set_editable(!draft.fixed_key);
    root.append(&key);

    let kind = property_kind_dropdown(draft.choice);
    kind.set_valign(gtk::Align::Center);
    let fields = PropertyValueFields::new();
    fields.set_choice(draft.choice);
    fields.set_frontmatter(draft.choice, &draft.value);
    fields.widget().set_hexpand(true);
    connect_kind_change(&kind, &fields, draft.choice);
    root.append(&kind);
    root.append(fields.widget());

    if !draft.fixed_key {
        let remove = gtk::Button::from_icon_name("list-remove-symbolic");
        remove.set_widget_name(&format!("document-property-remove-{index}"));
        remove.add_css_class("flat");
        remove.set_tooltip_text(Some(&gettext("Remove property")));
        remove.update_property(&[gtk::accessible::Property::Label(&gettext(
            "Remove property",
        ))]);
        connect_remove_row(&remove, index, container, rows);
        root.append(&remove);
    }
    PropertyRow {
        root,
        key,
        kind,
        fields,
    }
}

pub(crate) fn connect_kind_change(
    kind: &gtk::DropDown,
    fields: &PropertyValueFields,
    initial: PropertyKindChoice,
) {
    let fields = fields.clone();
    let previous = Rc::new(std::cell::Cell::new(initial));
    kind.connect_selected_notify(move |dropdown| {
        let next = PropertyKindChoice::from_index(dropdown.selected());
        if next == previous.get() {
            return;
        }
        let value = fields.frontmatter(previous.get());
        fields.set_choice(next);
        fields.set_frontmatter(next, &value);
        previous.set(next);
    });
}

fn connect_remove_row(
    remove: &gtk::Button,
    index: usize,
    container: &gtk::Box,
    rows: &Rc<RefCell<Vec<PropertyRow>>>,
) {
    let container = container.clone();
    let rows = Rc::clone(rows);
    remove.connect_clicked(move |_| {
        let mut drafts = capture_drafts(&rows);
        if index < drafts.len() {
            drafts.remove(index);
        }
        rebuild_rows(&container, &rows, &drafts);
    });
}

fn connect_add_property(
    button: &gtk::Button,
    rows: &Rc<RefCell<Vec<PropertyRow>>>,
    container: &gtk::Box,
) {
    let container = container.clone();
    let rows = Rc::clone(rows);
    button.connect_clicked(move |_| {
        let mut drafts = capture_drafts(&rows);
        drafts.push(RowDraft::blank());
        rebuild_rows(&container, &rows, &drafts);
    });
}

fn connect_add_defaults(
    button: &gtk::Button,
    rows: &Rc<RefCell<Vec<PropertyRow>>>,
    container: &gtk::Box,
    defaults: &[DocumentProperty],
) {
    let container = container.clone();
    let rows = Rc::clone(rows);
    let defaults = defaults.to_vec();
    button.connect_clicked(move |_| {
        let mut drafts = capture_drafts(&rows);
        for property in &defaults {
            if property.key == TITLE_KEY || drafts.iter().any(|draft| draft.key == property.key) {
                continue;
            }
            drafts.push(RowDraft::from_property(property));
        }
        rebuild_rows(&container, &rows, &drafts);
    });
}

fn capture_drafts(rows: &Rc<RefCell<Vec<PropertyRow>>>) -> Vec<RowDraft> {
    rows.borrow()
        .iter()
        .map(|row| RowDraft {
            key: row.key.text().to_string(),
            choice: PropertyKindChoice::from_index(row.kind.selected()),
            value: row
                .fields
                .frontmatter(PropertyKindChoice::from_index(row.kind.selected())),
            fixed_key: !row.key.is_editable(),
        })
        .collect()
}

fn selected_format(dropdown: &gtk::DropDown) -> FrontmatterFormat {
    match dropdown.selected() {
        1 => FrontmatterFormat::Json,
        2 => FrontmatterFormat::Toml,
        _ => FrontmatterFormat::Yaml,
    }
}

fn format_dropdown(format: FrontmatterFormat) -> gtk::DropDown {
    let labels = [gettext("YAML"), gettext("JSON"), gettext("TOML")];
    let labels: Vec<&str> = labels.iter().map(String::as_str).collect();
    let dropdown = gtk::DropDown::from_strings(&labels);
    dropdown.set_selected(match format {
        FrontmatterFormat::Yaml => 0,
        FrontmatterFormat::Json => 1,
        FrontmatterFormat::Toml => 2,
    });
    dropdown
}

fn frontmatter_text(value: &FrontmatterValue) -> String {
    match value {
        FrontmatterValue::Text(text) => text.clone(),
        FrontmatterValue::Number(number) => number.to_string(),
        FrontmatterValue::Boolean(flag) => flag.to_string(),
        FrontmatterValue::Null => String::new(),
        FrontmatterValue::Object(_) | FrontmatterValue::List(_) => to_json(value).to_string(),
    }
}

fn frontmatter_list_text(value: &FrontmatterValue) -> String {
    match value {
        FrontmatterValue::List(items) => items
            .iter()
            .map(frontmatter_text)
            .collect::<Vec<_>>()
            .join(", "),
        other => frontmatter_text(other),
    }
}

fn json_scalar_text(value: &serde_json::Value) -> String {
    value
        .as_str()
        .map_or_else(|| value.to_string(), ToOwned::to_owned)
}

fn json_list_text(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Array(items) => items
            .iter()
            .map(json_scalar_text)
            .collect::<Vec<_>>()
            .join(", "),
        other => json_scalar_text(other),
    }
}

fn parse_number(text: &str) -> Option<serde_json::Number> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Ok(value) = trimmed.parse::<i64>() {
        return Some(serde_json::Number::from(value));
    }
    if let Ok(value) = trimmed.parse::<u64>() {
        return Some(serde_json::Number::from(value));
    }
    trimmed
        .parse::<f64>()
        .ok()
        .and_then(serde_json::Number::from_f64)
}

fn to_json(value: &FrontmatterValue) -> serde_json::Value {
    value.to_json()
}
