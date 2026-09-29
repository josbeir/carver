//! Header actions and configuration for the current Base.
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use crate::mvu::{AppDispatcher, AppMsg, BasesMsg, RequestId, TabOrigin, TabsMsg};
use carver_config::DocumentProperty;
use carver_sdk::{
    BaseColumn, BaseDefinition, BaseFilter, BaseFilterMode, BaseFilterOperator, BaseSort,
    BaseSortDirection, BaseView,
};
use gettextrs::{gettext, ngettext};
use gtk::prelude::*;
use libadwaita::{self as adw, prelude::*};

use super::field_picker::{FieldCatalog, FieldPicker, FieldPickerOptions, field_label};

#[derive(Clone, Copy)]
pub(crate) enum BaseConfigurationMode {
    Create,
    Update {
        base_id: carver_sdk::BaseId,
        revision: carver_sdk::Revision,
    },
}

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
            let dialog = adw::AlertDialog::builder()
                .heading(tr_fmt!(gettext("Delete “{name}”?"), name = name))
                .body(gettext(
                    "Only this Base will be deleted. Your notes and their properties will not be changed.",
                ))
                .close_response("cancel")
                .default_response("cancel")
                .build();
            dialog.set_widget_name("delete-base-confirmation");
            let cancel = gettext("Cancel");
            let delete = gettext("Delete Base");
            dialog.add_responses(&[("cancel", cancel.as_str()), ("delete", delete.as_str())]);
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

/// Shows and wires the context-scoped "Close tabs" action for one Base.
pub(crate) fn render_close_tabs(
    button: &gtk::Button,
    base: Option<&BaseDefinition>,
    has_tabs: bool,
    dispatcher: &AppDispatcher,
) {
    let group = gtk::gio::SimpleActionGroup::new();
    let action = gtk::gio::SimpleAction::new("close-tabs", None);
    let active = base.is_some() && has_tabs;
    action.set_enabled(active);
    if let Some(base) = base {
        let base_id = base.id;
        let dispatcher = dispatcher.clone();
        action.connect_activate(move |_, _| {
            let _ =
                dispatcher.dispatch(AppMsg::Tabs(TabsMsg::CloseOrigin(TabOrigin::Base(base_id))));
        });
    }
    group.add_action(&action);
    button.insert_action_group("base", Some(&group));
    button.set_visible(active);
}

struct FilterWidgets {
    id: u64,
    row: adw::ExpanderRow,
    field: FieldPicker,
    operator: adw::ComboRow,
    value: adw::EntryRow,
}

struct SortWidgets {
    id: u64,
    row: adw::ExpanderRow,
    field: FieldPicker,
    direction: adw::ComboRow,
}

trait RuleWidgets {
    fn id(&self) -> u64;
    fn row(&self) -> &adw::ExpanderRow;
}

impl RuleWidgets for FilterWidgets {
    fn id(&self) -> u64 {
        self.id
    }

    fn row(&self) -> &adw::ExpanderRow {
        &self.row
    }
}

impl RuleWidgets for SortWidgets {
    fn id(&self) -> u64 {
        self.id
    }

    fn row(&self) -> &adw::ExpanderRow {
        &self.row
    }
}

/// Rebuilds the rule expanders inside a preferences group, preserving order.
fn rebuild_rule_rows<T: RuleWidgets>(
    group: &adw::PreferencesGroup,
    rendered: &Rc<RefCell<Vec<adw::ExpanderRow>>>,
    rows: &[T],
) {
    for widget in rendered.borrow().iter() {
        group.remove(widget);
    }
    rendered.borrow_mut().clear();
    for rule in rows {
        group.add(rule.row());
        rendered.borrow_mut().push(rule.row().clone());
    }
}

fn reorder_rule<T: RuleWidgets>(
    rows: &Rc<RefCell<Vec<T>>>,
    group: &adw::PreferencesGroup,
    rendered: &Rc<RefCell<Vec<adw::ExpanderRow>>>,
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
    rebuild_rule_rows(group, rendered, &rows.borrow());
}

fn remove_rule<T: RuleWidgets>(
    rows: &Rc<RefCell<Vec<T>>>,
    group: &adw::PreferencesGroup,
    rendered: &Rc<RefCell<Vec<adw::ExpanderRow>>>,
    id: u64,
) {
    let removed = {
        let mut rows = rows.borrow_mut();
        rows.iter()
            .position(|rule| rule.id() == id)
            .map(|index| rows.remove(index))
    };
    if removed.is_some() {
        rebuild_rule_rows(group, rendered, &rows.borrow());
    }
}

fn rule_button(icon_name: &str, tooltip: &str) -> gtk::Button {
    let button = gtk::Button::new();
    let icon = gtk::Image::from_icon_name(icon_name);
    icon.set_pixel_size(14);
    button.set_child(Some(&icon));
    button.set_tooltip_text(Some(tooltip));
    button.add_css_class("circular");
    button.set_size_request(30, 30);
    button.set_valign(gtk::Align::Center);
    button
}

