//! Native document-properties dialogs.
//!
//! Both the per-note editor and the default-properties editor are built from libadwaita
//! preference rows: each property is an [`adw::ExpanderRow`] whose collapsed state shows the
//! key and a value preview and whose expanded state reveals the name, type, and value rows.
//! The dialogs never own document state: saving dispatches [`EditorMsg::ApplyFrontmatter`] and
//! the MVU runtime splices the result into canonical Carve source.

use std::{cell::RefCell, collections::BTreeSet, rc::Rc};

use carver_config::{
    DocumentProperty, DocumentPropertyType, ResolvedProperty, configured_property, resolve_property,
};
use carver_domain::{
    FrontmatterDocument, FrontmatterField, FrontmatterFormat, FrontmatterValue, is_reserved_key,
};
use gettextrs::gettext;
use gtk::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;
use sourceview5::prelude::*;

use crate::mvu::{
    AppDispatcher, AppMsg, BasesMsg, EditorMsg, EditorPropertiesRequest, FrontmatterEdit,
    PreferencesMsg, PropertiesSave,
};
use crate::ui::property::{frontmatter_text, parse_number};

/// The reserved, always-present title field key.
const TITLE_KEY: &str = "title";

/// The field types, in drop-down order.
const FIELD_TYPES: [DocumentPropertyType; 7] = [
    DocumentPropertyType::Text,
    DocumentPropertyType::LongText,
    DocumentPropertyType::Number,
    DocumentPropertyType::Boolean,
    DocumentPropertyType::List,
    DocumentPropertyType::Date,
    DocumentPropertyType::DateTime,
];

/// Returns the field type represented by a drop-down row index.
fn type_from_index(index: u32) -> DocumentPropertyType {
    FIELD_TYPES
        .get(usize::try_from(index).unwrap_or(usize::MAX))
        .copied()
        .unwrap_or(DocumentPropertyType::Text)
}

/// Returns the drop-down row index for a field type.
fn type_index(field_type: DocumentPropertyType) -> u32 {
    FIELD_TYPES
        .iter()
        .position(|candidate| *candidate == field_type)
        .and_then(|index| u32::try_from(index).ok())
        .unwrap_or(0)
}

/// Returns whether a value can be edited through the structured fields UI.
///
/// Scalars and lists of plain, comma-free text are editable; nested objects and lists holding
/// typed, nested, or comma-bearing elements are not, so they fall back to the raw editor rather
/// than being flattened into text.
fn is_editable_value(value: &FrontmatterValue) -> bool {
    match value {
        FrontmatterValue::Object(_) => false,
        FrontmatterValue::List(items) => items
            .iter()
            .all(|item| matches!(item, FrontmatterValue::Text(text) if !text.contains(','))),
        FrontmatterValue::Text(_)
        | FrontmatterValue::Number(_)
        | FrontmatterValue::Boolean(_)
        | FrontmatterValue::Null => true,
    }
}

/// Escapes text for the Pango markup that libadwaita rows parse in their titles and subtitles.
fn markup_escape(text: &str) -> String {
    glib::markup_escape_text(text).to_string()
}

fn type_label(field_type: DocumentPropertyType) -> String {
    crate::ui::property::property_type_label(field_type)
}

fn type_model() -> gtk::StringList {
    let labels: Vec<String> = FIELD_TYPES.iter().map(|value| type_label(*value)).collect();
    let labels: Vec<&str> = labels.iter().map(String::as_str).collect();
    gtk::StringList::new(&labels)
}

/// The value editor for one property row.
#[derive(Clone)]
enum ValueWidget {
    Entry(adw::EntryRow),
    Long {
        row: adw::ActionRow,
        view: gtk::TextView,
    },
    Boolean(adw::SwitchRow),
    DateTime(DateTimePicker),
    Hint(gtk::Label),
}

/// A calendar (and optional time) picker row delegating to the shared date picker.
#[derive(Clone)]
struct DateTimePicker {
    row: adw::ActionRow,
    picker: crate::ui::property::DatePicker,
}

impl DateTimePicker {
    fn new(
        field_type: DocumentPropertyType,
        value: &FrontmatterValue,
        name: &str,
        title: &str,
    ) -> Self {
        let row = adw::ActionRow::new();
        row.set_title(title);
        row.set_widget_name(name);
        let picker = crate::ui::property::DatePicker::new(field_type, value, name);
        row.add_suffix(picker.button());
        let date_only = picker.is_date_only();
        let row_for_subtitle = row.clone();
        let picker_for_subtitle = picker.clone();
        picker.connect_changed(move || {
            let subtitle = picker_for_subtitle.value().map_or_else(
                || gettext("Not set"),
                |iso| crate::ui::property::display_date(&iso, date_only),
            );
            row_for_subtitle.set_subtitle(&subtitle);
        });
        picker.set_value(picker.value().as_deref());

        Self { row, picker }
    }

    fn frontmatter(&self) -> FrontmatterValue {
        self.picker.frontmatter()
    }

    fn frontmatter_preserving(&self, previous: &FrontmatterValue) -> FrontmatterValue {
        let current = self.picker.value();
        if current.as_deref() == Some(frontmatter_text(previous).as_str()) {
            previous.clone()
        } else {
            self.picker.frontmatter()
        }
    }
}

impl ValueWidget {
    fn build(
        choice: DocumentPropertyType,
        value: &FrontmatterValue,
        name: &str,
        title: &str,
    ) -> Self {
        let title = markup_escape(title);
        match choice {
            DocumentPropertyType::Boolean => {
                let row = adw::SwitchRow::new();
                row.set_title(&title);
                row.set_widget_name(name);
                row.set_active(matches!(value, FrontmatterValue::Boolean(true)));
                Self::Boolean(row)
            }
            DocumentPropertyType::LongText => {
                let view = gtk::TextView::new();
                view.set_widget_name(name);
                view.set_wrap_mode(gtk::WrapMode::WordChar);
                view.buffer().set_text(&frontmatter_text(value));
                let scroll = gtk::ScrolledWindow::new();
                scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
                scroll.set_min_content_height(120);
                scroll.add_css_class("card");
                scroll.set_child(Some(&view));
                let row = adw::ActionRow::builder()
                    .title(title.as_str())
                    .child(&scroll)
                    .build();
                Self::Long { row, view }
            }
            DocumentPropertyType::Date | DocumentPropertyType::DateTime => {
                Self::DateTime(DateTimePicker::new(choice, value, name, &title))
            }
            _ => {
                let row = adw::EntryRow::new();
                row.set_title(&title);
                row.set_widget_name(name);
                if choice == DocumentPropertyType::Number {
                    row.set_input_purpose(gtk::InputPurpose::Number);
                }
                let text = if choice == DocumentPropertyType::List {
                    frontmatter_list_text(value)
                } else {
                    frontmatter_text(value)
                };
                row.set_text(&text);
                Self::Entry(row)
            }
        }
    }

