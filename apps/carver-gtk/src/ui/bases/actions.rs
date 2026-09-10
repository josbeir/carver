//! Header actions and configuration for the current Base.
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use crate::mvu::{AppDispatcher, AppMsg, BasesMsg};
use carver_sdk::{
    BaseColumn, BaseDefinition, BaseFilter, BaseFilterMode, BaseFilterOperator, BaseSort,
    BaseSortDirection,
};
use gtk::prelude::*;
use libadwaita::{self as adw, prelude::*};

use super::field_picker::{FieldCatalog, FieldPicker, field_label};

pub(crate) fn render_delete(
    button: &gtk::Button,
    base: Option<&BaseDefinition>,
    dispatcher: &AppDispatcher,
) {
    let group = gtk::gio::SimpleActionGroup::new();
    let action = gtk::gio::SimpleAction::new("delete", None);
    action.set_enabled(base.is_some());
    if let Some(base) = base {
        let id = base.id;
        let name = base.name.clone();
        let dispatcher = dispatcher.clone();
        let weak_button = button.downgrade();
        action.connect_activate(move |_, _| {
            let Some(parent) = weak_button.upgrade().and_then(|button| button.root()).and_downcast::<gtk::Window>() else { return; };
            let dialog = adw::AlertDialog::builder().heading(format!("Delete “{name}”?"))
                .body("Only this Base will be deleted. Your notes and their properties will not be changed.")
                .close_response("cancel").default_response("cancel").build();
            dialog.set_widget_name("delete-base-confirmation");
            dialog.add_responses(&[("cancel", "Cancel"), ("delete", "Delete Base")]);
            dialog.set_response_appearance("delete", adw::ResponseAppearance::Destructive);
            let dispatcher = dispatcher.clone();
            dialog.connect_response(None, move |_, response| {
                if response == "delete" { let _ = dispatcher.dispatch(AppMsg::Bases(BasesMsg::Delete(id))); }
            });
            dialog.present(Some(&parent));
        });
    }
    group.add_action(&action);
    button.insert_action_group("base", Some(&group));
}

#[expect(
    deprecated,
    reason = "ComboBoxText remains supported by the minimum GTK runtime"
)]
struct FilterWidgets {
    id: u64,
    row: gtk::Box,
    field: FieldPicker,
    operator: gtk::ComboBoxText,
    value: gtk::Entry,
}

#[expect(
    deprecated,
    reason = "ComboBoxText remains supported by the minimum GTK runtime"
)]
struct SortWidgets {
    id: u64,
    row: gtk::Box,
    field: FieldPicker,
    direction: gtk::ComboBoxText,
}

trait RuleWidgets {
    fn id(&self) -> u64;
    fn row(&self) -> &gtk::Box;
}

impl RuleWidgets for FilterWidgets {
    fn id(&self) -> u64 {
        self.id
    }

    fn row(&self) -> &gtk::Box {
        &self.row
    }
}

impl RuleWidgets for SortWidgets {
    fn id(&self) -> u64 {
        self.id
    }

    fn row(&self) -> &gtk::Box {
        &self.row
    }
}

fn rebuild_rule_rows<T: RuleWidgets>(container: &gtk::Box, rows: &[T], add_button: &gtk::Button) {
    while let Some(child) = container.first_child() {
        container.remove(&child);
    }
    for rule in rows {
        container.append(rule.row());
    }
    container.append(add_button);
}

fn reorder_rule<T: RuleWidgets>(
    rows: &Rc<RefCell<Vec<T>>>,
    container: &gtk::Box,
    add_button: &gtk::Button,
    id: u64,
    offset: isize,
) {
    {
        let mut rows = rows.borrow_mut();
        let Some(index) = rows.iter().position(|rule| rule.id() == id) else {
            return;
        };
        let target = match offset {
            -1 => index.checked_sub(1),
            1 => index.checked_add(1).filter(|target| *target < rows.len()),
            _ => None,
        };
        let Some(target) = target else {
            return;
        };
        rows.swap(index, target);
    }
    rebuild_rule_rows(container, &rows.borrow(), add_button);
}

