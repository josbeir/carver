//! Native document-properties dialogs.
//!
//! Both the per-note editor and the default-properties editor are built from libadwaita
//! preference rows: each property is an [`adw::ExpanderRow`] whose collapsed state shows the
//! key and a value preview and whose expanded state reveals the name, type, and value rows.
//! The dialogs never own document state: saving dispatches [`EditorMsg::ApplyFrontmatter`] and
//! the MVU runtime splices the result into canonical Carve source.

use std::{cell::RefCell, collections::BTreeSet, rc::Rc};

use carver_config::DocumentProperty;
use carver_domain::{
    FrontmatterDocument, FrontmatterField, FrontmatterFormat, FrontmatterValue, PropertyKind,
    is_reserved_key,
};
use gettextrs::gettext;
use gtk::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;

use crate::mvu::{
    AppDispatcher, AppMsg, EditorMsg, EditorPropertiesRequest, FrontmatterEdit, PreferencesMsg,
};

/// The reserved, always-present title field key.
const TITLE_KEY: &str = "title";

/// The value kinds offered by the document-properties dialogs.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PropertyKindChoice {
    /// A single-line text value.
    Text,
    /// A multi-line text value.
    LongText,
    /// A numeric value.
    Number,
    /// A boolean value.
    Boolean,
    /// A list of text values.
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

    fn label(self) -> String {
        match self {
            Self::Text => gettext("Text"),
            Self::LongText => gettext("Long text"),
            Self::Number => gettext("Number"),
            Self::Boolean => gettext("Boolean"),
            Self::List => gettext("List"),
        }
    }
}

fn kind_model() -> gtk::StringList {
    gtk::StringList::new(&[
        &gettext("Text"),
        &gettext("Long text"),
        &gettext("Number"),
        &gettext("Boolean"),
        &gettext("List"),
    ])
}

fn format_model() -> gtk::StringList {
    gtk::StringList::new(&[&gettext("YAML"), &gettext("JSON"), &gettext("TOML")])
}

fn selected_format(dropdown: &adw::ComboRow) -> FrontmatterFormat {
    match dropdown.selected() {
        1 => FrontmatterFormat::Json,
        2 => FrontmatterFormat::Toml,
        _ => FrontmatterFormat::Yaml,
    }
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
}

impl ValueWidget {
    fn build(
        choice: PropertyKindChoice,
        value: &FrontmatterValue,
        name: &str,
        title: &str,
    ) -> Self {
        match choice {
            PropertyKindChoice::Boolean => {
                let row = adw::SwitchRow::new();
                row.set_title(title);
                row.set_widget_name(name);
                row.set_active(matches!(value, FrontmatterValue::Boolean(true)));
                Self::Boolean(row)
            }
            PropertyKindChoice::LongText => {
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
                    .title(title)
                    .child(&scroll)
                    .build();
                Self::Long { row, view }
            }
            _ => {
                let row = adw::EntryRow::new();
                row.set_title(title);
                row.set_widget_name(name);
                if choice == PropertyKindChoice::Number {
                    row.set_input_purpose(gtk::InputPurpose::Number);
                }
                let text = if choice == PropertyKindChoice::List {
                    frontmatter_list_text(value)
                } else {
                    frontmatter_text(value)
                };
                row.set_text(&text);
                Self::Entry(row)
            }
        }
    }

    fn widget(&self) -> gtk::Widget {
        match self {
            Self::Entry(row) => row.clone().upcast(),
            Self::Long { row, .. } => row.clone().upcast(),
            Self::Boolean(row) => row.clone().upcast(),
        }
    }

    fn add_suffix(&self, widget: &impl IsA<gtk::Widget>) {
        match self {
            Self::Entry(row) => row.add_suffix(widget),
            Self::Long { row, .. } => row.add_suffix(widget),
            Self::Boolean(row) => row.add_suffix(widget),
        }
    }

    fn frontmatter(&self, choice: PropertyKindChoice) -> FrontmatterValue {
        match self {
            Self::Boolean(row) => FrontmatterValue::Boolean(row.is_active()),
            Self::Long { view, .. } => FrontmatterValue::Text(buffer_text(&view.buffer())),
            Self::Entry(row) => {
                let text = row.text().to_string();
                match choice {
                    PropertyKindChoice::Number => parse_number(&text)
                        .map_or(FrontmatterValue::Text(text), FrontmatterValue::Number),
                    PropertyKindChoice::List => FrontmatterValue::List(
                        text.split(',')
                            .map(str::trim)
                            .filter(|item| !item.is_empty())
                            .map(|item| FrontmatterValue::Text(item.to_owned()))
                            .collect(),
                    ),
                    _ => FrontmatterValue::Text(text),
                }
            }
        }
    }