    /// Builds a non-editable hint row (date or date-time defaults in Settings).
    fn hint(text: &str, name: &str) -> Self {
        let label = gtk::Label::new(Some(text));
        label.set_wrap(true);
        label.set_xalign(0.0);
        label.add_css_class("dim-label");
        label.set_margin_start(12);
        label.set_margin_end(12);
        label.set_margin_top(4);
        label.set_widget_name(name);
        Self::Hint(label)
    }

    fn widget(&self) -> gtk::Widget {
        match self {
            Self::Entry(row) => row.clone().upcast(),
            Self::Long { row, .. } => row.clone().upcast(),
            Self::Boolean(row) => row.clone().upcast(),
            Self::DateTime(picker) => picker.row.clone().upcast(),
            Self::Hint(label) => label.clone().upcast(),
        }
    }

    fn add_suffix(&self, widget: &impl IsA<gtk::Widget>) {
        match self {
            Self::Entry(row) => row.add_suffix(widget),
            Self::Long { row, .. } => row.add_suffix(widget),
            Self::Boolean(row) => row.add_suffix(widget),
            Self::DateTime(picker) => picker.row.add_suffix(widget),
            Self::Hint(_) => {}
        }
    }

    fn frontmatter(&self, choice: DocumentPropertyType) -> FrontmatterValue {
        match self {
            Self::Boolean(row) => FrontmatterValue::Boolean(row.is_active()),
            Self::Long { view, .. } => FrontmatterValue::Text(buffer_text(&view.buffer())),
            Self::Entry(row) => {
                let text = row.text().to_string();
                match choice {
                    DocumentPropertyType::Number => parse_number(&text)
                        .map_or(FrontmatterValue::Text(text), FrontmatterValue::Number),
                    DocumentPropertyType::List => list_value(&text),
                    _ => FrontmatterValue::Text(text),
                }
            }
            Self::DateTime(picker) => picker.frontmatter(),
            Self::Hint(_) => FrontmatterValue::Null,
        }
    }

    /// Reads the value while keeping an unchanged typed value intact.
    ///
    /// The list editor joins elements with commas, so re-parsing an untouched row would turn
    /// `[true, 2]` into `["true", "2"]` and split elements containing commas. When the widget
    /// text still matches the rendered original, the parsed value is returned unchanged.
    fn frontmatter_preserving(
        &self,
        choice: DocumentPropertyType,
        previous: &FrontmatterValue,
    ) -> FrontmatterValue {
        match self {
            Self::Boolean(row) => FrontmatterValue::Boolean(row.is_active()),
            Self::Long { view, .. } => {
                let text = buffer_text(&view.buffer());
                if text == frontmatter_text(previous) {
                    previous.clone()
                } else {
                    FrontmatterValue::Text(text)
                }
            }
            Self::Entry(row) => {
                let text = row.text().to_string();
                let expected = match choice {
                    DocumentPropertyType::List => frontmatter_list_text(previous),
                    _ => frontmatter_text(previous),
                };
                if text == expected {
                    return previous.clone();
                }
                match choice {
                    DocumentPropertyType::Number => parse_number(&text)
                        .map_or(FrontmatterValue::Text(text), FrontmatterValue::Number),
                    DocumentPropertyType::List => list_value(&text),
                    _ => FrontmatterValue::Text(text),
                }
            }
            Self::DateTime(picker) => picker.frontmatter_preserving(previous),
            Self::Hint(_) => previous.clone(),
        }
    }

    fn connect_changed(&self, callback: &Rc<dyn Fn()>) {
        match self {
            Self::Entry(row) => {
                let callback = Rc::clone(callback);
                row.connect_changed(move |_| callback());
            }
            Self::Long { view, .. } => {
                let callback = Rc::clone(callback);
                view.buffer().connect_changed(move |_| callback());
            }
            Self::Boolean(row) => {
                let callback = Rc::clone(callback);
                row.connect_active_notify(move |_| callback());
            }
            Self::DateTime(_) | Self::Hint(_) => {}
        }
    }
}

/// The editable state of one property row.
// CONTEXT: A row independently tracks key/type editability, removability, and list selection;
// collapsing them into enums would obscure the flat draft state.
#[expect(
    clippy::struct_excessive_bools,
    reason = "a property row has independent key, type, removal, and selection flags"
)]
#[derive(Clone)]
struct PropertyDraft {
    key: String,
    choice: DocumentPropertyType,
    value: FrontmatterValue,
    /// Whether the user may rename the key (`title` and configured defaults are fixed).
    editable_key: bool,
    /// Whether the user may change the value type (configured defaults fix it).
    editable_kind: bool,
    /// Whether the row can be removed (`title` cannot).
    removable: bool,
    /// Whether a list property allows selecting more than one option.
    multiple: bool,
    /// The configured options of a list property; empty for custom properties.
    options: Vec<String>,
    /// Whether an authored empty or null value must survive an unchanged save.
    preserve_empty: bool,
    /// Whether the value is a derived prefill that is only persisted once edited.
    derived: bool,
}

impl PropertyDraft {
    /// A blank custom property.
    fn blank() -> Self {
        Self {
            key: String::new(),
            choice: DocumentPropertyType::Text,
            value: FrontmatterValue::Text(String::new()),
            editable_key: true,
            editable_kind: true,
            removable: true,
            multiple: false,
            options: Vec::new(),
            preserve_empty: false,
            derived: false,
        }
    }

