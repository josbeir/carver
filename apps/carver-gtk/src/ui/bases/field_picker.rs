//! Searchable field controls shared by Base columns, filters, and sorts.

use std::{cell::RefCell, collections::BTreeMap, rc::Rc};

use carver_sdk::{BaseColumn, BaseDefinition, PropertyDescriptor, PropertyKind, PropertyPath};
use gtk::prelude::*;
use libadwaita::{self as adw, prelude::*};

type PopulateFn = Rc<dyn Fn(&str)>;
type PopulateSlot = Rc<RefCell<Option<PopulateFn>>>;

#[derive(Clone, Copy, Debug)]
pub(crate) struct FieldPickerOptions<'a> {
    pub(crate) keep_open: bool,
    pub(crate) button_label: Option<&'a str>,
    pub(crate) button_icon_name: Option<&'a str>,
}

/// One field offered by a Base configuration picker.
#[derive(Clone, Debug)]
pub(crate) struct FieldOption {
    /// Stable field value used when saving configuration.
    pub(crate) field: BaseColumn,
    /// Human-readable label shown in the picker.
    pub(crate) label: String,
    /// Searchable aliases including the canonical path and leaf key.
    pub(crate) search: String,
    /// Compact type and example context.
    pub(crate) metadata: String,
}

/// Shared catalog of built-in, discovered, and locally added fields.
#[derive(Clone, Debug)]
pub(crate) struct FieldCatalog {
    options: Rc<RefCell<Vec<FieldOption>>>,
}

impl FieldCatalog {
    /// Builds a catalog from the current definition and library-wide descriptors.
    pub(crate) fn new(definition: &BaseDefinition, descriptors: &[PropertyDescriptor]) -> Self {
        let mut options = BTreeMap::new();
        for field in [BaseColumn::Name, BaseColumn::Category, BaseColumn::Updated] {
            options.insert(field_id(&field), option_for_field(&field, None));
        }
        for descriptor in descriptors {
            let field = BaseColumn::Property(descriptor.path.clone());
            options.insert(field_id(&field), option_for_field(&field, Some(descriptor)));
        }
        for field in definition
            .columns
            .iter()
            .chain(definition.filters.iter().map(|filter| &filter.field))
            .chain(definition.sorts.iter().map(|sort| &sort.field))
        {
            options
                .entry(field_id(field))
                .or_insert_with(|| option_for_field(field, None));
        }
        Self {
            options: Rc::new(RefCell::new(options.into_values().collect())),
        }
    }

    /// Returns a snapshot for rendering a picker.
    pub(crate) fn options(&self) -> Vec<FieldOption> {
        self.options.borrow().clone()
    }

    /// Adds a locally entered custom field if it is not already present.
    pub(crate) fn ensure(&self, field: &BaseColumn) {
        let id = field_id(field);
        let exists = self
            .options
            .borrow()
            .iter()
            .any(|option| field_id(&option.field) == id);
        if !exists {
            let mut options = self.options.borrow_mut();
            options.push(option_for_field(field, None));
            options.sort_by_key(|option| field_id(&option.field));
        }
    }
}

/// A single searchable field button and its local selected value.
pub(crate) struct FieldPicker {
    /// Button rendered in a rule row.
    pub(crate) button: gtk::Button,
    selected: Rc<RefCell<BaseColumn>>,
}

impl FieldPicker {
    /// Creates a picker. `keep_open` is used by the multi-select visible-column editor.
    pub(crate) fn new(
        catalog: &FieldCatalog,
        initial: &BaseColumn,
        widget_name: &str,
        keep_open: bool,
        button_label: Option<&str>,
        on_selected: impl Fn(BaseColumn) + 'static,
    ) -> Self {
        let current = initial.clone();
        let is_selected: Rc<dyn Fn(&BaseColumn) -> bool> =
            Rc::new(move |field: &BaseColumn| field == &current);
        Self::new_with_selection(
            catalog,
            initial,
            widget_name,
            FieldPickerOptions {
                keep_open,
                button_label,
                button_icon_name: None,
            },
            &is_selected,
            on_selected,
        )
    }