fn remove_rule<T: RuleWidgets>(
    rows: &Rc<RefCell<Vec<T>>>,
    container: &gtk::Box,
    add_button: &gtk::Button,
    id: u64,
) {
    let removed = {
        let mut rows = rows.borrow_mut();
        rows.iter()
            .position(|rule| rule.id() == id)
            .map(|index| rows.remove(index))
    };
    if removed.is_some() {
        rebuild_rule_rows(container, &rows.borrow(), add_button);
    }
}

fn rule_button(icon_name: &str, tooltip: &str) -> gtk::Button {
    let button = gtk::Button::from_icon_name(icon_name);
    button.set_tooltip_text(Some(tooltip));
    button.add_css_class("flat");
    button
}

fn append_rule_controls<T: RuleWidgets + 'static>(
    row: &gtk::Box,
    noun: &'static str,
    id: u64,
    container: &gtk::Box,
    rows: &Rc<RefCell<Vec<T>>>,
    add_button: &gtk::Button,
    refresh_preview: Option<&Rc<dyn Fn()>>,
) {
    let controls = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    let up = rule_button("go-up-symbolic", &format!("Move {noun} up"));
    let down = rule_button("go-down-symbolic", &format!("Move {noun} down"));
    let remove = rule_button("list-remove-symbolic", &format!("Remove {noun}"));
    let prefix = noun.replace(' ', "-");
    up.set_widget_name(&format!("base-rule-{prefix}-up-{id}"));
    down.set_widget_name(&format!("base-rule-{prefix}-down-{id}"));
    remove.set_widget_name(&format!("base-rule-{prefix}-remove-{id}"));
    {
        let rows = Rc::clone(rows);
        let container = container.clone();
        let add_button = add_button.clone();
        let refresh_preview = refresh_preview.map(Rc::downgrade);
        up.connect_clicked(move |_| {
            reorder_rule(&rows, &container, &add_button, id, -1);
            if let Some(refresh_preview) = refresh_preview.as_ref().and_then(std::rc::Weak::upgrade)
            {
                refresh_preview();
            }
        });
    }
    {
        let rows = Rc::clone(rows);
        let container = container.clone();
        let add_button = add_button.clone();
        let refresh_preview = refresh_preview.map(Rc::downgrade);
        down.connect_clicked(move |_| {
            reorder_rule(&rows, &container, &add_button, id, 1);
            if let Some(refresh_preview) = refresh_preview.as_ref().and_then(std::rc::Weak::upgrade)
            {
                refresh_preview();
            }
        });
    }
    {
        let rows = Rc::clone(rows);
        let container = container.clone();
        let add_button = add_button.clone();
        let refresh_preview = refresh_preview.map(Rc::downgrade);
        remove.connect_clicked(move |_| {
            remove_rule(&rows, &container, &add_button, id);
            if let Some(refresh_preview) = refresh_preview.as_ref().and_then(std::rc::Weak::upgrade)
            {
                refresh_preview();
            }
        });
    }
    controls.append(&up);
    controls.append(&down);
    controls.append(&remove);
    row.append(&controls);
}

fn allocate_rule_id(counter: &Cell<u64>) -> u64 {
    let id = counter.get();
    counter.set(id.saturating_add(1));
    id
}

#[expect(
    deprecated,
    reason = "ComboBoxText remains supported by the minimum GTK runtime"
)]
fn selected_filter_mode(combo: &gtk::ComboBoxText) -> BaseFilterMode {
    if combo.active_id().as_deref() == Some("any") {
        BaseFilterMode::Any
    } else {
        BaseFilterMode::All
    }
}