    /// The reserved, always-present title row.
    fn title(value: FrontmatterValue) -> Self {
        Self {
            key: TITLE_KEY.to_owned(),
            choice: DocumentPropertyType::Text,
            value,
            editable_key: false,
            editable_kind: false,
            removable: false,
            multiple: false,
            options: Vec::new(),
            preserve_empty: false,
            derived: false,
        }
    }

    /// An ad-hoc note property with an editable name, type, and value.
    fn custom(key: String, value: FrontmatterValue) -> Self {
        let resolved = resolve_property(None, DocumentPropertyType::from_value(&value));
        Self::from_resolved(key, value, &resolved, false)
    }

    /// A note property whose key matches a configured default: the resolved type and options come
    /// from the default and the row cannot be removed, so the configured attributes persist.
    fn default_row(key: String, property: &DocumentProperty, value: FrontmatterValue) -> Self {
        let resolved = resolve_property(Some(property), DocumentPropertyType::from_value(&value));
        Self::from_resolved(key, value, &resolved, true)
    }

    /// A configured default edited in the defaults settings, where the type is editable.
    fn editable_property(property: &DocumentProperty) -> Self {
        Self::from_resolved(
            property.key.clone(),
            FrontmatterValue::from_json(&property.value),
            &property.resolved(),
            false,
        )
    }

    /// Builds a draft from the shared resolved property shape.
    ///
    /// `configured` fixes the key, type, and presence in the dialog because a configured default
    /// owns those attributes.
    fn from_resolved(
        key: String,
        value: FrontmatterValue,
        resolved: &ResolvedProperty,
        configured: bool,
    ) -> Self {
        Self {
            key,
            choice: resolved.field_type,
            value,
            editable_key: !configured,
            editable_kind: !configured,
            removable: !configured,
            multiple: resolved.multiple,
            options: resolved.options.clone(),
            preserve_empty: false,
            derived: false,
        }
    }
}

/// Which surface a row is built for.
#[derive(Clone, Copy, Eq, PartialEq)]
enum RowMode {
    /// The note's document-properties dialog.
    Note,
    /// The default-properties settings, where list options are edited.
    Defaults,
}

/// The value of a fixed-type row.
enum SimpleValue {
    /// A typed value input (`title`, text/number/boolean/long defaults).
    Typed(ValueWidget),
    /// A single-select option dropdown for a configured list property.
    Choice {
        combo: adw::ComboRow,
        options: Vec<String>,
    },
    /// A multi-select option list for a configured list property.
    Options {
        expander: adw::ExpanderRow,
        switches: Vec<adw::SwitchRow>,
    },
    /// A value that does not match the configured type or options; shown read-only.
    Disabled { row: adw::ActionRow },
}

impl SimpleValue {
    fn widget(&self) -> gtk::Widget {
        match self {
            Self::Typed(value) => value.widget(),
            Self::Choice { combo, .. } => combo.clone().upcast(),
            Self::Options { expander, .. } => expander.clone().upcast(),
            Self::Disabled { row } => row.clone().upcast(),
        }
    }

    fn add_suffix(&self, widget: &impl IsA<gtk::Widget>) {
        match self {
            Self::Typed(value) => value.add_suffix(widget),
            Self::Choice { combo, .. } => combo.add_suffix(widget),
            Self::Options { expander, .. } => expander.add_suffix(widget),
            Self::Disabled { row } => row.add_suffix(widget),
        }
    }

    /// Reads the selected value, preserving an unchanged typed value.
    fn frontmatter_preserving(
        &self,
        choice: DocumentPropertyType,
        previous: &FrontmatterValue,
    ) -> FrontmatterValue {
        match self {
            Self::Typed(value) => value.frontmatter_preserving(choice, previous),
            Self::Choice { combo, options } => {
                let selected = options
                    .get(usize::try_from(combo.selected()).unwrap_or(usize::MAX))
                    .cloned()
                    .unwrap_or_default();
                if selected == frontmatter_text(previous) {
                    previous.clone()
                } else {
                    FrontmatterValue::Text(selected)
                }
            }
            Self::Options { switches, .. } => {
                let selected = selected_options(switches);
                if selected == frontmatter_list_text(previous) {
                    previous.clone()
                } else {
                    list_value(&selected)
                }
            }
            // A value the dialog does not control is preserved exactly as authored.
            Self::Disabled { .. } => previous.clone(),
        }
    }

    /// Builds a read-only row for a value that does not match the configured type or options.
    fn disabled(draft: &PropertyDraft, index: usize, title: &str) -> Self {
        let row = adw::ActionRow::new();
        row.set_title(&markup_escape(title));
        row.set_widget_name(&value_name(index));
        row.set_subtitle(&gettext("Not managed by this dialog"));
        let value = gtk::Label::new(Some(&frontmatter_text(&draft.value)));
        value.add_css_class("dim-label");
        value.set_selectable(true);
        row.add_suffix(&value);
        row.set_sensitive(false);
        Self::Disabled { row }
    }

    /// Builds a single-select option dropdown for a configured list property.
    fn choice(draft: &PropertyDraft, index: usize, title: &str) -> Self {
        let current = frontmatter_text(&draft.value);
        let mut options = draft.options.clone();
        if !current.is_empty() && !options.iter().any(|option| option == &current) {
            options.push(current.clone());
        }
        let labels: Vec<&str> = options.iter().map(String::as_str).collect();
        let combo = adw::ComboRow::new();
        combo.set_title(&markup_escape(title));
        combo.set_model(Some(&gtk::StringList::new(&labels)));
        let selected = options
            .iter()
            .position(|option| option == &current)
            .unwrap_or(0);
        combo.set_selected(u32::try_from(selected).unwrap_or(0));
        combo.set_widget_name(&value_name(index));
        Self::Choice { combo, options }
    }