    /// Creates a picker with a marker for fields selected by the initiating editor.
    #[expect(
        clippy::too_many_lines,
        reason = "Picker setup keeps its widget ownership and signal wiring together"
    )]
    pub(crate) fn new_with_selection(
        catalog: &FieldCatalog,
        initial: &BaseColumn,
        widget_name: &str,
        options: FieldPickerOptions<'_>,
        is_selected: &Rc<dyn Fn(&BaseColumn) -> bool>,
        on_selected: impl Fn(BaseColumn) + 'static,
    ) -> Self {
        catalog.ensure(initial);
        let selected = Rc::new(RefCell::new(initial.clone()));
        let initial_label = field_label(initial);
        let action_label = options.button_label.map(ToOwned::to_owned);
        let button = options.button_icon_name.map_or_else(
            || gtk::Button::with_label(action_label.as_deref().unwrap_or(&initial_label)),
            gtk::Button::from_icon_name,
        );
        button.set_widget_name(widget_name);
        button.add_css_class("field-picker");
        let initial_tooltip = picker_tooltip(action_label.as_deref(), &initial_label);
        button.set_tooltip_text(Some(&initial_tooltip));
        update_accessibility_with_label(&button, initial, catalog, action_label.as_deref());

        let popover = gtk::Popover::new();
        popover.set_has_arrow(true);
        popover.set_autohide(true);
        popover.set_parent(&button);
        let root = gtk::Box::new(gtk::Orientation::Vertical, 8);
        root.set_margin_start(12);
        root.set_margin_end(12);
        root.set_margin_top(12);
        root.set_margin_bottom(12);
        let search = gtk::SearchEntry::new();
        search.set_placeholder_text(Some("Search fields"));
        search.set_widget_name(&format!("{widget_name}-search"));
        search.set_height_request(42);
        root.append(&search);
        let list = gtk::ListBox::new();
        list.set_selection_mode(gtk::SelectionMode::None);
        list.add_css_class("boxed-list");
        let scroll = gtk::ScrolledWindow::new();
        scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
        scroll.set_min_content_width(360);
        scroll.set_max_content_width(500);
        scroll.set_min_content_height(96);
        scroll.set_max_content_height(360);
        scroll.set_child(Some(&list));
        root.append(&scroll);
        let custom = gtk::Button::with_label("Add custom field…");
        custom.add_css_class("flat");
        custom.set_halign(gtk::Align::Start);
        root.append(&custom);
        popover.set_child(Some(&root));

        let selected_for_render = Rc::clone(&selected);
        let button_for_render = button.clone();
        let catalog_for_render = catalog.clone();
        let popover_for_selection = popover.clone();
        let search_for_selection = search.clone();
        let populate_slot: PopulateSlot = Rc::new(RefCell::new(None));
        let populate_slot_for_selection = Rc::downgrade(&populate_slot);
        let button_label = action_label.clone();
        let button_icon_name = options.button_icon_name.map(ToOwned::to_owned);
        let keep_open = options.keep_open;
        let callback: Rc<dyn Fn(BaseColumn)> = Rc::new(move |field: BaseColumn| {
            *selected_for_render.borrow_mut() = field.clone();
            if let Some(icon_name) = button_icon_name.as_deref() {
                button_for_render.set_icon_name(icon_name);
            } else {
                button_for_render
                    .set_label(button_label.as_deref().unwrap_or(&field_label(&field)));
            }
            let tooltip = picker_tooltip(button_label.as_deref(), &field_label(&field));
            button_for_render.set_tooltip_text(Some(&tooltip));
            update_accessibility_with_label(
                &button_for_render,
                &field,
                &catalog_for_render,
                button_label.as_deref(),
            );
            on_selected(field);
            if keep_open {
                let populate = populate_slot_for_selection
                    .upgrade()
                    .and_then(|slot| slot.borrow().clone());
                if let Some(populate) = populate {
                    populate(search_for_selection.text().as_str());
                }
            } else {
                popover_for_selection.popdown();
            }
        });
        let populate: Rc<dyn Fn(&str)> = {
            let list = list.clone();
            let catalog = catalog.clone();
            let selected = Rc::clone(&selected);
            let is_selected = Rc::clone(is_selected);
            let callback = Rc::clone(&callback);
            Rc::new(move |query: &str| {
                populate_options(
                    &list,
                    &catalog,
                    &selected.borrow(),
                    query,
                    &callback,
                    &is_selected,
                );
            })
        };
        *populate_slot.borrow_mut() = Some(Rc::clone(&populate));
        populate("");
        {
            let populate = Rc::clone(&populate);
            search.connect_search_changed(move |search| populate(search.text().as_str()));
        }
        {
            let button = button.clone();
            let popover = popover.clone();
            let search = search.clone();
            let populate = Rc::clone(&populate);
            button.connect_clicked(move |_| {
                populate(search.text().as_str());
                popover.popup();
                search.grab_focus();
            });
        }
        {
            let button = button.clone();
            popover.connect_closed(move |_| {
                button.grab_focus();
            });
        }
        {
            let catalog = catalog.clone();
            let button = button.clone();
            let callback = Rc::clone(&callback);
            let populate = Rc::clone(&populate);
            let search = search.clone();
            custom.connect_clicked(move |_| {
                show_custom_field_dialog(&button, &catalog, &callback, &populate, &search);
            });
        }
        Self { button, selected }
    }

    /// Returns the current field selected by the user.
    pub(crate) fn selected(&self) -> BaseColumn {
        self.selected.borrow().clone()
    }
}