    /// Reads the value while keeping an unchanged typed value intact.
    ///
    /// The list editor joins elements with commas, so re-parsing an untouched row would turn
    /// `[true, 2]` into `["true", "2"]` and split elements containing commas. When the widget
    /// text still matches the rendered original, the parsed value is returned unchanged.
    fn frontmatter_preserving(
        &self,
        choice: PropertyKindChoice,
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
                    PropertyKindChoice::List => frontmatter_list_text(previous),
                    _ => frontmatter_text(previous),
                };
                if text == expected {
                    return previous.clone();
                }
                match choice {
                    PropertyKindChoice::Number => parse_number(&text)
                        .map_or(FrontmatterValue::Text(text), FrontmatterValue::Number),
                    PropertyKindChoice::List => FrontmatterValue::List(
                        text.split(',')
                            .map(str::trim)
                            .filter(|item| !item.is_empty())
                            .map(|item| FrontmatterValue::Text(item.to_owned()))
                            .collect(),
                    ),
                    _ => FrontmatterValue::Text(text),
                }
            }
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
        }
    }
}

/// The editable state of one property row.
#[derive(Clone)]
struct PropertyDraft {
    key: String,
    choice: PropertyKindChoice,
    value: FrontmatterValue,
    /// Whether the user may rename the key (`title` and configured defaults are fixed).
    editable_key: bool,
    /// Whether the user may change the value type (configured defaults fix it).
    editable_kind: bool,
    /// Whether the row can be removed (`title` cannot).
    removable: bool,
}

impl PropertyDraft {
    /// A blank custom property.
    fn blank() -> Self {
        Self {
            key: String::new(),
            choice: PropertyKindChoice::Text,
            value: FrontmatterValue::Text(String::new()),
            editable_key: true,
            editable_kind: true,
            removable: true,
        }
    }

    /// A note property whose key matches a configured default: the type comes from the default.
    fn default_row(key: String, property: &DocumentProperty, value: FrontmatterValue) -> Self {
        Self {
            key,
            choice: PropertyKindChoice::for_property(property),
            value,
            editable_key: false,
            editable_kind: false,
            removable: true,
        }
    }

    /// A configured default edited in the defaults settings, where the type is editable.
    fn editable_property(property: &DocumentProperty) -> Self {
        Self {
            key: property.key.clone(),
            choice: PropertyKindChoice::for_property(property),
            value: FrontmatterValue::from_json(&property.value),
            editable_key: true,
            editable_kind: true,
            removable: true,
        }
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
    /// A fixed-type row (`title` or a configured default) with a typed value input.
    Simple {
        widget: gtk::Widget,
        value: ValueWidget,
    },
}

impl BuiltRow {
    fn widget(&self) -> gtk::Widget {
        match self {
            Self::Expander { expander, .. } => expander.clone().upcast(),
            Self::Simple { widget, .. } => widget.clone(),
        }
    }
}

type Rows = Rc<RefCell<Vec<BuiltRow>>>;
type Drafts = Rc<RefCell<Vec<PropertyDraft>>>;

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

    let initial_format = request
        .document
        .as_ref()
        .map_or(FrontmatterFormat::Yaml, |document| document.format);
    let format_group = adw::PreferencesGroup::new();
    let format = adw::ComboRow::new();
    format.set_title(&gettext("Format"));
    format.set_model(Some(&format_model()));
    format.set_selected(match initial_format {
        FrontmatterFormat::Yaml => 0,
        FrontmatterFormat::Json => 1,
        FrontmatterFormat::Toml => 2,
    });
    format.set_widget_name("document-properties-format");
    format_group.add(&format);
    page.add(&format_group);

    let raw_mode = request.document.as_ref().is_some_and(|document| {
        document.error.is_some()
            || document
                .fields
                .iter()
                .any(|field| matches!(field.value, FrontmatterValue::Object(_)))
    });