    /// Builds a multi-select option list for a configured list property.
    fn options(draft: &PropertyDraft, index: usize, title: &str) -> Self {
        let expander = adw::ExpanderRow::new();
        expander.set_title(&markup_escape(title));
        expander.set_widget_name(&value_name(index));
        let selected: Vec<String> = match &draft.value {
            FrontmatterValue::List(items) => items.iter().map(frontmatter_text).collect(),
            other => vec![frontmatter_text(other)],
        };
        let switches: Vec<adw::SwitchRow> = draft
            .options
            .iter()
            .map(|option| {
                let row = adw::SwitchRow::new();
                row.set_title(&markup_escape(option));
                row.set_active(selected.iter().any(|item| item == option));
                expander.add_row(&row);
                row
            })
            .collect();
        let refresh: Rc<dyn Fn()> = {
            let expander = expander.clone();
            let switches = switches.clone();
            Rc::new(move || {
                let selected = selected_options(&switches);
                expander.set_subtitle(&markup_escape(&preview_text(&list_value(&selected))));
            })
        };
        for row in &switches {
            let refresh = Rc::clone(&refresh);
            row.connect_active_notify(move |_| refresh());
        }
        refresh();
        Self::Options { expander, switches }
    }
}

/// Widgets backing one property row.
enum BuiltRow {
    /// A custom property with editable name, type, and value.
    Expander {
        expander: adw::ExpanderRow,
        key: adw::EntryRow,
        kind: adw::ComboRow,
        value: ValueWidget,
    },
    /// A fixed-type row (`title` or a configured default).
    Simple {
        widget: gtk::Widget,
        value: SimpleValue,
    },
}

impl BuiltRow {
    fn widget(&self) -> gtk::Widget {
        match self {
            Self::Expander { expander, .. } => expander.clone().upcast(),
            Self::Simple { widget, .. } => widget.clone(),
        }
    }

    /// Returns whether this row exposes its editable content.
    fn is_expanded(&self) -> bool {
        match self {
            Self::Expander { expander, .. } => expander.is_expanded(),
            Self::Simple { .. } => false,
        }
    }

    /// Expands or collapses this row's editable content.
    fn set_expanded(&self, expanded: bool) {
        if let Self::Expander { expander, .. } = self {
            expander.set_expanded(expanded);
        }
    }

    /// Expands this row and focuses its name entry so a new property can be typed immediately.
    fn expand_and_focus(&self) {
        self.set_expanded(true);
        self.focus_key();
    }

    /// Focuses this row's name entry.
    fn focus_key(&self) {
        if let Self::Expander { key, .. } = self {
            key.grab_focus();
        }
    }

    /// Keeps keyboard focus on this row's type control after a rebuild.
    fn focus_kind(&self) {
        if let Self::Expander { kind, .. } = self {
            kind.grab_focus();
        }
    }
}

type Rows = Rc<RefCell<Vec<BuiltRow>>>;
type Drafts = Rc<RefCell<Vec<PropertyDraft>>>;

/// Presents the native document-properties dialog for an immutable snapshot.
///
/// The snapshot's [`PropertiesSave`] target decides whether a save applies to the open editor or
/// to a Base row, so the same dialog serves both surfaces.
// CONTEXT: The dialog builds its whole page and save route in one function so the editor and Base
// targets share an identical presentation.
#[expect(
    clippy::too_many_lines,
    reason = "the dialog keeps its shared presentation and save routing together"
)]
pub(crate) fn show(
    parent: &gtk::Window,
    dispatcher: &AppDispatcher,
    request: &EditorPropertiesRequest,
    syntax_dir: Option<&std::path::Path>,
) {
    let dialog = adw::Dialog::builder()
        .title(gettext("Document Properties"))
        .follows_content_size(true)
        .content_width(560)
        .build();
    dialog.set_widget_name("document-properties-dialog");

    let header = adw::HeaderBar::new();
    let cancel = gtk::Button::with_label(&gettext("Cancel"));
    cancel.set_widget_name("document-properties-cancel");
    let save = gtk::Button::with_label(&gettext("Save"));
    save.add_css_class("suggested-action");
    save.set_widget_name("document-properties-save");
    header.pack_start(&cancel);
    header.pack_end(&save);

    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&header);

    let page = adw::PreferencesPage::new();
    page.set_width_request(560);

    // The frontmatter format is configured in Settings; the dialog preserves the note's own
    // format and never changes it here.
    let format = request
        .document
        .as_ref()
        .map_or(request.default_format, |document| document.format);

    let raw_mode = request.document.as_ref().is_some_and(|document| {
        document.error.is_some()
            || document
                .fields
                .iter()
                .any(|field| !is_editable_value(&field.value))
    });

    // Category leads the dialog because it is the note's top-level place, not frontmatter. It is
    // offered whenever the caller can persist a move.
    let category_control = request.category_id.and_then(|current| {
        (!request.categories.is_empty()).then(|| {
            let group = adw::PreferencesGroup::new();
            group.set_title(&gettext("Category"));
            let combo = adw::ComboRow::new();
            combo.set_title(&gettext("Category"));
            combo.set_widget_name("document-properties-category");
            let names: Vec<&str> = request
                .categories
                .iter()
                .map(|category| category.name.as_str())
                .collect();
            combo.set_model(Some(&gtk::StringList::new(&names)));
            let selected = request
                .categories
                .iter()
                .position(|category| category.id == current)
                .unwrap_or(0);
            combo.set_selected(u32::try_from(selected).unwrap_or(0));
            group.add(&combo);
            page.add(&group);
            (combo, request.categories.clone(), current)
        })
    });

    let save_source = if raw_mode {
        let group = adw::PreferencesGroup::new();
        group.set_title(&gettext("Raw frontmatter"));
        let (view, buffer) = raw_frontmatter_view(
            format,
            syntax_dir,
            request.raw.as_deref().unwrap_or_default(),
        );
        follow_color_scheme(&dialog, &buffer, syntax_dir);
        let scroll = gtk::ScrolledWindow::new();
        scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
        scroll.set_min_content_height(220);
        scroll.add_css_class("card");
        scroll.set_child(Some(&view));
        let action = adw::ActionRow::builder()
            .title(gettext("Content"))
            .child(&scroll)
            .build();
        group.add(&action);
        page.add(&group);
        SaveSource::Raw(view)
    } else {
        let group = adw::PreferencesGroup::new();
        group.set_widget_name("document-properties-group");
        group.set_description(Some(&gettext(
            "This note's properties are stored in its frontmatter. Configure default properties for new notes in Preferences.",
        )));
        let drafts: Drafts = Rc::new(RefCell::new(initial_drafts(request)));
        let rows: Rows = Rc::new(RefCell::new(Vec::new()));
        let on_change: Rc<dyn Fn()> = Rc::new(|| {});
        rebuild(&group, &rows, &drafts, &on_change, RowMode::Note);
        page.add(&group);

        let actions = adw::PreferencesGroup::new();
        let add = button_row("document-properties-add", &gettext("Add property"));
        actions.add(&add);
        page.add(&actions);
        let add_action = add_property_action(&group, &rows, &drafts, &on_change, RowMode::Note);
        add.connect_activated(move |_| add_action());

        SaveSource::Fields { rows, drafts }
    };

    toolbar.set_content(Some(&page));
    dialog.set_child(Some(&toolbar));

    let draft_cancel = dialog.clone();
    cancel.connect_clicked(move |_| {
        let _ = draft_cancel.close();
    });

    let save_target = request.save;
    let dispatcher = dispatcher.clone();
    let dialog_for_save = dialog.clone();
    save.connect_clicked(move |_| {
        let edit = match &save_source {
            SaveSource::Raw(view) => FrontmatterEdit::Raw {
                format,
                content: buffer_text(&view.buffer()),
            },
            SaveSource::Fields { rows, drafts } => {
                capture(rows, drafts);
                FrontmatterEdit::Parsed(build_document(format, &drafts.borrow()))
            }
        };
        let category = category_control
            .as_ref()
            .and_then(|(combo, categories, current)| {
                let selected = categories.get(combo.selected() as usize)?.id;
                (selected != *current).then_some(selected)
            });
        let _ = dialog_for_save.close();
        let message = match save_target {
            PropertiesSave::Editor { session } => AppMsg::Editor(EditorMsg::ApplyFrontmatter {
                session,
                edit,
                category,
            }),
            PropertiesSave::Base { note_id, revision } => {
                AppMsg::Bases(BasesMsg::ApplyProperties {
                    note_id,
                    revision,
                    edit,
                    category,
                })
            }
        };
        let _ = dispatcher.dispatch(message);
    });

    dialog.present(Some(parent));
}