fn populate_options(
    list: &gtk::ListBox,
    catalog: &FieldCatalog,
    selected: &BaseColumn,
    query: &str,
    callback: &Rc<dyn Fn(BaseColumn)>,
    is_selected: &Rc<dyn Fn(&BaseColumn) -> bool>,
) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }
    let query = query.trim().to_lowercase();
    let mut matches = 0;
    for option in catalog.options() {
        if !matches_query(&option, &query) {
            continue;
        }
        matches += 1;
        let row = field_option_row(&option, selected, callback, is_selected);
        list.append(&row);
    }
    if matches == 0 {
        let label = gtk::Label::new(Some("No matching fields"));
        label.add_css_class("dim-label");
        label.set_margin_top(10);
        label.set_margin_bottom(10);
        list.append(&label);
    }
}

fn matches_query(option: &FieldOption, query: &str) -> bool {
    query.is_empty() || option.search.contains(query)
}

fn field_option_row(
    option: &FieldOption,
    selected: &BaseColumn,
    callback: &Rc<dyn Fn(BaseColumn)>,
    is_selected: &Rc<dyn Fn(&BaseColumn) -> bool>,
) -> gtk::ListBoxRow {
    let row = gtk::ListBoxRow::new();
    let button = gtk::Button::new();
    button.add_css_class("flat");
    button.set_hexpand(true);
    button.update_property(&[
        gtk::accessible::Property::Label(&option.label),
        gtk::accessible::Property::Description(&option.metadata),
    ]);
    let content = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    content.set_margin_start(8);
    content.set_margin_end(8);
    content.set_margin_top(6);
    content.set_margin_bottom(6);
    let labels = gtk::Box::new(gtk::Orientation::Vertical, 2);
    labels.set_hexpand(true);
    let label = gtk::Label::new(Some(&option.label));
    label.set_xalign(0.0);
    label.set_wrap(true);
    labels.append(&label);
    let metadata = gtk::Label::new(Some(&option.metadata));
    metadata.set_xalign(0.0);
    metadata.set_wrap(true);
    metadata.add_css_class("dim-label");
    labels.append(&metadata);
    content.append(&labels);
    if is_selected(&option.field) || &option.field == selected {
        let selected_label = gtk::Label::new(Some("Selected"));
        selected_label.add_css_class("dim-label");
        content.append(&selected_label);
    }
    button.set_child(Some(&content));
    let field = option.field.clone();
    let callback = Rc::clone(callback);
    button.connect_clicked(move |_| callback(field.clone()));
    row.set_child(Some(&button));
    row
}