fn selected_filters(widgets: &[FilterWidgets]) -> Vec<BaseFilter> {
    widgets
        .iter()
        .map(|widgets| BaseFilter {
            field: widgets.field.selected(),
            operator: selected_operator(&widgets.operator),
            value: value_from_text(&widgets.value.text()),
        })
        .filter(|filter| {
            matches!(
                filter.operator,
                BaseFilterOperator::IsPresent | BaseFilterOperator::IsMissing
            ) || filter.value.is_some()
        })
        .collect()
}

#[expect(
    deprecated,
    reason = "ComboBoxText remains supported by the minimum GTK runtime"
)]
fn selected_sorts(widgets: &[SortWidgets]) -> Vec<BaseSort> {
    widgets
        .iter()
        .map(|widgets| BaseSort {
            field: widgets.field.selected(),
            direction: if widgets.direction.active_id().as_deref() == Some("ascending") {
                BaseSortDirection::Ascending
            } else {
                BaseSortDirection::Descending
            },
        })
        .collect()
}

#[expect(
    deprecated,
    reason = "ComboBoxText remains supported by the minimum GTK runtime"
)]
fn connect_filter_preview(widgets: &FilterWidgets, refresh_preview: &Rc<dyn Fn()>) {
    let operator_refresh = Rc::downgrade(refresh_preview);
    widgets.operator.connect_changed(move |_| {
        if let Some(refresh_preview) = operator_refresh.upgrade() {
            refresh_preview();
        }
    });
    let value_refresh = Rc::downgrade(refresh_preview);
    widgets.value.connect_changed(move |_| {
        if let Some(refresh_preview) = value_refresh.upgrade() {
            refresh_preview();
        }
    });
}

fn operator_id(operator: BaseFilterOperator) -> &'static str {
    match operator {
        BaseFilterOperator::Equals => "equals",
        BaseFilterOperator::NotEquals => "not-equals",
        BaseFilterOperator::Contains => "contains",
        BaseFilterOperator::StartsWith => "starts-with",
        BaseFilterOperator::GreaterThan => "greater-than",
        BaseFilterOperator::LessThan => "less-than",
        BaseFilterOperator::GreaterOrEqual => "greater-or-equal",
        BaseFilterOperator::LessOrEqual => "less-or-equal",
        BaseFilterOperator::IsPresent => "present",
        BaseFilterOperator::IsMissing => "missing",
        BaseFilterOperator::ListContains => "list-contains",
        BaseFilterOperator::ListNotContains => "list-not-contains",
    }
}

#[expect(
    deprecated,
    reason = "ComboBoxText remains supported by the minimum GTK runtime"
)]
fn selected_operator(combo: &gtk::ComboBoxText) -> BaseFilterOperator {
    match combo.active_id().as_deref() {
        Some("not-equals") => BaseFilterOperator::NotEquals,
        Some("contains") => BaseFilterOperator::Contains,
        Some("starts-with") => BaseFilterOperator::StartsWith,
        Some("greater-than") => BaseFilterOperator::GreaterThan,
        Some("less-than") => BaseFilterOperator::LessThan,
        Some("greater-or-equal") => BaseFilterOperator::GreaterOrEqual,
        Some("less-or-equal") => BaseFilterOperator::LessOrEqual,
        Some("present") => BaseFilterOperator::IsPresent,
        Some("missing") => BaseFilterOperator::IsMissing,
        Some("list-contains") => BaseFilterOperator::ListContains,
        Some("list-not-contains") => BaseFilterOperator::ListNotContains,
        _ => BaseFilterOperator::Equals,
    }
}

#[expect(
    deprecated,
    reason = "ComboBoxText remains supported by the minimum GTK runtime"
)]
fn add_operator_options(combo: &gtk::ComboBoxText) {
    for (id, label) in [
        ("equals", "is"),
        ("not-equals", "is not"),
        ("contains", "contains"),
        ("starts-with", "starts with"),
        ("greater-than", ">"),
        ("less-than", "<"),
        ("greater-or-equal", "≥"),
        ("less-or-equal", "≤"),
        ("present", "is present"),
        ("missing", "is missing"),
        ("list-contains", "list contains"),
        ("list-not-contains", "list does not contain"),
    ] {
        combo.append(Some(id), label);
    }
}