/// Builds the raw frontmatter source view.
///
/// The block is highlighted with the grammar for its declared format (YAML, TOML, or JSON) and the
/// Carve style scheme matching the app color scheme, and is inset from the card edges.
fn raw_frontmatter_view(
    format: FrontmatterFormat,
    syntax_dir: Option<&std::path::Path>,
    content: &str,
) -> (sourceview5::View, sourceview5::Buffer) {
    let buffer = super::frontmatter_source::source_buffer(
        format,
        syntax_dir,
        adw::StyleManager::default().is_dark(),
    );
    buffer.set_text(content);
    let view = sourceview5::View::with_buffer(&buffer);
    view.set_widget_name("document-properties-raw");
    view.set_wrap_mode(gtk::WrapMode::WordChar);
    view.set_monospace(true);
    // Keep the highlighted text clear of the card edges and the scrollbar.
    view.set_top_margin(12);
    view.set_bottom_margin(12);
    view.set_left_margin(12);
    view.set_right_margin(12);
    (view, buffer)
}

/// Keeps the raw frontmatter highlight in step with the application color scheme.
///
/// The scheme is re-derived from the installed assets on every dark/light change and the handler
/// is released when the dialog closes.
fn follow_color_scheme(
    dialog: &adw::Dialog,
    buffer: &sourceview5::Buffer,
    syntax_dir: Option<&std::path::Path>,
) {
    let Some(syntax_dir) = syntax_dir.map(std::path::Path::to_path_buf) else {
        return;
    };
    let buffer = buffer.clone();
    let handler = adw::StyleManager::default().connect_dark_notify(move |manager| {
        if let Ok(scheme) = super::source::frontmatter_style_scheme(&syntax_dir, manager.is_dark())
        {
            buffer.set_style_scheme(Some(&scheme));
        }
    });
    // `connect_closed` takes an `Fn`, so release the handler at most once through shared state.
    let handler = Rc::new(RefCell::new(Some(handler)));
    dialog.connect_closed(move |_| {
        if let Some(handler) = handler.borrow_mut().take() {
            adw::StyleManager::default().disconnect(handler);
        }
    });
}

/// What the per-note dialog reads when saving.
enum SaveSource {
    Raw(sourceview5::View),
    Fields { rows: Rows, drafts: Drafts },
}

/// Presents the editor for the user-configurable default properties.
///
/// `entries` is shared with the caller so reopening the dialog reflects the latest persisted
/// defaults rather than the snapshot captured when the Preferences dialog was built.
pub(crate) fn show_defaults(
    parent: Option<&gtk::Window>,
    dispatcher: &AppDispatcher,
    entries: &Rc<RefCell<Vec<DocumentProperty>>>,
) -> adw::Dialog {
    let dialog = adw::Dialog::builder()
        .title(gettext("Default properties"))
        .follows_content_size(true)
        .content_width(560)
        .build();
    dialog.set_widget_name("document-properties-defaults-dialog");
    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&adw::HeaderBar::new());

    let page = adw::PreferencesPage::new();
    page.set_width_request(560);
    let group = adw::PreferencesGroup::new();
    group.set_title(&gettext("Default properties"));
    group.set_description(Some(&gettext(
        "These properties are offered in every document-properties dialog and, when enabled, seed new notes.",
    )));
    let actions = adw::PreferencesGroup::new();
    let add = button_row("document-property-add", &gettext("Add property"));
    actions.add(&add);
    page.add(&group);
    page.add(&actions);
    toolbar.set_content(Some(&page));
    dialog.set_child(Some(&toolbar));

    let drafts: Drafts = Rc::new(RefCell::new(
        entries
            .borrow()
            .iter()
            .map(PropertyDraft::editable_property)
            .collect(),
    ));
    let rows: Rows = Rc::new(RefCell::new(Vec::new()));
    let on_change: Rc<dyn Fn()> = {
        let drafts = Rc::clone(&drafts);
        let rows = Rc::clone(&rows);
        let entries = Rc::clone(entries);
        let dispatcher = dispatcher.clone();
        Rc::new(move || persist_defaults(&rows, &drafts, &entries, &dispatcher))
    };
    rebuild(&group, &rows, &drafts, &on_change, RowMode::Defaults);
    let add_action = add_property_action(&group, &rows, &drafts, &on_change, RowMode::Defaults);
    add.connect_activated(move |_| add_action());

    let close_drafts = Rc::clone(&drafts);
    let close_rows = Rc::clone(&rows);
    let close_entries = Rc::clone(entries);
    let close_dispatcher = dispatcher.clone();
    dialog.connect_closed(move |_| {
        persist_defaults(
            &close_rows,
            &close_drafts,
            &close_entries,
            &close_dispatcher,
        );
    });

    dialog.present(parent);
    dialog
}