    let (save_source, action_bar) = if raw_mode {
        let group = adw::PreferencesGroup::new();
        group.set_title(&gettext("Raw frontmatter"));
        let view = gtk::TextView::new();
        view.set_widget_name("document-properties-raw");
        view.set_wrap_mode(gtk::WrapMode::WordChar);
        view.set_monospace(true);
        view.buffer()
            .set_text(request.raw.as_deref().unwrap_or_default());
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
        (SaveSource::Raw(view), None)
    } else {
        let group = adw::PreferencesGroup::new();
        group.set_title(&gettext("Properties"));
        let drafts: Drafts = Rc::new(RefCell::new(initial_drafts(request)));
        let rows: Rows = Rc::new(RefCell::new(Vec::new()));
        let on_change: Rc<dyn Fn()> = Rc::new(|| {});
        rebuild(&group, &rows, &drafts, &on_change);
        page.add(&group);
        let bar = build_action_bar(&group, &rows, &drafts, &on_change, request);
        (SaveSource::Fields { rows, drafts }, Some(bar))
    };

    toolbar.set_content(Some(&page));
    if let Some(bar) = action_bar {
        toolbar.add_bottom_bar(&bar);
    }
    dialog.set_child(Some(&toolbar));

    let draft_cancel = dialog.clone();
    cancel.connect_clicked(move |_| {
        let _ = draft_cancel.close();
    });

    let session = request.session;
    let dispatcher = dispatcher.clone();
    let dialog_for_save = dialog.clone();
    save.connect_clicked(move |_| {
        let edit = match &save_source {
            SaveSource::Raw(view) => FrontmatterEdit::Raw {
                format: selected_format(&format),
                content: buffer_text(&view.buffer()),
            },
            SaveSource::Fields { rows, drafts } => {
                capture(rows, drafts);
                FrontmatterEdit::Parsed(build_document(selected_format(&format), &drafts.borrow()))
            }
        };
        let _ = dialog_for_save.close();
        let _ = dispatcher.dispatch(AppMsg::Editor(EditorMsg::ApplyFrontmatter {
            session,
            edit,
        }));
    });

    dialog.present(Some(parent));
}

/// What the per-note dialog reads when saving.
enum SaveSource {
    Raw(gtk::TextView),
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
    let dialog = adw::PreferencesDialog::new();
    dialog.set_title(&gettext("Default properties"));
    dialog.set_widget_name("document-properties-defaults-dialog");

    let page = adw::PreferencesPage::new();
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
    dialog.add(&page);

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
    rebuild(&group, &rows, &drafts, &on_change);
    let add_action = add_property_action(&group, &rows, &drafts, &on_change);
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
    dialog.upcast()
}

fn button_row(name: &str, title: &str) -> adw::ButtonRow {
    let row = adw::ButtonRow::new();
    row.set_title(title);
    row.set_start_icon_name(Some("list-add-symbolic"));
    row.set_widget_name(name);
    row
}

fn initial_drafts(request: &EditorPropertiesRequest) -> Vec<PropertyDraft> {
    let mut title: Option<FrontmatterValue> = None;
    let mut note_drafts = Vec::new();
    if let Some(document) = &request.document {
        for field in &document.fields {
            if field.key == TITLE_KEY && title.is_none() {
                title = Some(field.value.clone());
                continue;
            }
            let draft = request
                .defaults
                .iter()
                .find(|property| property.key == field.key)
                .map_or_else(
                    || PropertyDraft {
                        key: field.key.clone(),
                        choice: PropertyKindChoice::for_value(&field.value),
                        value: field.value.clone(),
                        editable_key: true,
                        editable_kind: true,
                        removable: true,
                    },
                    |property| {
                        PropertyDraft::default_row(field.key.clone(), property, field.value.clone())
                    },
                );
            note_drafts.push(draft);
        }
    }
    // Only the note's own properties are listed; configured defaults are added explicitly with
    // the "Add default properties" action so an untouched note is never changed on save.
    let mut catalog = vec![PropertyDraft {
        key: TITLE_KEY.to_owned(),
        choice: PropertyKindChoice::Text,
        value: title.unwrap_or(FrontmatterValue::Text(String::new())),
        editable_key: false,
        editable_kind: false,
        removable: false,
    }];
    catalog.extend(note_drafts);
    catalog
}