fn value_from_text(text: &str) -> Option<serde_json::Value> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    Some(serde_json::from_str(text).unwrap_or_else(|_| serde_json::Value::String(text.to_owned())))
}

#[expect(
    deprecated,
    reason = "ComboBoxText remains supported by the minimum GTK runtime"
)]
fn filter_row(
    catalog: &FieldCatalog,
    initial: Option<&BaseFilter>,
    id: u64,
    on_selected: impl Fn(BaseColumn) + 'static,
) -> (gtk::Box, FilterWidgets) {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    row.set_hexpand(true);
    let initial_field = initial.map_or_else(|| BaseColumn::Name, |filter| filter.field.clone());
    let field = FieldPicker::new(
        catalog,
        &initial_field,
        &format!("base-rule-filter-field-{id}"),
        false,
        None,
        on_selected,
    );
    let operator = gtk::ComboBoxText::new();
    add_operator_options(&operator);
    let value = gtk::Entry::new();
    value.set_hexpand(true);
    if let Some(filter) = initial {
        operator.set_active_id(Some(operator_id(filter.operator)));
        if let Some(value_json) = &filter.value {
            let text = value_json
                .as_str()
                .map_or_else(|| value_json.to_string(), ToOwned::to_owned);
            value.set_text(&text);
        }
    } else {
        operator.set_active_id(Some("equals"));
    }
    row.append(&field.button);
    row.append(&operator);
    row.append(&value);
    (
        row.clone(),
        FilterWidgets {
            id,
            row: row.clone(),
            field,
            operator,
            value,
        },
    )
}

#[expect(
    deprecated,
    reason = "ComboBoxText remains supported by the minimum GTK runtime"
)]
fn sort_row(
    catalog: &FieldCatalog,
    initial: Option<&BaseSort>,
    id: u64,
) -> (gtk::Box, SortWidgets) {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    let initial_field = initial.map_or_else(|| BaseColumn::Updated, |sort| sort.field.clone());
    let field = FieldPicker::new(
        catalog,
        &initial_field,
        &format!("base-rule-sort-field-{id}"),
        false,
        None,
        |_| {},
    );
    let direction = gtk::ComboBoxText::new();
    direction.append(Some("ascending"), "Ascending");
    direction.append(Some("descending"), "Descending");
    if let Some(sort) = initial {
        direction.set_active_id(Some(
            if matches!(sort.direction, BaseSortDirection::Ascending) {
                "ascending"
            } else {
                "descending"
            },
        ));
    } else {
        direction.set_active_id(Some("descending"));
    }
    row.append(&field.button);
    row.append(&direction);
    (
        row.clone(),
        SortWidgets {
            id,
            row,
            field,
            direction,
        },
    )
}

fn visible_columns(definition: &BaseDefinition) -> Vec<BaseColumn> {
    let mut columns = vec![BaseColumn::Name];
    columns.extend(
        definition
            .columns
            .iter()
            .filter(|field| !matches!(field, BaseColumn::Name))
            .cloned(),
    );
    columns
}