fn button_row(name: &str, title: &str) -> adw::ButtonRow {
    let row = adw::ButtonRow::new();
    row.set_title(title);
    row.set_start_icon_name(Some("list-add-symbolic"));
    row.set_widget_name(name);
    row
}

fn initial_drafts(request: &EditorPropertiesRequest) -> Vec<PropertyDraft> {
    let mut drafts: Vec<PropertyDraft> = Vec::new();
    let mut seen_keys: BTreeSet<String> = BTreeSet::new();
    let mut authored_title: Option<FrontmatterValue> = None;
    if let Some(document) = &request.document {
        for field in &document.fields {
            if !seen_keys.insert(field.key.clone()) {
                continue;
            }
            if field.key == TITLE_KEY {
                authored_title = Some(field.value.clone());
                continue;
            }
            let draft = match configured_property(&request.defaults, &field.key) {
                Some(property) => {
                    PropertyDraft::default_row(field.key.clone(), property, field.value.clone())
                }
                None => PropertyDraft::custom(field.key.clone(), field.value.clone()),
            };
            drafts.push(authored_draft(draft));
        }
    }
    // The reserved title always leads the dialog. It shows the authored value, otherwise the
    // heading prefill (which is only persisted once the user edits it), otherwise empty.
    let title = match authored_title {
        Some(value) => authored_draft(PropertyDraft::title(value)),
        None => match request.heading_title.as_deref() {
            Some(heading) => {
                let mut draft = PropertyDraft::title(FrontmatterValue::Text(heading.to_owned()));
                draft.derived = true;
                draft
            }
            None => PropertyDraft::title(FrontmatterValue::Text(String::new())),
        },
    };
    drafts.insert(0, title);
    // Configured defaults absent from the note are appended, so the authored key order is
    // preserved and an unchanged save round-trips byte-for-byte.
    for property in &request.defaults {
        if is_reserved_key(&property.key) || seen_keys.contains(&property.key) {
            continue;
        }
        let value = property
            .default_field()
            .map_or(FrontmatterValue::Null, |field| field.value);
        drafts.push(PropertyDraft::default_row(
            property.key.clone(),
            property,
            value,
        ));
    }
    drafts
}

/// Marks a draft built from an authored field so an explicit empty or null value survives an
/// unchanged save.
fn authored_draft(mut draft: PropertyDraft) -> PropertyDraft {
    draft.preserve_empty = is_blank_value(&draft.value);
    draft
}

/// Returns whether a value is an explicit empty text or null.
fn is_blank_value(value: &FrontmatterValue) -> bool {
    match value {
        FrontmatterValue::Null => true,
        FrontmatterValue::Text(text) => text.trim().is_empty(),
        _ => false,
    }
}