fn build_document(format: FrontmatterFormat, drafts: &[PropertyDraft]) -> FrontmatterDocument {
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

fn rebuild(group: &adw::PreferencesGroup, rows: &Rows, drafts: &Drafts, on_change: &Rc<dyn Fn()>) {
    {
        let built = rows.borrow();
        for row in built.iter() {
            group.remove(&row.widget());
        }
    }
    rows.borrow_mut().clear();
    let current = drafts.borrow().clone();
    for (index, draft) in current.iter().enumerate() {
        let row = build_row(draft, index, group, rows, drafts, on_change);
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
) -> BuiltRow {
    if draft.editable_kind {
        build_expander_row(draft, index, group, rows, drafts, on_change)
    } else {
        build_simple_row(draft, index, group, rows, drafts, on_change)
    }
}

/// Builds a fixed-type row: `title` and configured defaults use a plain typed value input.
fn build_simple_row(
    draft: &PropertyDraft,
    index: usize,
    group: &adw::PreferencesGroup,
    rows: &Rows,
    drafts: &Drafts,
    on_change: &Rc<dyn Fn()>,
) -> BuiltRow {
    let title = if draft.key == TITLE_KEY {
        gettext("Title")
    } else {
        draft.key.clone()
    };
    let value = ValueWidget::build(
        draft.choice,
        &draft.value,
        &format!("document-property-value-{index}"),
        &title,
    );
    let widget = value.widget();
    if draft.removable {
        let remove = remove_button(index, group, rows, drafts, on_change);
        value.add_suffix(&remove);
    }
    BuiltRow::Simple { widget, value }
}

// CONTEXT: The row keeps its widgets and its live callbacks together so the draft is the only
// shared state between the note editor and the defaults editor.
fn build_expander_row(
    draft: &PropertyDraft,
    index: usize,
    group: &adw::PreferencesGroup,
    rows: &Rows,
    drafts: &Drafts,
    on_change: &Rc<dyn Fn()>,
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
    kind.set_model(Some(&kind_model()));
    kind.set_selected(draft.choice.index());
    kind.set_widget_name(&format!("document-property-kind-{index}"));

    let value = ValueWidget::build(
        draft.choice,
        &draft.value,
        &format!("document-property-value-{index}"),
        &gettext("Value"),
    );

    expander.add_row(&key);
    expander.add_row(&kind);
    expander.add_row(&value.widget());

    let refresh: Rc<dyn Fn()> = {
        let expander = expander.clone();
        let key = key.clone();
        let kind = kind.clone();
        let value = value.clone();
        Rc::new(move || {
            let choice = PropertyKindChoice::from_index(kind.selected());
            let key_text = key.text().to_string();
            expander.set_title(&if key_text.trim().is_empty() {
                gettext("New property")
            } else {
                key_text
            });
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
            let choice = PropertyKindChoice::from_index(dropdown.selected());
            let group = group.clone();
            let rows = Rc::clone(&rows);
            let drafts = Rc::clone(&drafts);
            let on_change = Rc::clone(&on_change);
            glib::idle_add_local_once(move || {
                capture(&rows, &drafts);
                if let Some(draft) = drafts.borrow_mut().get_mut(index) {
                    draft.choice = choice;
                }
                rebuild(&group, &rows, &drafts, &on_change);
                on_change();
            });
        });
    }

    if draft.removable {
        let remove = remove_button(index, group, rows, drafts, on_change);
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
            rebuild(&group, &rows, &drafts, &on_change);
            on_change();
        });
    });
    remove
}

fn build_action_bar(
    group: &adw::PreferencesGroup,
    rows: &Rows,
    drafts: &Drafts,
    on_change: &Rc<dyn Fn()>,
    request: &EditorPropertiesRequest,
) -> gtk::Box {
    let bar = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    bar.set_widget_name("document-properties-actions");
    bar.set_margin_start(12);
    bar.set_margin_end(12);
    bar.set_margin_top(6);
    bar.set_margin_bottom(6);
    bar.set_halign(gtk::Align::Start);

    let add = action_button("document-properties-add", &gettext("Add property"));
    let add_action = add_property_action(group, rows, drafts, on_change);
    add.connect_clicked(move |_| add_action());
    bar.append(&add);

    // The defaults are only offered when the note carries no properties of its own; notes that
    // already have a frontmatter block keep exactly the properties they authored.
    let has_fields = request
        .document
        .as_ref()
        .is_some_and(|document| !document.fields.is_empty());
    if request.defaults_enabled && !request.defaults.is_empty() && !has_fields {
        let add_defaults = action_button(
            "document-properties-add-defaults",
            &gettext("Add default properties"),
        );
        let defaults_action =
            add_defaults_action(group, rows, drafts, on_change, &request.defaults);
        add_defaults.connect_clicked(move |_| defaults_action());
        bar.append(&add_defaults);
    }
    bar
}