fn rebuild_visible_columns(
    container: &gtk::Box,
    selected: &Rc<RefCell<Vec<BaseColumn>>>,
    catalog: &FieldCatalog,
) {
    while let Some(child) = container.first_child() {
        container.remove(&child);
    }
    let fields = selected.borrow().clone();
    for (index, field) in fields.into_iter().enumerate() {
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        row.set_widget_name(&format!("base-visible-field-{index}"));
        row.set_margin_top(2);
        row.set_margin_bottom(2);
        let handle = gtk::Image::from_icon_name("list-drag-handle-symbolic");
        handle.set_opacity(0.65);
        handle.set_tooltip_text(Some("Drag to reorder field"));
        row.append(&handle);
        let labels = gtk::Box::new(gtk::Orientation::Vertical, 1);
        labels.set_hexpand(true);
        let label = gtk::Label::new(Some(&field_label(&field)));
        label.set_xalign(0.0);
        labels.append(&label);
        let metadata = catalog
            .options()
            .into_iter()
            .find(|option| option.field == field)
            .map_or_else(|| "Field".to_owned(), |option| option.metadata);
        let metadata_label = gtk::Label::new(Some(&metadata));
        metadata_label.set_xalign(0.0);
        metadata_label.add_css_class("dim-label");
        labels.append(&metadata_label);
        row.append(&labels);
        if !matches!(field, BaseColumn::Name) {
            install_column_drag_and_drop(&row, &field, selected, container, catalog);
        }
        if !matches!(field, BaseColumn::Name) {
            let remove = gtk::Button::from_icon_name("list-remove-symbolic");
            remove.set_widget_name(&format!("base-visible-field-remove-{index}"));
            remove.set_tooltip_text(Some("Remove field"));
            remove.add_css_class("flat");
            remove.update_property(&[gtk::accessible::Property::Label("Remove field")]);
            let selected = Rc::clone(selected);
            let container = container.clone();
            let catalog = catalog.clone();
            remove.connect_clicked(move |_| {
                selected
                    .borrow_mut()
                    .retain(|candidate| candidate != &field);
                rebuild_visible_columns(&container, &selected, &catalog);
            });
            row.append(&remove);
        }
        container.append(&row);
    }
}

fn install_column_drag_and_drop(
    row: &gtk::Box,
    field: &BaseColumn,
    selected: &Rc<RefCell<Vec<BaseColumn>>>,
    container: &gtk::Box,
    catalog: &FieldCatalog,
) {
    use glib::{prelude::ToValue, types::StaticType};

    let source = gtk::DragSource::new();
    source.set_actions(gtk::gdk::DragAction::MOVE);
    let source_id = super::field_picker::field_id(field);
    source.connect_prepare(move |_source, _x, _y| {
        Some(gtk::gdk::ContentProvider::for_value(&source_id.to_value()))
    });
    row.add_controller(source);

    let target = gtk::DropTarget::new(String::static_type(), gtk::gdk::DragAction::MOVE);
    let selected = Rc::clone(selected);
    let container = container.clone();
    let catalog = catalog.clone();
    let target_field = field.clone();
    target.connect_drop(move |_target, value, _x, _y| {
        if matches!(target_field, BaseColumn::Name) {
            return false;
        }
        let Ok(source_id) = value.get::<String>() else {
            return false;
        };
        let target_id = super::field_picker::field_id(&target_field);
        if source_id == target_id {
            return false;
        }
        let changed = {
            let mut fields = selected.borrow_mut();
            let Some(source_index) = fields
                .iter()
                .position(|field| super::field_picker::field_id(field) == source_id)
            else {
                return false;
            };
            let Some(target_index) = fields.iter().position(|field| field == &target_field) else {
                return false;
            };
            let field = fields.remove(source_index);
            let target_index = fields
                .iter()
                .position(|candidate| candidate == &target_field)
                .unwrap_or(target_index.min(fields.len()));
            fields.insert(target_index, field);
            true
        };
        if changed {
            rebuild_visible_columns(&container, &selected, &catalog);
        }
        changed
    });
    row.add_controller(target);
}

/// Installs the Configure action for the current Base header.
pub(crate) fn render_configure(
    button: &gtk::Button,
    base: Option<&BaseDefinition>,
    rows: &[carver_sdk::BaseRow],
    property_descriptors: &[carver_sdk::PropertyDescriptor],
    dispatcher: &AppDispatcher,
) {
    let group = gtk::gio::SimpleActionGroup::new();
    let action = gtk::gio::SimpleAction::new("configure", None);
    action.set_enabled(base.is_some());
    if let Some(base) = base.cloned() {
        let rows = rows.to_vec();
        let property_descriptors = property_descriptors.to_vec();
        let dispatcher = dispatcher.clone();
        let weak_button = button.downgrade();
        action.connect_activate(move |_, _| {
            let Some(parent) = weak_button
                .upgrade()
                .and_then(|button| button.root())
                .and_downcast::<gtk::Window>()
            else {
                return;
            };
            show_configuration_dialog(&parent, &dispatcher, &base, &rows, &property_descriptors);
        });
    }
    group.add_action(&action);
    button.insert_action_group("base", Some(&group));
}