#[expect(
    clippy::too_many_arguments,
    reason = "one helper wires the reorder and remove controls for both rule kinds"
)]
fn append_rule_controls<T: RuleWidgets + 'static>(
    row: &adw::ExpanderRow,
    noun: &'static str,
    noun_label: &str,
    id: u64,
    group: &adw::PreferencesGroup,
    rows: &Rc<RefCell<Vec<T>>>,
    rendered: &Rc<RefCell<Vec<adw::ExpanderRow>>>,
    refresh_preview: Option<&Rc<dyn Fn()>>,
) {
    let up = rule_button(
        "go-up-symbolic",
        &tr_fmt!(gettext("Move {noun} up"), noun = noun_label),
    );
    let down = rule_button(
        "go-down-symbolic",
        &tr_fmt!(gettext("Move {noun} down"), noun = noun_label),
    );
    let remove = rule_button(
        "list-remove-symbolic",
        &tr_fmt!(gettext("Remove {noun}"), noun = noun_label),
    );
    let prefix = noun.replace(' ', "-");
    up.set_widget_name(&format!("base-rule-{prefix}-up-{id}"));
    down.set_widget_name(&format!("base-rule-{prefix}-down-{id}"));
    remove.set_widget_name(&format!("base-rule-{prefix}-remove-{id}"));
    {
        let rows = Rc::clone(rows);
        let group = group.clone();
        let rendered = Rc::clone(rendered);
        let refresh_preview = refresh_preview.map(Rc::downgrade);
        up.connect_clicked(move |_| {
            reorder_rule(&rows, &group, &rendered, id, -1);
            if let Some(refresh_preview) = refresh_preview.as_ref().and_then(std::rc::Weak::upgrade)
            {
                refresh_preview();
            }
        });
    }
    {
        let rows = Rc::clone(rows);
        let group = group.clone();
        let rendered = Rc::clone(rendered);
        let refresh_preview = refresh_preview.map(Rc::downgrade);
        down.connect_clicked(move |_| {
            reorder_rule(&rows, &group, &rendered, id, 1);
            if let Some(refresh_preview) = refresh_preview.as_ref().and_then(std::rc::Weak::upgrade)
            {
                refresh_preview();
            }
        });
    }
    {
        let rows = Rc::clone(rows);
        let group = group.clone();
        let rendered = Rc::clone(rendered);
        let refresh_preview = refresh_preview.map(Rc::downgrade);
        remove.connect_clicked(move |_| {
            remove_rule(&rows, &group, &rendered, id);
            if let Some(refresh_preview) = refresh_preview.as_ref().and_then(std::rc::Weak::upgrade)
            {
                refresh_preview();
            }
        });
    }
    row.add_suffix(&up);
    row.add_suffix(&down);
    row.add_suffix(&remove);
}

fn allocate_rule_id(counter: &Cell<u64>) -> u64 {
    let id = counter.get();
    counter.set(id.saturating_add(1));
    id
}

const FILTER_OPERATORS: [BaseFilterOperator; 12] = [
    BaseFilterOperator::Equals,
    BaseFilterOperator::NotEquals,
    BaseFilterOperator::Contains,
    BaseFilterOperator::StartsWith,
    BaseFilterOperator::GreaterThan,
    BaseFilterOperator::LessThan,
    BaseFilterOperator::GreaterOrEqual,
    BaseFilterOperator::LessOrEqual,
    BaseFilterOperator::IsPresent,
    BaseFilterOperator::IsMissing,
    BaseFilterOperator::ListContains,
    BaseFilterOperator::ListNotContains,
];

fn operator_label(operator: BaseFilterOperator) -> String {
    match operator {
        BaseFilterOperator::Equals => gettext("is"),
        BaseFilterOperator::NotEquals => gettext("is not"),
        BaseFilterOperator::Contains => gettext("contains"),
        BaseFilterOperator::StartsWith => gettext("starts with"),
        BaseFilterOperator::GreaterThan => gettext(">"),
        BaseFilterOperator::LessThan => gettext("<"),
        BaseFilterOperator::GreaterOrEqual => gettext("≥"),
        BaseFilterOperator::LessOrEqual => gettext("≤"),
        BaseFilterOperator::IsPresent => gettext("is present"),
        BaseFilterOperator::IsMissing => gettext("is missing"),
        BaseFilterOperator::ListContains => gettext("list contains"),
        BaseFilterOperator::ListNotContains => gettext("list does not contain"),
    }
}

fn operator_index(operator: BaseFilterOperator) -> u32 {
    FILTER_OPERATORS
        .iter()
        .position(|candidate| *candidate == operator)
        .map_or(0, |index| u32::try_from(index).unwrap_or(0))
}