fn action_button(name: &str, label: &str) -> gtk::Button {
    let button = gtk::Button::new();
    button.set_widget_name(name);
    let content = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    content.append(&gtk::Image::from_icon_name("list-add-symbolic"));
    content.append(&gtk::Label::new(Some(label)));
    button.set_child(Some(&content));
    button
}

fn add_property_action(
    group: &adw::PreferencesGroup,
    rows: &Rows,
    drafts: &Drafts,
    on_change: &Rc<dyn Fn()>,
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
            rebuild(&group, &rows, &drafts, &on_change);
            on_change();
        });
    })
}

fn add_defaults_action(
    group: &adw::PreferencesGroup,
    rows: &Rows,
    drafts: &Drafts,
    on_change: &Rc<dyn Fn()>,
    defaults: &[DocumentProperty],
) -> Rc<dyn Fn()> {
    let group = group.clone();
    let rows = Rc::clone(rows);
    let drafts = Rc::clone(drafts);
    let on_change = Rc::clone(on_change);
    let defaults = defaults.to_vec();
    Rc::new(move || {
        let group = group.clone();
        let rows = Rc::clone(&rows);
        let drafts = Rc::clone(&drafts);
        let on_change = Rc::clone(&on_change);
        let defaults = defaults.clone();
        glib::idle_add_local_once(move || {
            capture(&rows, &drafts);
            for property in &defaults {
                let present = drafts
                    .borrow()
                    .iter()
                    .any(|draft| draft.key == property.key);
                if property.key == TITLE_KEY || present {
                    continue;
                }
                drafts.borrow_mut().push(PropertyDraft::default_row(
                    property.key.clone(),
                    property,
                    FrontmatterValue::from_json(&property.value),
                ));
            }
            rebuild(&group, &rows, &drafts, &on_change);
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
                let choice = PropertyKindChoice::from_index(kind.selected());
                draft.key = key.text().to_string();
                draft.choice = choice;
                let value = value.frontmatter_preserving(choice, &draft.value);
                draft.value = value;
            }
            BuiltRow::Simple { value, .. } => {
                // The key and type are fixed; only the value is read back.
                let next = value.frontmatter_preserving(draft.choice, &draft.value);
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
        let kind = draft.choice.property_kind();
        let multiline = draft.choice.is_multiline();
        normalized.push(DocumentProperty {
            key,
            kind,
            multiline,
            value: normalized_default_value(kind, &draft.value.to_json()),
        });
    }
    normalized
}

fn normalized_default_value(kind: PropertyKind, value: &serde_json::Value) -> serde_json::Value {
    let matches = value.is_null()
        || match kind {
            PropertyKind::Text => value.is_string(),
            PropertyKind::Number => value.is_number(),
            PropertyKind::Boolean => value.is_boolean(),
            PropertyKind::List => value.is_array(),
            PropertyKind::Null | PropertyKind::Mixed => false,
        };
    if matches {
        return value.clone();
    }
    match kind {
        PropertyKind::Number => serde_json::Value::from(0),
        PropertyKind::Boolean => serde_json::Value::Bool(false),
        PropertyKind::List => serde_json::Value::Array(Vec::new()),
        _ => serde_json::Value::String(String::new()),
    }
}

fn row_subtitle(choice: PropertyKindChoice, value: &FrontmatterValue) -> String {
    let preview = preview_text(value);
    if preview.is_empty() {
        choice.label()
    } else {
        format!("{} · {preview}", choice.label())
    }
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

fn frontmatter_text(value: &FrontmatterValue) -> String {
    match value {
        FrontmatterValue::Text(text) => text.clone(),
        FrontmatterValue::Number(number) => number.to_string(),
        FrontmatterValue::Boolean(flag) => flag.to_string(),
        FrontmatterValue::Null => String::new(),
        FrontmatterValue::Object(_) | FrontmatterValue::List(_) => value.to_json().to_string(),
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

#[cfg(test)]
mod tests;