// CONTEXT: The modal builds a cohesive form and keeps widget ownership local to its presentation boundary.
#[expect(
    clippy::too_many_lines,
    reason = "Base configuration form is kept together for modal state"
)]
#[expect(
    deprecated,
    reason = "ComboBoxText remains supported by the minimum GTK runtime"
)]
fn show_configuration_dialog(
    parent: &gtk::Window,
    dispatcher: &AppDispatcher,
    definition: &BaseDefinition,
    rows: &[carver_sdk::BaseRow],
    property_descriptors: &[carver_sdk::PropertyDescriptor],
) {
    let dialog = adw::Dialog::builder()
        .title("Configure Base")
        .content_width(760)
        .content_height(680)
        .build();
    dialog.set_widget_name("base-configuration-dialog");
    let toolbar = adw::ToolbarView::new();
    toolbar.set_vexpand(true);
    toolbar.set_hexpand(true);
    toolbar.add_top_bar(&adw::HeaderBar::new());
    let content = gtk::Box::new(gtk::Orientation::Vertical, 16);
    content.set_vexpand(true);
    content.set_valign(gtk::Align::Start);
    content.set_margin_start(24);
    content.set_margin_end(24);
    content.set_margin_top(20);
    content.set_margin_bottom(20);

    let name = gtk::Entry::builder()
        .text(&definition.name)
        .placeholder_text("Base name")
        .build();
    content.append(&section_label("Name"));
    content.append(&name);

    let catalog = FieldCatalog::new(definition, property_descriptors);
    let selected_columns = Rc::new(RefCell::new(visible_columns(definition)));
    let columns_box = gtk::Box::new(gtk::Orientation::Vertical, 4);
    columns_box.set_widget_name("base-visible-fields-list");
    let columns_scroll = gtk::ScrolledWindow::new();
    columns_scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    columns_scroll.set_min_content_height(52);
    columns_scroll.set_max_content_height(220);
    columns_scroll.set_child(Some(&columns_box));
    content.append(&section_label("Visible fields"));
    content.append(&description_label(
        "Choose the columns shown in this Base. Name is always included.",
    ));
    content.append(&columns_scroll);
    rebuild_visible_columns(&columns_box, &selected_columns, &catalog);
    {
        let selected_columns = Rc::clone(&selected_columns);
        let columns_box = columns_box.clone();
        let catalog = catalog.clone();
        let catalog_for_callback = catalog.clone();
        let selected_for_marker = Rc::clone(&selected_columns);
        let is_selected: Rc<dyn Fn(&BaseColumn) -> bool> =
            Rc::new(move |field: &BaseColumn| selected_for_marker.borrow().contains(field));
        let add_picker = FieldPicker::new_with_selection(
            &catalog,
            &BaseColumn::Name,
            "base-add-visible-field-picker",
            true,
            Some("Add field…"),
            &is_selected,
            move |field| {
                let mut columns = selected_columns.borrow_mut();
                if !columns.contains(&field) {
                    columns.push(field);
                    drop(columns);
                    rebuild_visible_columns(&columns_box, &selected_columns, &catalog_for_callback);
                }
            },
        );
        add_picker.button.set_widget_name("base-add-visible-field");
        add_picker.button.add_css_class("flat");
        add_picker.button.set_halign(gtk::Align::Start);
        content.append(&add_picker.button);
    }

    let filter_mode = gtk::ComboBoxText::new();
    filter_mode.append(Some("all"), "Match all filters");
    filter_mode.append(Some("any"), "Match any filter");
    filter_mode.set_active_id(Some(
        if matches!(definition.filter_mode, BaseFilterMode::Any) {
            "any"
        } else {
            "all"
        },
    ));
    content.append(&section_label("Filters"));
    content.append(&description_label(
        "Use the arrows to set rule priority, or remove a rule you no longer need.",
    ));
    content.append(&filter_mode);
    let preview = gtk::Label::new(Some(&format!(
        "Currently matches {} {}",
        definition.row_count,
        if definition.row_count == 1 {
            "note"
        } else {
            "notes"
        }
    )));
    preview.set_xalign(0.0);
    preview.add_css_class("dim-label");
    preview.set_widget_name("base-configuration-preview");
    content.append(&preview);
    let filter_box = gtk::Box::new(gtk::Orientation::Vertical, 6);
    let filter_widgets: Rc<RefCell<Vec<FilterWidgets>>> = Rc::new(RefCell::new(Vec::new()));
    let sort_box = gtk::Box::new(gtk::Orientation::Vertical, 6);
    let sort_widgets: Rc<RefCell<Vec<SortWidgets>>> = Rc::new(RefCell::new(Vec::new()));
    let add_filter = gtk::Button::with_label("Add filter");
    add_filter.set_widget_name("base-add-filter");
    let add_sort = gtk::Button::with_label("Add sort rule");
    add_sort.set_widget_name("base-add-sort");
    let next_filter_id = Rc::new(Cell::new(definition.filters.len() as u64));
    let next_sort_id = Rc::new(Cell::new(definition.sorts.len() as u64));
    let refresh_preview: Rc<dyn Fn()> = {
        let preview = preview.clone();
        let rows = rows.to_vec();
        let filter_mode = filter_mode.clone();
        let filter_widgets = Rc::clone(&filter_widgets);
        let sort_widgets = Rc::clone(&sort_widgets);
        Rc::new(move || {
            let filters = selected_filters(&filter_widgets.borrow());
            let sorts = selected_sorts(&sort_widgets.borrow());
            let count = carver_domain::project_base_rows(
                rows.clone(),
                selected_filter_mode(&filter_mode),
                &filters,
                &sorts,
            )
            .len();
            preview.set_text(&format!(
                "Currently matches {count} {}",
                if count == 1 { "note" } else { "notes" }
            ));
        })
    };
    {
        let refresh_preview = Rc::downgrade(&refresh_preview);
        filter_mode.connect_changed(move |_| {
            if let Some(refresh_preview) = refresh_preview.upgrade() {
                refresh_preview();
            }
        });
    }
    for (index, filter) in definition.filters.iter().enumerate() {
        let id = index as u64;
        let refresh_preview_for_field = Rc::clone(&refresh_preview);
        let (row, widgets) = filter_row(&catalog, Some(filter), id, move |_| {
            refresh_preview_for_field();
        });
        connect_filter_preview(&widgets, &refresh_preview);
        filter_widgets.borrow_mut().push(widgets);
        append_rule_controls(
            &row,
            "filter",
            id,
            &filter_box,
            &filter_widgets,
            &add_filter,
            Some(&refresh_preview),
        );
        filter_box.append(&row);
    }
    {
        let catalog = catalog.clone();
        let filter_box = filter_box.clone();
        let filter_widgets = Rc::clone(&filter_widgets);
        let next_filter_id = Rc::clone(&next_filter_id);
        let add_filter_for_rows = add_filter.clone();
        let refresh_preview = Rc::clone(&refresh_preview);
        add_filter.connect_clicked(move |_| {
            let id = allocate_rule_id(&next_filter_id);
            let refresh_preview_for_field = Rc::clone(&refresh_preview);
            let (row, widgets) =
                filter_row(&catalog, None, id, move |_| refresh_preview_for_field());
            connect_filter_preview(&widgets, &refresh_preview);
            filter_widgets.borrow_mut().push(widgets);
            append_rule_controls(
                &row,
                "filter",
                id,
                &filter_box,
                &filter_widgets,
                &add_filter_for_rows,
                Some(&refresh_preview),
            );
            rebuild_rule_rows(&filter_box, &filter_widgets.borrow(), &add_filter_for_rows);
            refresh_preview();
        });
    }
    filter_box.append(&add_filter);
    content.append(&filter_box);

    content.append(&section_label("Sort"));
    content.append(&description_label(
        "Sort rules are applied from top to bottom.",
    ));
    for (index, sort) in definition.sorts.iter().enumerate() {
        let id = index as u64;
        let (row, widgets) = sort_row(&catalog, Some(sort), id);
        sort_widgets.borrow_mut().push(widgets);
        append_rule_controls(
            &row,
            "sort rule",
            id,
            &sort_box,
            &sort_widgets,
            &add_sort,
            None,
        );
        sort_box.append(&row);
    }
    {
        let catalog = catalog.clone();
        let sort_box = sort_box.clone();
        let sort_widgets = Rc::clone(&sort_widgets);
        let next_sort_id = Rc::clone(&next_sort_id);
        let add_sort_for_rows = add_sort.clone();
        add_sort.connect_clicked(move |_| {
            let id = allocate_rule_id(&next_sort_id);
            let (row, widgets) = sort_row(&catalog, None, id);
            sort_widgets.borrow_mut().push(widgets);
            append_rule_controls(
                &row,
                "sort rule",
                id,
                &sort_box,
                &sort_widgets,
                &add_sort_for_rows,
                None,
            );
            rebuild_rule_rows(&sort_box, &sort_widgets.borrow(), &add_sort_for_rows);
        });
    }
    sort_box.append(&add_sort);
    content.append(&sort_box);
    refresh_preview();

    let save = gtk::Button::with_label("Save");
    save.add_css_class("suggested-action");
    save.set_sensitive(!definition.name.trim().is_empty());
    {
        let save = save.clone();
        name.connect_changed(move |entry| save.set_sensitive(!entry.text().trim().is_empty()));
    }
    let scroll = gtk::ScrolledWindow::new();
    scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    scroll.set_hexpand(true);
    scroll.set_vexpand(true);
    scroll.set_min_content_height(560);
    scroll.set_size_request(-1, 560);
    scroll.set_child(Some(&content));
    let footer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    footer.set_margin_start(24);
    footer.set_margin_end(24);
    footer.set_margin_bottom(16);
    footer.append(&save);
    let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
    root.set_hexpand(true);
    root.set_vexpand(true);
    root.append(&scroll);
    root.append(&footer);
    toolbar.set_content(Some(&root));
    dialog.set_child(Some(&toolbar));

    let dispatcher = dispatcher.clone();
    let definition = definition.clone();
    let name_for_save = name.clone();
    let dialog_for_save = dialog.clone();
    save.connect_clicked(move |_| {
        let columns = selected_columns.borrow().clone();
        let filters = selected_filters(&filter_widgets.borrow());
        let sorts = selected_sorts(&sort_widgets.borrow());
        let _ = dispatcher.dispatch(AppMsg::Bases(BasesMsg::Update {
            base_id: definition.id,
            revision: definition.revision,
            name: name_for_save.text().to_string(),
            columns,
            filter_mode: selected_filter_mode(&filter_mode),
            filters,
            sorts,
        }));
        dialog_for_save.close();
    });
    name.grab_focus();
    dialog.present(Some(parent));
}

fn section_label(text: &str) -> gtk::Label {
    let label = gtk::Label::new(Some(text));
    label.set_xalign(0.0);
    label.add_css_class("heading");
    label
}

fn description_label(text: &str) -> gtk::Label {
    let label = gtk::Label::new(Some(text));
    label.set_xalign(0.0);
    label.set_wrap(true);
    label.add_css_class("dim-label");
    label
}