fn operator_model() -> gtk::StringList {
    let labels: Vec<String> = FILTER_OPERATORS
        .iter()
        .map(|op| operator_label(*op))
        .collect();
    let labels: Vec<&str> = labels.iter().map(String::as_str).collect();
    gtk::StringList::new(&labels)
}

fn operator_combo(initial: Option<BaseFilterOperator>, id: u64) -> adw::ComboRow {
    let combo = adw::ComboRow::new();
    combo.set_widget_name(&format!("base-rule-filter-operator-{id}"));
    combo.set_title(&gettext("Operator"));
    combo.set_model(Some(&operator_model()));
    combo.set_selected(operator_index(
        initial.unwrap_or(BaseFilterOperator::Equals),
    ));
    combo
}

/// Maps the list card's checked state to the persisted presentation.
fn base_view_from_active(list_active: bool) -> BaseView {
    if list_active {
        BaseView::List
    } else {
        BaseView::Grid
    }
}

/// Builds one selectable card describing a Base presentation.
fn base_view_card(
    widget_name: &str,
    icon_name: &str,
    title: &str,
    description: &str,
) -> gtk::ToggleButton {
    let card = gtk::ToggleButton::new();
    card.set_widget_name(widget_name);
    card.add_css_class("flat");
    card.add_css_class("base-view-card");
    card.update_property(&[gtk::accessible::Property::Label(title)]);
    let content = gtk::Box::new(gtk::Orientation::Vertical, 6);
    content.set_halign(gtk::Align::Center);
    content.set_valign(gtk::Align::Center);
    let icon = gtk::Image::from_icon_name(icon_name);
    icon.set_pixel_size(32);
    icon.set_halign(gtk::Align::Center);
    content.append(&icon);
    let heading = gtk::Label::new(Some(title));
    heading.add_css_class("heading");
    content.append(&heading);
    let body = gtk::Label::new(Some(description));
    body.add_css_class("dim-label");
    body.add_css_class("base-view-card-description");
    body.set_wrap(true);
    body.set_justify(gtk::Justification::Center);
    body.set_max_width_chars(24);
    content.append(&body);
    card.set_child(Some(&content));
    card
}