fn show_custom_field_dialog(
    anchor: &gtk::Button,
    catalog: &FieldCatalog,
    callback: &Rc<dyn Fn(BaseColumn)>,
    populate: &Rc<dyn Fn(&str)>,
    search: &gtk::SearchEntry,
) {
    let Some(parent) = anchor.root().and_downcast::<gtk::Window>() else {
        return;
    };
    let entry = gtk::Entry::builder()
        .placeholder_text("/project/status")
        .activates_default(true)
        .build();
    entry.set_widget_name("base-custom-field-entry");
    let hint = gtk::Label::new(Some("Use a slash-prefixed JSON Pointer for nested fields."));
    hint.set_wrap(true);
    hint.add_css_class("dim-label");
    let error = gtk::Label::new(Some("Enter a valid path such as /project/status."));
    error.set_xalign(0.0);
    error.set_wrap(true);
    error.add_css_class("error");
    error.set_visible(false);
    let extra = gtk::Box::new(gtk::Orientation::Vertical, 6);
    extra.append(&entry);
    extra.append(&hint);
    extra.append(&error);
    let dialog = adw::AlertDialog::builder()
        .heading("Add custom field")
        .extra_child(&extra)
        .default_response("add")
        .close_response("cancel")
        .build();
    dialog.add_responses(&[("cancel", "Cancel"), ("add", "Add")]);
    dialog.set_response_enabled("add", false);
    {
        let dialog = dialog.clone();
        let error = error.clone();
        entry.connect_changed(move |entry| {
            let valid = valid_json_pointer(entry.text().as_str());
            dialog.set_response_enabled("add", valid);
            error.set_visible(!entry.text().is_empty() && !valid);
        });
    }
    let catalog = catalog.clone();
    let callback = Rc::clone(callback);
    let populate = Rc::clone(populate);
    let search = search.clone();
    let entry_for_response = entry.clone();
    dialog.connect_response(None, move |_dialog, response| {
        if response == "add" {
            let field =
                BaseColumn::Property(PropertyPath(entry_for_response.text().trim().to_owned()));
            catalog.ensure(&field);
            callback(field);
            populate(search.text().as_str());
        }
    });
    dialog.present(Some(&parent));
    entry.grab_focus();
}

fn valid_json_pointer(path: &str) -> bool {
    let path = path.trim();
    let mut chars = path.chars();
    if chars.next() != Some('/') {
        return false;
    }
    while let Some(character) = chars.next() {
        if character == '\0' {
            return false;
        }
        if character == '~' && !matches!(chars.next(), Some('0' | '1')) {
            return false;
        }
    }
    true
}

fn option_for_field(field: &BaseColumn, descriptor: Option<&PropertyDescriptor>) -> FieldOption {
    let label = field_label(field);
    let metadata = match field {
        BaseColumn::Name | BaseColumn::Category | BaseColumn::Updated => {
            "Built-in field".to_owned()
        }
        BaseColumn::Property(_) => descriptor.map_or_else(
            || "Custom field".to_owned(),
            |descriptor| {
                let kind = property_kind_label(descriptor.kind);
                descriptor.example.as_deref().map_or_else(
                    || kind.to_owned(),
                    |example| format!("{kind} · {}", truncate(example, 48)),
                )
            },
        ),
    };
    let path = match field {
        BaseColumn::Property(path) => path.0.clone(),
        _ => String::new(),
    };
    let leaf = label.rsplit(" → ").next().unwrap_or(&label).to_owned();
    FieldOption {
        field: field.clone(),
        search: format!(
            "{} {} {}",
            label.to_lowercase(),
            path.to_lowercase(),
            leaf.to_lowercase()
        ),
        label,
        metadata,
    }
}

fn update_accessibility_with_label(
    button: &gtk::Button,
    field: &BaseColumn,
    catalog: &FieldCatalog,
    label_override: Option<&str>,
) {
    let metadata = catalog
        .options()
        .into_iter()
        .find(|option| option.field == *field)
        .map_or_else(|| "Field".to_owned(), |option| option.metadata);
    let label = picker_accessible_label(label_override, field);
    button.update_property(&[
        gtk::accessible::Property::Label(&label),
        gtk::accessible::Property::Description(&metadata),
    ]);
}