fn build_document(format: FrontmatterFormat, drafts: &[PropertyDraft]) -> FrontmatterDocument {
    let fields = drafts
        .iter()
        .filter_map(|draft| {
            let key = draft.key.trim();
            if key.is_empty() {
                return None;
            }
            // A derived title prefill is only written once the user edits it.
            if draft.derived {
                return None;
            }
            // Empty and null values write no key unless they were authored and left untouched, so
            // unfilled defaults and cleared fields disappear while explicit empties survive.
            if !draft.preserve_empty && is_blank_value(&draft.value) {
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

fn rebuild(
    group: &adw::PreferencesGroup,
    rows: &Rows,
    drafts: &Drafts,
    on_change: &Rc<dyn Fn()>,
    mode: RowMode,
) {
    // Keep the rows the user opened open across a rebuild (for example after a type change).
    let expanded: Vec<bool> = rows.borrow().iter().map(BuiltRow::is_expanded).collect();
    {
        let built = rows.borrow();
        for row in built.iter() {
            group.remove(&row.widget());
        }
    }
    rows.borrow_mut().clear();
    let current = drafts.borrow().clone();
    for (index, draft) in current.iter().enumerate() {
        let row = build_row(draft, index, group, rows, drafts, on_change, mode);
        if expanded.get(index).copied().unwrap_or(false) {
            row.set_expanded(true);
        }
        group.add(&row.widget());
        rows.borrow_mut().push(row);
    }
}

fn build_row(
    draft: &PropertyDraft,
    index: usize,
    group: &adw::PreferencesGroup,
    rows: &Rows,
    drafts: &Drafts,
    on_change: &Rc<dyn Fn()>,
    mode: RowMode,
) -> BuiltRow {
    if draft.editable_kind {
        build_expander_row(draft, index, group, rows, drafts, on_change, mode)
    } else {
        build_simple_row(draft, index, group, rows, drafts, on_change, mode)
    }
}

/// Builds a fixed-type row: `title` and configured defaults use a plain value input.
fn build_simple_row(
    draft: &PropertyDraft,
    index: usize,
    group: &adw::PreferencesGroup,
    rows: &Rows,
    drafts: &Drafts,
    on_change: &Rc<dyn Fn()>,
    mode: RowMode,
) -> BuiltRow {
    let title = display_title(&draft.key);
    // A configured list property offers its options. Any value that does not match the configured
    // type or options is shown read-only so the user knows it is authored by hand.
    let configured_list = draft.choice == DocumentPropertyType::List && !draft.options.is_empty();
    let value = if !configured_value_supported(draft) {
        SimpleValue::disabled(draft, index, &title)
    } else if configured_list && draft.multiple {
        SimpleValue::options(draft, index, &title)
    } else if configured_list {
        SimpleValue::choice(draft, index, &title)
    } else {
        SimpleValue::Typed(ValueWidget::build(
            draft.choice,
            &draft.value,
            &value_name(index),
            &title,
        ))
    };
    let widget = value.widget();
    if draft.removable {
        let remove = remove_button(index, group, rows, drafts, on_change, mode);
        value.add_suffix(&remove);
    }
    BuiltRow::Simple { widget, value }
}

// CONTEXT: The row keeps its widgets and its live callbacks together so the draft is the only
// shared state between the note editor and the defaults editor.
#[expect(
    clippy::too_many_lines,
    reason = "one property row wires key, type, value, hint, and removal together"
)]
fn build_expander_row(
    draft: &PropertyDraft,
    index: usize,
    group: &adw::PreferencesGroup,
    rows: &Rows,
    drafts: &Drafts,
    on_change: &Rc<dyn Fn()>,
    mode: RowMode,
) -> BuiltRow {
    let expander = adw::ExpanderRow::new();
    expander.set_widget_name(&format!("document-property-row-{index}"));

    let key = adw::EntryRow::new();
    key.set_title(&gettext("Name"));
    key.set_text(&draft.key);
    key.set_editable(draft.editable_key);
    key.set_widget_name(&format!("document-property-key-{index}"));

    let kind = adw::ComboRow::new();
    kind.set_title(&gettext("Type"));
    kind.set_model(Some(&type_model()));
    kind.set_selected(type_index(draft.choice));
    kind.set_widget_name(&format!("document-property-kind-{index}"));

    let value = if mode == RowMode::Defaults && draft.choice.is_date() {
        ValueWidget::hint(
            &if draft.choice == DocumentPropertyType::Date {
                gettext("New notes use today's date")
            } else {
                gettext("New notes use the current date and time")
            },
            &value_name(index),
        )
    } else if mode == RowMode::Defaults && draft.choice == DocumentPropertyType::List {
        ValueWidget::build(
            draft.choice,
            &draft.value,
            &value_name(index),
            &gettext("Options"),
        )
    } else {
        ValueWidget::build(
            draft.choice,
            &draft.value,
            &value_name(index),
            &gettext("Value"),
        )
    };

    expander.add_row(&key);
    expander.add_row(&kind);
    expander.add_row(&value.widget());

    if mode == RowMode::Defaults && draft.choice == DocumentPropertyType::List {
        let hint = gtk::Label::new(Some(&gettext(
            "Comma-separated values offered as a dropdown.",
        )));
        hint.set_wrap(true);
        hint.set_xalign(0.0);
        hint.add_css_class("dim-label");
        hint.set_margin_start(12);
        hint.set_margin_end(12);
        hint.set_margin_top(4);
        expander.add_row(&hint);

        let multiple = adw::SwitchRow::new();
        multiple.set_title(&gettext("Allow multiple values"));
        multiple.set_active(draft.multiple);
        multiple.set_widget_name(&format!("document-property-multiple-{index}"));
        {
            let drafts = Rc::clone(drafts);
            let on_change = Rc::clone(on_change);
            multiple.connect_active_notify(move |row| {
                if let Some(draft) = drafts.borrow_mut().get_mut(index) {
                    draft.multiple = row.is_active();
                }
                on_change();
            });
        }
        expander.add_row(&multiple);
    }

    let refresh: Rc<dyn Fn()> = {
        let expander = expander.clone();
        let key = key.clone();
        let kind = kind.clone();
        let value = value.clone();
        Rc::new(move || {
            let choice = type_from_index(kind.selected());
            let key_text = key.text().to_string();
            let title = if key_text.trim().is_empty() {
                gettext("New property")
            } else {
                key_text
            };
            expander.set_title(&markup_escape(&title));
            expander.set_subtitle(&row_subtitle(choice, &value.frontmatter(choice)));
        })
    };
    refresh();
    {
        let refresh = Rc::clone(&refresh);
        key.connect_changed(move |_| refresh());
    }
    value.connect_changed(&refresh);

    {
        let group = group.clone();
        let rows = Rc::clone(rows);
        let drafts = Rc::clone(drafts);
        let on_change = Rc::clone(on_change);
        kind.connect_selected_notify(move |dropdown| {
            let choice = type_from_index(dropdown.selected());
            let group = group.clone();
            let rows = Rc::clone(&rows);
            let drafts = Rc::clone(&drafts);
            let on_change = Rc::clone(&on_change);
            glib::idle_add_local_once(move || {
                capture(&rows, &drafts);
                if let Some(draft) = drafts.borrow_mut().get_mut(index) {
                    draft.choice = choice;
                }
                rebuild(&group, &rows, &drafts, &on_change, mode);
                if let Some(row) = rows.borrow().get(index) {
                    row.focus_kind();
                }
                on_change();
            });
        });
    }

    if draft.removable {
        let remove = remove_button(index, group, rows, drafts, on_change, mode);
        expander.add_suffix(&remove);
    }

    BuiltRow::Expander {
        expander,
        key,
        kind,
        value,
    }
}

fn remove_button(
    index: usize,
    group: &adw::PreferencesGroup,
    rows: &Rows,
    drafts: &Drafts,
    on_change: &Rc<dyn Fn()>,
    mode: RowMode,
) -> gtk::Button {
    let remove = gtk::Button::from_icon_name("edit-delete-symbolic");
    remove.set_widget_name(&format!("document-property-remove-{index}"));
    remove.add_css_class("flat");
    remove.set_tooltip_text(Some(&gettext("Remove property")));
    remove.update_property(&[gtk::accessible::Property::Label(&gettext(
        "Remove property",
    ))]);
    let group = group.clone();
    let rows = Rc::clone(rows);
    let drafts = Rc::clone(drafts);
    let on_change = Rc::clone(on_change);
    remove.connect_clicked(move |_| {
        let group = group.clone();
        let rows = Rc::clone(&rows);
        let drafts = Rc::clone(&drafts);
        let on_change = Rc::clone(&on_change);
        glib::idle_add_local_once(move || {
            capture(&rows, &drafts);
            {
                let mut current = drafts.borrow_mut();
                if index < current.len() {
                    current.remove(index);
                }
            }
            rebuild(&group, &rows, &drafts, &on_change, mode);
            let target = index.min(rows.borrow().len().saturating_sub(1));
            if let Some(row) = rows.borrow().get(target) {
                row.focus_key();
            }
            on_change();
        });
    });
    remove
}

fn add_property_action(
    group: &adw::PreferencesGroup,
    rows: &Rows,
    drafts: &Drafts,
    on_change: &Rc<dyn Fn()>,
    mode: RowMode,
) -> Rc<dyn Fn()> {
    let group = group.clone();
    let rows = Rc::clone(rows);
    let drafts = Rc::clone(drafts);
    let on_change = Rc::clone(on_change);
    Rc::new(move || {
        let group = group.clone();
        let rows = Rc::clone(&rows);
        let drafts = Rc::clone(&drafts);
        let on_change = Rc::clone(&on_change);
        glib::idle_add_local_once(move || {
            capture(&rows, &drafts);
            drafts.borrow_mut().push(PropertyDraft::blank());
            rebuild(&group, &rows, &drafts, &on_change, mode);
            if let Some(row) = rows.borrow().last() {
                row.expand_and_focus();
            }
            on_change();
        });
    })
}

fn capture(rows: &Rows, drafts: &Drafts) {
    let built = rows.borrow();
    let mut current = drafts.borrow_mut();
    for (draft, row) in current.iter_mut().zip(built.iter()) {
        match row {
            BuiltRow::Expander {
                key, kind, value, ..
            } => {
                let choice = type_from_index(kind.selected());
                draft.key = key.text().to_string();
                draft.choice = choice;
                let next = value.frontmatter_preserving(choice, &draft.value);
                if next != draft.value {
                    // An edited value is no longer an authored empty or a derived prefill.
                    draft.preserve_empty = false;
                    draft.derived = false;
                }
                draft.value = next;
            }
            BuiltRow::Simple { value, .. } => {
                // The key and type are fixed; only the value is read back.
                let next = value.frontmatter_preserving(draft.choice, &draft.value);
                if next != draft.value {
                    draft.preserve_empty = false;
                    draft.derived = false;
                }
                draft.value = next;
            }
        }
    }
}

fn persist_defaults(
    rows: &Rows,
    drafts: &Drafts,
    entries: &Rc<RefCell<Vec<DocumentProperty>>>,
    dispatcher: &AppDispatcher,
) {
    capture(rows, drafts);
    let properties = normalized_default_properties(&drafts.borrow());
    entries.borrow_mut().clone_from(&properties);
    let _ = dispatcher.dispatch(AppMsg::Preferences(PreferencesMsg::SetDocumentProperties(
        properties,
    )));
}

fn normalized_default_properties(drafts: &[PropertyDraft]) -> Vec<DocumentProperty> {
    let mut seen = BTreeSet::new();
    let mut normalized = Vec::new();
    for draft in drafts {
        let key = draft.key.trim().to_owned();
        if key.is_empty() || is_reserved_key(&key) {
            continue;
        }
        if !seen.insert(key.clone()) {
            continue;
        }
        let field_type = draft.choice;
        let multiple = field_type == DocumentPropertyType::List && draft.multiple;
        let value = if field_type.is_date() {
            // Date and date-time defaults are dynamic, so no fixed value is stored.
            serde_json::Value::String(String::new())
        } else {
            field_type.normalized_json(&draft.value.to_json())
        };
        normalized.push(DocumentProperty {
            key,
            field_type,
            multiple,
            value,
        });
    }
    normalized
}

fn row_subtitle(choice: DocumentPropertyType, value: &FrontmatterValue) -> String {
    let preview = preview_text(value);
    let subtitle = if preview.is_empty() {
        type_label(choice)
    } else {
        format!("{} · {preview}", type_label(choice))
    };
    markup_escape(&subtitle)
}

fn preview_text(value: &FrontmatterValue) -> String {
    let text = match value {
        FrontmatterValue::List(items) => items
            .iter()
            .map(frontmatter_text)
            .collect::<Vec<_>>()
            .join(", "),
        other => frontmatter_text(other),
    };
    let collapsed = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.chars().count() > 60 {
        let mut truncated: String = collapsed.chars().take(59).collect();
        truncated.push('…');
        truncated
    } else {
        collapsed
    }
}

fn buffer_text(buffer: &gtk::TextBuffer) -> String {
    buffer
        .text(&buffer.start_iter(), &buffer.end_iter(), false)
        .to_string()
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

fn display_title(key: &str) -> String {
    if key == TITLE_KEY {
        gettext("Title")
    } else {
        key.to_owned()
    }
}

fn value_name(index: usize) -> String {
    format!("document-property-value-{index}")
}

/// Parses a comma-separated option string into a list value.
fn list_value(text: &str) -> FrontmatterValue {
    FrontmatterValue::List(
        text.split(',')
            .map(str::trim)
            .filter(|item| !item.is_empty())
            .map(|item| FrontmatterValue::Text(item.to_owned()))
            .collect(),
    )
}

/// Joins the active option switches into a comma-separated string.
fn selected_options(switches: &[adw::SwitchRow]) -> String {
    switches
        .iter()
        .filter(|row| row.is_active())
        .map(|row| row.title().to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

/// Returns whether a note value is representable by the configured default for its row.
fn configured_value_supported(draft: &PropertyDraft) -> bool {
    // A configured list with options is an option set, so its members are checked here; the
    // scalar/date acceptance comes from the shared domain type.
    if draft.choice == DocumentPropertyType::List && !draft.options.is_empty() {
        return if draft.multiple {
            matches!(
                &draft.value,
                FrontmatterValue::List(items) if items.iter().all(|item| option_supported(draft, item))
            )
        } else {
            matches!(
                &draft.value,
                FrontmatterValue::Text(text) if draft.options.iter().any(|option| option == text)
            )
        };
    }
    draft.choice.accepts_value(&draft.value)
}

fn option_supported(draft: &PropertyDraft, item: &FrontmatterValue) -> bool {
    matches!(item, FrontmatterValue::Text(text) if draft.options.iter().any(|option| option == text))
}

#[cfg(test)]
mod tests;