fn selected_filter_mode(combo: &adw::ComboRow) -> BaseFilterMode {
    if combo.selected() == 1 {
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

fn selected_sorts(widgets: &[SortWidgets]) -> Vec<BaseSort> {
    widgets
        .iter()
        .map(|widgets| BaseSort {
            field: widgets.field.selected(),
            direction: if widgets.direction.selected() == 0 {
                BaseSortDirection::Ascending
            } else {
                BaseSortDirection::Descending
            },
        })
        .collect()
}

fn connect_filter_preview(widgets: &FilterWidgets, refresh_preview: &Rc<dyn Fn()>) {
    let operator_refresh = Rc::downgrade(refresh_preview);
    widgets.operator.connect_selected_notify(move |_| {
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

fn selected_operator(combo: &adw::ComboRow) -> BaseFilterOperator {
    FILTER_OPERATORS
        .get(combo.selected() as usize)
        .copied()
        .unwrap_or(BaseFilterOperator::Equals)
}

fn value_from_text(text: &str) -> Option<serde_json::Value> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    Some(serde_json::from_str(text).unwrap_or_else(|_| serde_json::Value::String(text.to_owned())))
}

fn catalog_field_label(catalog: &FieldCatalog, field: &BaseColumn) -> String {
    catalog
        .options()
        .into_iter()
        .find(|option| &option.field == field)
        .map_or_else(|| gettext("Field"), |option| option.label)
}

/// Builds the "Field" expander row whose picker button selects the rule's column.
fn field_picker_row(field: &FieldPicker) -> adw::ActionRow {
    let row = adw::ActionRow::new();
    row.set_title(&gettext("Field"));
    row.set_use_markup(false);
    let button = field.button.clone();
    button.add_css_class("flat");
    row.add_suffix(&button);
    row.set_activatable_widget(Some(&button));
    row
}

fn filter_row(
    catalog: &FieldCatalog,
    initial: Option<&BaseFilter>,
    id: u64,
    on_selected: impl Fn(BaseColumn) + 'static,
) -> (adw::ExpanderRow, FilterWidgets) {
    let initial_field = initial.map_or_else(|| BaseColumn::Name, |filter| filter.field.clone());
    let expander = adw::ExpanderRow::new();
    expander.set_widget_name(&format!("base-filter-rule-{id}"));
    expander.set_use_markup(false);
    let expander_for_title = expander.clone();
    let catalog_for_title = catalog.clone();
    let field = FieldPicker::new(
        catalog,
        &initial_field,
        &format!("base-rule-filter-field-{id}"),
        false,
        None,
        move |field| {
            expander_for_title.set_title(&catalog_field_label(&catalog_for_title, &field));
            on_selected(field);
        },
    );
    expander.set_title(&catalog_field_label(catalog, &initial_field));
    expander.add_row(&field_picker_row(&field));
    let operator = operator_combo(initial.map(|filter| filter.operator), id);
    expander.set_subtitle(&operator_label(
        initial.map_or(BaseFilterOperator::Equals, |filter| filter.operator),
    ));
    let expander_for_subtitle = expander.clone();
    operator.connect_selected_notify(move |combo| {
        expander_for_subtitle.set_subtitle(&operator_label(selected_operator(combo)));
    });
    expander.add_row(&operator);
    let value = adw::EntryRow::new();
    value.set_title(&gettext("Value"));
    if let Some(filter) = initial
        && let Some(value_json) = &filter.value
    {
        value.set_text(&filter_value_text(value_json));
    }
    expander.add_row(&value);
    (
        expander.clone(),
        FilterWidgets {
            id,
            row: expander,
            field,
            operator,
            value,
        },
    )
}

fn sort_row(
    catalog: &FieldCatalog,
    initial: Option<&BaseSort>,
    id: u64,
) -> (adw::ExpanderRow, SortWidgets) {
    let initial_field = initial.map_or_else(|| BaseColumn::Updated, |sort| sort.field.clone());
    let expander = adw::ExpanderRow::new();
    expander.set_widget_name(&format!("base-sort-rule-{id}"));
    expander.set_use_markup(false);
    let expander_for_title = expander.clone();
    let catalog_for_title = catalog.clone();
    let field = FieldPicker::new(
        catalog,
        &initial_field,
        &format!("base-rule-sort-field-{id}"),
        false,
        None,
        move |field| {
            expander_for_title.set_title(&catalog_field_label(&catalog_for_title, &field));
        },
    );
    expander.set_title(&catalog_field_label(catalog, &initial_field));
    expander.add_row(&field_picker_row(&field));
    let direction = adw::ComboRow::new();
    direction.set_widget_name(&format!("base-rule-sort-direction-{id}"));
    direction.set_title(&gettext("Direction"));
    direction.set_model(Some(&gtk::StringList::new(&[
        gettext("Ascending").as_str(),
        gettext("Descending").as_str(),
    ])));
    let ascending =
        initial.is_some_and(|sort| matches!(sort.direction, BaseSortDirection::Ascending));
    direction.set_selected(u32::from(!ascending));
    expander.set_subtitle(&if ascending {
        gettext("Ascending")
    } else {
        gettext("Descending")
    });
    let expander_for_subtitle = expander.clone();
    direction.connect_selected_notify(move |combo| {
        expander_for_subtitle.set_subtitle(&if combo.selected() == 0 {
            gettext("Ascending")
        } else {
            gettext("Descending")
        });
    });
    expander.add_row(&direction);
    (
        expander.clone(),
        SortWidgets {
            id,
            row: expander,
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
    group: &adw::PreferencesGroup,
    rendered: &Rc<RefCell<Vec<adw::ActionRow>>>,
    selected: &Rc<RefCell<Vec<BaseColumn>>>,
    catalog: &FieldCatalog,
) {
    for row in rendered.borrow().iter() {
        group.remove(row);
    }
    rendered.borrow_mut().clear();
    let fields = selected.borrow().clone();
    let mut new_rows = Vec::with_capacity(fields.len());
    for (index, field) in fields.into_iter().enumerate() {
        let row = adw::ActionRow::new();
        row.set_widget_name(&format!("base-visible-field-{index}"));
        row.set_use_markup(false);
        row.set_title(&field_label(&field));
        let metadata = catalog
            .options()
            .into_iter()
            .find(|option| option.field == field)
            .map_or_else(|| gettext("Field"), |option| option.metadata);
        row.set_subtitle(&metadata);
        if !matches!(field, BaseColumn::Name) {
            let move_up = rule_button("go-up-symbolic", &gettext("Move field up"));
            move_up.set_widget_name(&format!("base-visible-field-move-up-{index}"));
            let move_down = rule_button("go-down-symbolic", &gettext("Move field down"));
            move_down.set_widget_name(&format!("base-visible-field-move-down-{index}"));
            {
                let field = field.clone();
                let selected = Rc::clone(selected);
                let group = group.clone();
                let rendered = Rc::clone(rendered);
                let catalog = catalog.clone();
                move_up.connect_clicked(move |_| {
                    if move_visible_column_by_offset(&mut selected.borrow_mut(), &field, -1) {
                        rebuild_visible_columns(&group, &rendered, &selected, &catalog);
                    }
                });
            }
            {
                let field = field.clone();
                let selected = Rc::clone(selected);
                let group = group.clone();
                let rendered = Rc::clone(rendered);
                let catalog = catalog.clone();
                move_down.connect_clicked(move |_| {
                    if move_visible_column_by_offset(&mut selected.borrow_mut(), &field, 1) {
                        rebuild_visible_columns(&group, &rendered, &selected, &catalog);
                    }
                });
            }
            let remove = rule_button("list-remove-symbolic", &gettext("Remove field"));
            remove.set_widget_name(&format!("base-visible-field-remove-{index}"));
            remove.update_property(&[gtk::accessible::Property::Label(&gettext("Remove field"))]);
            let selected = Rc::clone(selected);
            let group = group.clone();
            let rendered = Rc::clone(rendered);
            let catalog = catalog.clone();
            remove.connect_clicked(move |_| {
                selected
                    .borrow_mut()
                    .retain(|candidate| candidate != &field);
                rebuild_visible_columns(&group, &rendered, &selected, &catalog);
            });
            row.add_suffix(&move_up);
            row.add_suffix(&move_down);
            row.add_suffix(&remove);
        }
        group.add(&row);
        new_rows.push(row);
    }
    *rendered.borrow_mut() = new_rows;
}

/// Moves a non-Name visible column relative to its current position.
///
/// Name is intentionally kept at index zero, while built-in and custom fields may be freely
/// reordered around one another.
fn move_visible_column_by_offset(
    fields: &mut [BaseColumn],
    field: &BaseColumn,
    offset: isize,
) -> bool {
    let Some(index) = fields.iter().position(|candidate| candidate == field) else {
        return false;
    };
    let target = match offset {
        -1 => index.checked_sub(1),
        1 => index.checked_add(1).filter(|target| *target < fields.len()),
        _ => None,
    };
    let Some(target) = target else {
        return false;
    };
    if index == 0 || target == 0 {
        return false;
    }
    fields.swap(index, target);
    true
}

/// Installs the Configure action for the current Base header.
pub(crate) fn render_configure(
    button: &gtk::Button,
    base: Option<&BaseDefinition>,
    dispatcher: &AppDispatcher,
) {
    let group = gtk::gio::SimpleActionGroup::new();
    let action = gtk::gio::SimpleAction::new("configure", None);
    action.set_enabled(base.is_some());
    let dispatcher = dispatcher.clone();
    action.connect_activate(move |_, _| {
        let _ = dispatcher.dispatch(AppMsg::Bases(BasesMsg::Configure));
    });
    group.add_action(&action);
    button.insert_action_group("base", Some(&group));
}

pub(crate) fn show_configuration_dialog(
    parent: &gtk::Window,
    dispatcher: &AppDispatcher,
    dialog_id: RequestId,
    definition: &BaseDefinition,
    property_descriptors: &[carver_sdk::PropertyDescriptor],
    default_properties: &[DocumentProperty],
) -> (adw::Dialog, BaseConfigurationForm) {
    show_base_configuration_dialog(
        parent,
        dispatcher,
        dialog_id,
        definition,
        property_descriptors,
        default_properties,
        BaseConfigurationMode::Update {
            base_id: definition.id,
            revision: definition.revision,
        },
    )
}

/// The starter definition used when creating a Base.
///
/// Only the implicit Title column is shown initially; Category, Updated, and any property are
/// added later through the visible-fields picker.
pub(crate) fn new_base_definition() -> BaseDefinition {
    BaseDefinition::defaults(
        carver_sdk::BaseId::new(),
        String::new(),
        Vec::new(),
        carver_sdk::Revision(0),
    )
}

/// Presents the shared Base settings pane before a new Base has been persisted.
pub(crate) fn show_new_configuration_dialog(
    parent: &gtk::Window,
    dispatcher: &AppDispatcher,
    dialog_id: RequestId,
    property_descriptors: &[carver_sdk::PropertyDescriptor],
    default_properties: &[DocumentProperty],
) -> (adw::Dialog, BaseConfigurationForm) {
    let definition = new_base_definition();
    show_base_configuration_dialog(
        parent,
        dispatcher,
        dialog_id,
        &definition,
        property_descriptors,
        default_properties,
        BaseConfigurationMode::Create,
    )
}

/// The embeddable Base configuration form, shared by the standalone dialog and
/// the tabbed Add dialog.
pub(crate) struct BaseConfigurationForm {
    pub(crate) page: adw::PreferencesPage,
    pub(crate) name: adw::EntryRow,
    pub(crate) save: gtk::Button,
}

impl BaseConfigurationForm {
    /// Enables or disables the form while a save is in flight.
    pub(crate) fn set_busy(&self, busy: bool) {
        self.page.set_sensitive(!busy);
        if !busy {
            self.save.set_sensitive(!self.name.text().trim().is_empty());
        }
    }
}

// CONTEXT: Create and update share one settings presentation; only persistence differs.
#[expect(
    clippy::too_many_lines,
    reason = "the Base configuration form is kept together for its shared modal state"
)]
pub(crate) fn build_base_configuration_form(
    dispatcher: &AppDispatcher,
    dialog_id: RequestId,
    definition: &BaseDefinition,
    property_descriptors: &[carver_sdk::PropertyDescriptor],
    default_properties: &[DocumentProperty],
    mode: BaseConfigurationMode,
) -> BaseConfigurationForm {
    let page = adw::PreferencesPage::new();

    let name_group = adw::PreferencesGroup::new();
    name_group.set_title(&gettext("Name"));
    let name = adw::EntryRow::new();
    name.set_widget_name("base-configuration-name");
    name.set_title(&gettext("Name"));
    name.set_text(&definition.name);
    name_group.add(&name);
    page.add(&name_group);

    let view_group = adw::PreferencesGroup::new();
    view_group.set_widget_name("base-view-section");
    view_group.set_title(&gettext("View"));
    view_group.set_description(Some(&gettext("Choose how this Base presents its notes.")));
    let grid_option = base_view_card(
        "base-view-grid",
        "view-grid-symbolic",
        &gettext("Grid"),
        &gettext("Edit notes and properties directly in a table."),
    );
    let list_option = base_view_card(
        "base-view-list",
        "view-list-symbolic",
        &gettext("Notes list"),
        &gettext("Browse notes as read-only cards."),
    );
    // A single toggle group makes the two cards mutually exclusive.
    list_option.set_group(Some(&grid_option));
    if matches!(definition.view, BaseView::List) {
        list_option.set_active(true);
    } else {
        grid_option.set_active(true);
    }
    let view_options = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    view_options.set_widget_name("base-view-options");
    view_options.set_homogeneous(true);
    view_options.append(&grid_option);
    view_options.append(&list_option);
    view_group.add(&view_options);
    page.add(&view_group);

    let catalog = FieldCatalog::new(definition, property_descriptors, default_properties);
    let selected_columns = Rc::new(RefCell::new(visible_columns(definition)));
    let visible_group = adw::PreferencesGroup::new();
    visible_group.set_widget_name("base-visible-fields-section");
    visible_group.set_title(&gettext("Visible fields"));
    visible_group.set_description(Some(&gettext(
        "Choose the columns shown in this Base. Name is always included.",
    )));
    let rendered_columns: Rc<RefCell<Vec<adw::ActionRow>>> = Rc::new(RefCell::new(Vec::new()));
    rebuild_visible_columns(
        &visible_group,
        &rendered_columns,
        &selected_columns,
        &catalog,
    );
    let add_field_label = gettext("Add field");
    {
        let selected_columns = Rc::clone(&selected_columns);
        let rendered_columns = Rc::clone(&rendered_columns);
        let visible_group_for_callback = visible_group.clone();
        let catalog = catalog.clone();
        let catalog_for_callback = catalog.clone();
        let selected_for_marker = Rc::clone(&selected_columns);
        let is_selected: Rc<dyn Fn(&BaseColumn) -> bool> =
            Rc::new(move |field: &BaseColumn| selected_for_marker.borrow().contains(field));
        let add_picker = FieldPicker::new_with_selection(
            &catalog,
            &BaseColumn::Name,
            "base-add-visible-field-picker",
            FieldPickerOptions {
                keep_open: true,
                button_label: None,
                button_icon_name: Some("list-add-symbolic"),
            },
            &is_selected,
            move |field| {
                let mut columns = selected_columns.borrow_mut();
                if !columns.contains(&field) {
                    columns.push(field);
                    drop(columns);
                    rebuild_visible_columns(
                        &visible_group_for_callback,
                        &rendered_columns,
                        &selected_columns,
                        &catalog_for_callback,
                    );
                }
            },
        );
        add_picker.button.set_widget_name("base-add-visible-field");
        add_picker.button.add_css_class("flat");
        add_picker
            .button
            .update_property(&[gtk::accessible::Property::Label(&add_field_label)]);
        let add_row = adw::ActionRow::new();
        add_row.set_title(&add_field_label);
        add_row.add_prefix(&add_picker.button);
        add_row.set_activatable_widget(Some(&add_picker.button));
        let visible_actions = adw::PreferencesGroup::new();
        visible_actions.add(&add_row);
        page.add(&visible_group);
        page.add(&visible_actions);
    }

    let filters_group = adw::PreferencesGroup::new();
    filters_group.set_widget_name("base-filters-section");
    filters_group.set_title(&gettext("Filters"));
    filters_group.set_description(Some(&gettext(
        "Use the arrows to set rule priority, or remove a rule you no longer need.",
    )));
    let filter_mode = adw::ComboRow::new();
    filter_mode.set_widget_name("base-filter-mode");
    filter_mode.set_use_markup(false);
    filter_mode.set_title(&gettext("Matching"));
    filter_mode.set_subtitle(&preview_count_text(definition.row_count));
    filter_mode.set_model(Some(&gtk::StringList::new(&[
        gettext("Match all filters").as_str(),
        gettext("Match any filter").as_str(),
    ])));
    filter_mode.set_selected(u32::from(matches!(
        definition.filter_mode,
        BaseFilterMode::Any
    )));
    filters_group.add(&filter_mode);
    let rendered_filters: Rc<RefCell<Vec<adw::ExpanderRow>>> = Rc::new(RefCell::new(Vec::new()));
    let filter_widgets: Rc<RefCell<Vec<FilterWidgets>>> = Rc::new(RefCell::new(Vec::new()));
    let next_filter_id = Rc::new(Cell::new(definition.filters.len() as u64));

    let sorts_group = adw::PreferencesGroup::new();
    sorts_group.set_widget_name("base-sort-section");
    sorts_group.set_title(&gettext("Sort"));
    sorts_group.set_description(Some(&gettext("Sort rules are applied from top to bottom.")));
    let rendered_sorts: Rc<RefCell<Vec<adw::ExpanderRow>>> = Rc::new(RefCell::new(Vec::new()));
    let sort_widgets: Rc<RefCell<Vec<SortWidgets>>> = Rc::new(RefCell::new(Vec::new()));
    let next_sort_id = Rc::new(Cell::new(definition.sorts.len() as u64));

    let refresh_preview: Rc<dyn Fn()> = {
        let dispatcher = dispatcher.clone();
        let filter_mode = filter_mode.clone();
        let filter_widgets = Rc::clone(&filter_widgets);
        Rc::new(move || {
            let filters = selected_filters(&filter_widgets.borrow());
            let _ = dispatcher.dispatch(AppMsg::Bases(BasesMsg::PreviewCount {
                dialog_id,
                filter_mode: selected_filter_mode(&filter_mode),
                filters,
            }));
        })
    };
    {
        let refresh_preview = Rc::downgrade(&refresh_preview);
        filter_mode.connect_selected_notify(move |_| {
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
            &gettext("filter"),
            id,
            &filters_group,
            &filter_widgets,
            &rendered_filters,
            Some(&refresh_preview),
        );
    }
    rebuild_rule_rows(&filters_group, &rendered_filters, &filter_widgets.borrow());
    page.add(&filters_group);
    {
        let add_filter = action_button_row("base-add-filter", &gettext("Add filter"));
        let catalog = catalog.clone();
        let filters_group = filters_group.clone();
        let rendered_filters = Rc::clone(&rendered_filters);
        let filter_widgets = Rc::clone(&filter_widgets);
        let next_filter_id = Rc::clone(&next_filter_id);
        let refresh_preview = Rc::clone(&refresh_preview);
        add_filter.connect_activated(move |_| {
            let id = allocate_rule_id(&next_filter_id);
            let refresh_preview_for_field = Rc::clone(&refresh_preview);
            let (row, widgets) =
                filter_row(&catalog, None, id, move |_| refresh_preview_for_field());
            connect_filter_preview(&widgets, &refresh_preview);
            filter_widgets.borrow_mut().push(widgets);
            append_rule_controls(
                &row,
                "filter",
                &gettext("filter"),
                id,
                &filters_group,
                &filter_widgets,
                &rendered_filters,
                Some(&refresh_preview),
            );
            rebuild_rule_rows(&filters_group, &rendered_filters, &filter_widgets.borrow());
            refresh_preview();
        });
        let filters_actions = adw::PreferencesGroup::new();
        filters_actions.add(&add_filter);
        page.add(&filters_actions);
    }

    for (index, sort) in definition.sorts.iter().enumerate() {
        let id = index as u64;
        let (row, widgets) = sort_row(&catalog, Some(sort), id);
        sort_widgets.borrow_mut().push(widgets);
        append_rule_controls(
            &row,
            "sort rule",
            &gettext("sort rule"),
            id,
            &sorts_group,
            &sort_widgets,
            &rendered_sorts,
            None,
        );
    }
    rebuild_rule_rows(&sorts_group, &rendered_sorts, &sort_widgets.borrow());
    page.add(&sorts_group);
    {
        let add_sort = action_button_row("base-add-sort", &gettext("Add sort rule"));
        let catalog = catalog.clone();
        let sorts_group = sorts_group.clone();
        let rendered_sorts = Rc::clone(&rendered_sorts);
        let sort_widgets = Rc::clone(&sort_widgets);
        let next_sort_id = Rc::clone(&next_sort_id);
        add_sort.connect_activated(move |_| {
            let id = allocate_rule_id(&next_sort_id);
            let (row, widgets) = sort_row(&catalog, None, id);
            sort_widgets.borrow_mut().push(widgets);
            append_rule_controls(
                &row,
                "sort rule",
                &gettext("sort rule"),
                id,
                &sorts_group,
                &sort_widgets,
                &rendered_sorts,
                None,
            );
            rebuild_rule_rows(&sorts_group, &rendered_sorts, &sort_widgets.borrow());
        });
        let sorts_actions = adw::PreferencesGroup::new();
        sorts_actions.add(&add_sort);
        page.add(&sorts_actions);
    }
    refresh_preview();

    let save = gtk::Button::with_label(&if matches!(mode, BaseConfigurationMode::Create) {
        gettext("Create")
    } else {
        gettext("Save")
    });
    save.set_widget_name("base-configuration-save");
    save.add_css_class("suggested-action");
    save.set_sensitive(!definition.name.trim().is_empty());
    {
        let save = save.clone();
        name.connect_changed(move |entry| save.set_sensitive(!entry.text().trim().is_empty()));
    }
    {
        let page = page.clone();
        let dispatcher = dispatcher.clone();
        let name_for_save = name.clone();
        let view_for_save = list_option.clone();
        save.connect_clicked(move |save| {
            if !save.is_sensitive() {
                return;
            }
            save.set_sensitive(false);
            page.set_sensitive(false);
            let columns = selected_columns.borrow().clone();
            let filters = selected_filters(&filter_widgets.borrow());
            let sorts = selected_sorts(&sort_widgets.borrow());
            let view = base_view_from_active(view_for_save.is_active());
            let message = match mode {
                BaseConfigurationMode::Create => BasesMsg::CreateConfigured {
                    name: name_for_save.text().to_string(),
                    columns,
                    filter_mode: selected_filter_mode(&filter_mode),
                    filters,
                    sorts,
                    view,
                },
                BaseConfigurationMode::Update { base_id, revision } => BasesMsg::Update {
                    base_id,
                    revision,
                    name: name_for_save.text().to_string(),
                    columns,
                    filter_mode: selected_filter_mode(&filter_mode),
                    filters,
                    sorts,
                    view,
                },
            };
            let _ = dispatcher.dispatch(AppMsg::Bases(message));
        });
    }
    BaseConfigurationForm { page, name, save }
}

/// Presents the Base configuration form in its standalone dialog (used to edit a Base).
fn show_base_configuration_dialog(
    parent: &gtk::Window,
    dispatcher: &AppDispatcher,
    dialog_id: RequestId,
    definition: &BaseDefinition,
    property_descriptors: &[carver_sdk::PropertyDescriptor],
    default_properties: &[DocumentProperty],
    mode: BaseConfigurationMode,
) -> (adw::Dialog, BaseConfigurationForm) {
    let form = build_base_configuration_form(
        dispatcher,
        dialog_id,
        definition,
        property_descriptors,
        default_properties,
        mode,
    );
    let dialog = adw::Dialog::builder()
        .title(if matches!(mode, BaseConfigurationMode::Create) {
            gettext("New Base")
        } else {
            gettext("Configure Base")
        })
        .content_width(640)
        .build();
    dialog.set_widget_name("base-configuration-dialog");
    let dismiss_dispatcher = dispatcher.clone();
    dialog.connect_closed(move |_| {
        let _ =
            dismiss_dispatcher.dispatch(AppMsg::Bases(BasesMsg::ConfigurationDismissed(dialog_id)));
    });
    let header = adw::HeaderBar::new();
    header.pack_end(&form.save);
    let busy_dialog = dialog.clone();
    form.save
        .connect_clicked(move |_| busy_dialog.set_can_close(false));
    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&header);
    toolbar.set_content(Some(&form.page));
    dialog.set_child(Some(&toolbar));
    form.name.grab_focus();
    dialog.present(Some(parent));
    (dialog, form)
}

/// Updates the matching-note count shown by an open configuration form.
pub(crate) fn render_preview(root: &gtk::Widget, count: usize) {
    let Some(row) = find_widget(root, "base-filter-mode").and_downcast::<adw::ComboRow>() else {
        return;
    };
    row.set_subtitle(&preview_count_text(count));
}

fn preview_count_text(count: usize) -> String {
    tr_fmt!(
        ngettext(
            "Currently matches {count} note",
            "Currently matches {count} notes",
            u32::try_from(count).unwrap_or(u32::MAX),
        ),
        count = count
    )
}

fn find_widget(root: &gtk::Widget, name: &str) -> Option<gtk::Widget> {
    if root.widget_name() == name {
        return Some(root.clone());
    }
    let mut child = root.first_child();
    while let Some(current) = child {
        if let Some(found) = find_widget(&current, name) {
            return Some(found);
        }
        child = current.next_sibling();
    }
    None
}

pub(crate) fn finish_configuration(
    form: &BaseConfigurationForm,
    dialog: &adw::Dialog,
    success: bool,
) {
    form.set_busy(false);
    dialog.set_can_close(true);
    if success {
        dialog.close();
    }
}

fn action_button_row(name: &str, title: &str) -> adw::ButtonRow {
    let row = adw::ButtonRow::new();
    row.set_title(title);
    row.set_start_icon_name(Some("list-add-symbolic"));
    row.set_widget_name(name);
    row
}

fn filter_value_text(value: &serde_json::Value) -> String {
    if let Some(text) = value.as_str()
        && value_from_text(text).as_ref() == Some(value)
    {
        return text.to_owned();
    }
    value.to_string()
}

#[cfg(test)]
#[path = "actions/tests.rs"]
mod tests;