fn picker_accessible_label(label_override: Option<&str>, field: &BaseColumn) -> String {
    label_override.map_or_else(|| field_label(field), ToOwned::to_owned)
}

fn picker_tooltip(label_override: Option<&str>, field_label: &str) -> String {
    label_override.map_or_else(|| field_label.to_owned(), ToOwned::to_owned)
}

pub(crate) fn field_id(field: &BaseColumn) -> String {
    match field {
        BaseColumn::Name => "name".to_owned(),
        BaseColumn::Category => "category".to_owned(),
        BaseColumn::Updated => "updated".to_owned(),
        BaseColumn::Property(path) => format!("property:{}", path.0),
    }
}

pub(crate) fn field_label(field: &BaseColumn) -> String {
    match field {
        BaseColumn::Name => "Name".to_owned(),
        BaseColumn::Category => "Category".to_owned(),
        BaseColumn::Updated => "Updated".to_owned(),
        BaseColumn::Property(path) => path
            .0
            .trim_start_matches('/')
            .split('/')
            .map(|segment| segment.replace("~1", "/").replace("~0", "~"))
            .collect::<Vec<_>>()
            .join(" → "),
    }
}

fn property_kind_label(kind: PropertyKind) -> &'static str {
    match kind {
        PropertyKind::Text => "Text",
        PropertyKind::Number => "Number",
        PropertyKind::Boolean => "Boolean",
        PropertyKind::List => "List",
        PropertyKind::Null => "Empty",
        PropertyKind::Mixed => "Mixed",
    }
}

fn truncate(value: &str, limit: usize) -> String {
    let mut output = value.chars().take(limit).collect::<String>();
    if value.chars().nth(limit).is_some() {
        output.push('…');
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn field_search_should_match_nested_paths_and_leaf_names() {
        let field = BaseColumn::Property(PropertyPath("/project/status".to_owned()));
        let option = option_for_field(&field, None);

        assert!(matches_query(&option, "/project/status"));
        assert!(matches_query(&option, "status"));
        assert!(!matches_query(&option, "owner"));
    }

    #[test]
    fn custom_json_pointer_should_require_valid_tokens() {
        assert!(valid_json_pointer(" /project/status "));
        assert!(valid_json_pointer("/project/~0name~1value"));
        assert!(valid_json_pointer("/"));
        assert!(!valid_json_pointer("project/status"));
        assert!(!valid_json_pointer("/project/~2status"));
        assert!(!valid_json_pointer("/project/\0status"));
        assert!(!valid_json_pointer(""));
    }

    #[test]
    fn field_label_should_unescape_nested_json_pointer_tokens() {
        let field = BaseColumn::Property(PropertyPath("/project/a~1b/~0name".to_owned()));
        assert_eq!(field_label(&field), "project → a/b → ~name");
    }

    #[test]
    fn catalog_should_keep_configured_fields_that_are_not_discovered() {
        let configured = BaseColumn::Property(PropertyPath("/configured".to_owned()));
        let definition = BaseDefinition::defaults(
            carver_sdk::BaseId::new(),
            "Projects".to_owned(),
            vec![configured.clone()],
            carver_sdk::Revision(1),
        );
        let descriptor = PropertyDescriptor {
            path: PropertyPath("/status".to_owned()),
            kind: PropertyKind::Text,
            example: Some("ready".to_owned()),
        };
        let catalog = FieldCatalog::new(&definition, &[descriptor]);
        let options = catalog.options();
        assert_eq!(
            options
                .iter()
                .find(|option| option.field == configured)
                .map(|option| option.metadata.as_str()),
            Some("Custom field")
        );
        assert_eq!(
            options
                .iter()
                .find(|option| matches!(option.field, BaseColumn::Property(ref path) if path.0 == "/status"))
                .map(|option| option.metadata.as_str()),
            Some("Text · ready")
        );
    }

    #[test]
    fn truncate_should_append_an_ellipsis_only_when_needed() {
        assert_eq!(truncate("short", 8), "short");
        assert_eq!(truncate("abcdefgh", 8), "abcdefgh");
        assert_eq!(truncate("abcdefghijk", 8), "abcdefgh…");
    }
}
