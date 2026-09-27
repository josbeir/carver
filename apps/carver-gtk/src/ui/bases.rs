//! Native database-style grid for saved note bases.
pub(crate) mod actions;
pub(crate) mod editing;
pub(crate) mod field_picker;

use std::rc::Rc;

use carver_config::DocumentProperty;
use carver_domain::{FrontmatterValue, PropertyDescriptor};
use carver_sdk::{
    BaseColumn, BaseDefinition, BaseRow, BaseSort, BaseSortDirection, NoteId, Revision,
};
use gettextrs::gettext;
use gtk::prelude::*;
use libadwaita as adw;

use self::editing::{
    CellEdit, CellEditor, build_boolean_cell, build_select_cell, options_with_current,
    resolve_editor, show_cell_editor,
};
use crate::mvu::{AppDispatcher, AppMsg, BasesMsg, NavigationMsg};
use crate::ui::search::{build_search_controls, connect_search_controls, install_search_shortcut};
use crate::ui::sidebar::{CompactNavigation, back_to_notes_button, sidebar_toggle_button};

/// Widgets needed to render the current saved base.
pub(crate) struct BaseViewRefs {
    pub(crate) configuration: std::cell::RefCell<
        Option<(
            crate::mvu::RequestId,
            adw::Dialog,
            actions::BaseConfigurationForm,
        )>,
    >,
    pub(crate) configure: gtk::Button,
    pub(crate) delete: gtk::Button,
    pub(crate) title: gtk::Label,
    pub(crate) search_bar: gtk::SearchBar,
    pub(crate) search_entry: gtk::SearchEntry,
    pub(crate) search_toggle: gtk::ToggleButton,
    pub(crate) last_search_open: std::cell::Cell<bool>,
    pub(crate) grid: gtk::ColumnView,
    pub(crate) pages: gtk::Stack,
    pub(crate) scroll: gtk::ScrolledWindow,
    pub(crate) status: adw::StatusPage,
    pub(crate) load_more: gtk::Button,
    pub(crate) rows: gtk::gio::ListStore,
    pub(crate) syncing_header_sort: std::rc::Rc<std::cell::Cell<bool>>,
    pub(crate) rendered_definition: std::cell::RefCell<Option<BaseDefinition>>,
    pub(crate) rendered_rows: std::cell::RefCell<Vec<(NoteId, Revision)>>,
}

#[expect(
    clippy::too_many_lines,
    reason = "the one-time Base composition keeps its widgets and paired view references together"
)]
pub(crate) fn build_base(
    dispatcher: &AppDispatcher,
    split_view: &adw::NavigationSplitView,
    compact_navigation: &CompactNavigation,
) -> (gtk::Widget, BaseViewRefs) {
    let toolbar = adw::ToolbarView::new();
    let header = adw::HeaderBar::new();
    header.pack_start(&sidebar_toggle_button(
        split_view,
        compact_navigation,
        "base-toggle-categories-button",
    ));
    let back = back_to_notes_button(
        dispatcher,
        "back-to-notes-from-base-button",
        AppMsg::Navigation(NavigationMsg::ShowBrowser),
    );
    header.pack_start(&back);
    let search = build_search_controls(
        "base",
        &gettext("Search this Base"),
        &gettext("Search this Base (Ctrl+F)"),
    );
    header.pack_start(&search.toggle);
    let title = gtk::Label::new(Some(&gettext("Base")));
    title.set_widget_name("base-title");
    title.add_css_class("title");
    header.set_title_widget(Some(&title));
    let delete = gtk::Button::from_icon_name("user-trash-symbolic");
    delete.set_widget_name("delete-base-button");
    delete.set_tooltip_text(Some(&gettext("Delete Base")));
    delete.set_action_name(Some("base.delete"));
    header.pack_end(&delete);
    let configure = gtk::Button::with_label(&gettext("Configure"));
    configure.set_icon_name("emblem-system-symbolic");
    configure.add_css_class("flat");
    configure.set_widget_name("configure-base-button");
    configure.set_action_name(Some("base.configure"));
    header.pack_end(&configure);
    toolbar.add_top_bar(&header);
    toolbar.add_top_bar(&search.bar);

    let rows = gtk::gio::ListStore::new::<glib::BoxedAnyObject>();
    let selection = gtk::NoSelection::new(Some(rows.clone()));
    let grid = gtk::ColumnView::new(Some(selection));
    grid.set_widget_name("bases-grid");
    grid.add_css_class("data-table");
    grid.add_css_class("bases-grid");
    grid.set_show_row_separators(true);
    grid.set_show_column_separators(true);
    grid.set_hexpand(true);
    grid.set_vexpand(true);
    let syncing_header_sort = std::rc::Rc::new(std::cell::Cell::new(false));
    connect_header_sorting(dispatcher, &grid, std::rc::Rc::clone(&syncing_header_sort));

    let scroll = gtk::ScrolledWindow::new();
    scroll.set_widget_name("base-scroll");
    scroll.set_policy(gtk::PolicyType::Automatic, gtk::PolicyType::Automatic);
    scroll.set_vexpand(true);
    scroll.set_child(Some(&grid));
    let status = adw::StatusPage::builder()
        .icon_name("view-grid-symbolic")
        .title(gettext("No matching notes"))
        .description(gettext("Notes that match this Base will appear here."))
        .build();
    status.set_widget_name("base-status");
    let load_more = gtk::Button::with_label(&gettext("Load more rows"));
    load_more.set_widget_name("base-load-more");
    load_more.add_css_class("flat");
    load_more.set_halign(gtk::Align::Center);
    load_more.set_visible(false);
    let grid_content = gtk::Box::new(gtk::Orientation::Vertical, 0);
    grid_content.set_vexpand(true);
    grid_content.append(&scroll);
    grid_content.append(&load_more);
    let pages = gtk::Stack::new();
    pages.set_widget_name("base-pages");
    pages.add_named(&grid_content, Some("grid"));
    pages.add_named(&status, Some("status"));
    pages.set_visible_child_name("grid");
    toolbar.set_content(Some(&pages));
    let dispatcher_for_button = dispatcher.clone();
    load_more.connect_clicked(move |_| {
        let _ = dispatcher_for_button.dispatch(AppMsg::Bases(BasesMsg::LoadMoreRows));
    });
    let dispatcher_for_scroll = dispatcher.clone();
    scroll
        .vadjustment()
        .connect_value_changed(move |adjustment| {
            let remaining = adjustment.upper() - adjustment.page_size() - adjustment.value();
            if remaining <= adjustment.page_size() * 2.0 {
                let _ = dispatcher_for_scroll.dispatch(AppMsg::Bases(BasesMsg::LoadMoreRows));
            }
        });
    connect_search_controls(
        dispatcher,
        &search,
        |query| AppMsg::Bases(BasesMsg::SearchChanged(query)),
        AppMsg::Bases(BasesMsg::SearchOpened),
        |visible| AppMsg::Bases(BasesMsg::SearchVisibilityChanged(visible)),
    );
    install_search_shortcut(
        &toolbar,
        dispatcher,
        "base-search-shortcut",
        AppMsg::Bases(BasesMsg::SearchShortcutRequested),
    );
    (
        toolbar.upcast(),
        BaseViewRefs {
            configuration: std::cell::RefCell::new(None),
            configure,
            delete,
            title,
            search_bar: search.bar,
            search_entry: search.entry,
            search_toggle: search.toggle,
            last_search_open: std::cell::Cell::new(false),
            grid,
            pages,
            scroll,
            status,
            load_more,
            rows,
            syncing_header_sort,
            rendered_definition: std::cell::RefCell::new(None),
            rendered_rows: std::cell::RefCell::new(Vec::new()),
        },
    )
}

pub(crate) fn render_base_status(refs: &BaseViewRefs, title: &str, description: &str) {
    refs.status.set_title(title);
    refs.status.set_description(Some(description));
    refs.pages.set_visible_child_name("status");
}

pub(crate) fn render_base_search(refs: &BaseViewRefs, open: bool, query: &str) {
    let was_open = refs.last_search_open.replace(open);
    let opening = open && !was_open;
    let closing = was_open && !open;
    if refs.search_bar.is_search_mode() != open {
        refs.search_bar.set_search_mode(open);
    }
    if refs.search_toggle.is_active() != open {
        refs.search_toggle.set_active(open);
    }
    if refs.search_entry.text().as_str() != query {
        refs.search_entry.set_text(query);
    }
    if opening {
        let bar = refs.search_bar.clone();
        let entry = refs.search_entry.clone();
        glib::idle_add_local_once(move || {
            if bar.is_search_mode() {
                entry.grab_focus();
                entry.select_region(0, -1);
            }
        });
    }
    if closing {
        refs.grid.grab_focus();
    }
}

pub(crate) fn render_base(
    refs: &BaseViewRefs,
    definition: &BaseDefinition,
    rows: &[BaseRow],
    descriptors: &[PropertyDescriptor],
    default_properties: &[DocumentProperty],
    dispatcher: &AppDispatcher,
) {
    refs.title.set_text(&definition.name);
    refs.grid.set_sensitive(true);
    if refs.rendered_definition.borrow().as_ref() != Some(definition) {
        rebuild_columns(
            refs,
            definition,
            descriptors,
            default_properties,
            dispatcher,
        );
        refs.rows.remove_all();
        refs.rendered_rows.borrow_mut().clear();
        refs.rendered_definition.replace(Some(definition.clone()));
    }
    append_rows(refs, rows);
    refs.pages
        .set_visible_child_name(if rows.is_empty() { "status" } else { "grid" });
}

fn rebuild_columns(
    refs: &BaseViewRefs,
    definition: &BaseDefinition,
    descriptors: &[PropertyDescriptor],
    default_properties: &[DocumentProperty],
    dispatcher: &AppDispatcher,
) {
    refs.syncing_header_sort.set(true);
    while let Some(column) = refs
        .grid
        .columns()
        .item(0)
        .and_downcast::<gtk::ColumnViewColumn>()
    {
        refs.grid.remove_column(&column);
    }
    append_column(
        &refs.grid,
        &BaseColumn::Name,
        &gettext("Name"),
        Some(CellEditor::Text),
        dispatcher,
    );
    for column in &definition.columns {
        match column {
            BaseColumn::Name => {}
            BaseColumn::Category => {
                append_column(&refs.grid, column, &gettext("Category"), None, dispatcher);
            }
            BaseColumn::Updated => {
                append_column(&refs.grid, column, &gettext("Updated"), None, dispatcher);
            }
            BaseColumn::Property(path) => append_column(
                &refs.grid,
                column,
                path.0.trim_start_matches('/'),
                resolve_editor(column, descriptors, default_properties),
                dispatcher,
            ),
        }
    }
    append_actions_column(&refs.grid, dispatcher);
    apply_header_sort(&refs.grid, &definition.sorts);
    refs.syncing_header_sort.set(false);
}

/// Appends the trailing per-row document-properties action.
fn append_actions_column(grid: &gtk::ColumnView, dispatcher: &AppDispatcher) {
    let factory = gtk::SignalListItemFactory::new();
    let dispatcher_for_setup = dispatcher.clone();
    factory.connect_setup(move |_, item| {
        let Some(item) = item.downcast_ref::<gtk::ListItem>() else {
            return;
        };
        let button = gtk::Button::from_icon_name("document-properties-symbolic");
        button.add_css_class("flat");
        button.set_widget_name("base-row-properties");
        button.set_tooltip_text(Some(&gettext("Edit document properties")));
        button.set_halign(gtk::Align::Center);
        let dispatcher = dispatcher_for_setup.clone();
        let weak_item = item.downgrade();
        button.connect_clicked(move |_| {
            let Some(item) = weak_item.upgrade() else {
                return;
            };
            let Some(row) = item.item().and_downcast::<glib::BoxedAnyObject>() else {
                return;
            };
            let row = row.borrow::<BaseRow>();
            let _ = dispatcher.dispatch(AppMsg::Bases(BasesMsg::EditProperties {
                note_id: row.note_id,
                revision: row.revision,
            }));
        });
        item.set_child(Some(&button));
    });
    let column = gtk::ColumnViewColumn::new(None, Some(factory));
    column.set_id(Some("base-row-actions"));
    column.set_resizable(false);
    column.set_expand(false);
    grid.append_column(&column);
}

fn append_rows(refs: &BaseViewRefs, rows: &[BaseRow]) {
    let incoming: Vec<_> = rows.iter().map(|row| (row.note_id, row.revision)).collect();
    let mut rendered = refs.rendered_rows.borrow_mut();
    let appends_existing_rows = rows.len() >= rendered.len()
        && incoming
            .get(..rendered.len())
            .is_some_and(|prefix| prefix == rendered.as_slice());
    if !appends_existing_rows {
        refs.rows.remove_all();
        rendered.clear();
    }
    for row in &rows[rendered.len()..] {
        refs.rows.append(&glib::BoxedAnyObject::new(row.clone()));
    }
    *rendered = incoming;
}

fn append_column(
    grid: &gtk::ColumnView,
    column: &BaseColumn,
    title: &str,
    editor: Option<CellEditor>,
    dispatcher: &AppDispatcher,
) {
    let factory = gtk::SignalListItemFactory::new();
    let column_for_setup = column.clone();
    let editor_for_setup = editor;
    let editor_for_bind = editor_for_setup.clone();
    let dispatcher_for_setup = dispatcher.clone();
    factory.connect_setup(move |_, item| {
        let Some(item) = item.downcast_ref::<gtk::ListItem>() else {
            return;
        };
        // A boolean is always visible: the switch itself is the cell, and toggling saves.
        if matches!(editor_for_setup, Some(CellEditor::Boolean)) {
            item.set_child(Some(&boolean_cell_for(
                item,
                &column_for_setup,
                &dispatcher_for_setup,
            )));
            return;
        }
        // A single-select list is always visible too: the dropdown itself is the cell.
        if let Some(CellEditor::List {
            options,
            multiple: false,
        }) = &editor_for_setup
        {
            item.set_child(Some(&select_cell_for(
                item,
                &column_for_setup,
                options,
                &dispatcher_for_setup,
            )));
            return;
        }
        let open: Rc<dyn Fn()> = {
            let weak_item = item.downgrade();
            let column = column_for_setup.clone();
            let editor = editor_for_setup.clone();
            let dispatcher = dispatcher_for_setup.clone();
            Rc::new(move || open_cell_editor(&weak_item, &column, editor.as_ref(), &dispatcher))
        };
        let display = build_display_cell(&column_for_setup, item, &dispatcher_for_setup, &open);
        let cell = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        cell.set_widget_name(&format!(
            "base-cell-display:{}",
            header_column_id(&column_for_setup)
        ));
        cell.set_hexpand(true);
        if editor_for_setup.is_some() {
            cell.set_cursor_from_name(Some("pointer"));
        }
        cell.append(&display);
        // Property cells open the editor on a single click; the Name cell uses its edit button.
        if editor_for_setup.is_some() && !matches!(column_for_setup, BaseColumn::Name) {
            let gesture = gtk::GestureClick::new();
            let open = Rc::clone(&open);
            gesture.connect_pressed(move |gesture, n_press, _, _| {
                if n_press == 1 {
                    gesture.set_state(gtk::EventSequenceState::Claimed);
                    open();
                }
            });
            cell.add_controller(gesture);
        }
        item.set_child(Some(&cell));
    });
    let column_for_bind = column.clone();
    factory.connect_bind(move |_, item| {
        let Some(item) = item.downcast_ref::<gtk::ListItem>() else {
            return;
        };
        let Some(object) = item.item().and_downcast::<glib::BoxedAnyObject>() else {
            return;
        };
        let row = object.borrow::<BaseRow>();
        bind_cell(item, &row, &column_for_bind, editor_for_bind.as_ref());
    });
    let is_name = matches!(column, BaseColumn::Name);
    let column_view = gtk::ColumnViewColumn::new(Some(title), Some(factory));
    column_view.set_id(Some(&header_column_id(column)));
    column_view.set_sorter(Some(&gtk::CustomSorter::new(|_, _| gtk::Ordering::Equal)));
    column_view.set_resizable(true);
    column_view.set_expand(is_name);
    grid.append_column(&column_view);
}

/// Projects one row into an already-created cell.
fn bind_cell(
    item: &gtk::ListItem,
    row: &BaseRow,
    column: &BaseColumn,
    editor: Option<&CellEditor>,
) {
    if matches!(editor, Some(CellEditor::Boolean)) {
        if let Some(switch) = item.child().and_downcast::<gtk::Switch>() {
            switch.set_active(boolean_cell_value(row, column));
        }
        return;
    }
    if let Some(CellEditor::List {
        options,
        multiple: false,
    }) = editor
    {
        if let Some(dropdown) = item.child().and_downcast::<gtk::DropDown>() {
            bind_select_cell(&dropdown, row, column, options);
        }
        return;
    }
    let Some(child) = item.child() else {
        return;
    };
    if let Some(label) = find_value_label(&child) {
        label.set_text(&row_value(row, column));
    }
    if let Some(button) = find_open_note_button(&child) {
        button.set_widget_name(&format!("base-note:{}", row.note_id));
    }
}

/// Builds an always-visible boolean cell that saves a toggle immediately.
fn boolean_cell_for(
    item: &gtk::ListItem,
    column: &BaseColumn,
    dispatcher: &AppDispatcher,
) -> gtk::Switch {
    let switch = build_boolean_cell(false, "cell-boolean");
    let weak_item = item.downgrade();
    let column = column.clone();
    let dispatcher = dispatcher.clone();
    switch.connect_state_set(move |_, active| {
        let Some(item) = weak_item.upgrade() else {
            return glib::Propagation::Proceed;
        };
        let Some(object) = item.item().and_downcast::<glib::BoxedAnyObject>() else {
            return glib::Propagation::Proceed;
        };
        let row = object.borrow::<BaseRow>();
        if boolean_cell_value(&row, &column) != active {
            let _ = dispatcher.dispatch(AppMsg::Bases(BasesMsg::CommitCellEdit {
                note_id: row.note_id,
                path: column_edit_path(&column),
                revision: row.revision,
                value: Some(serde_json::Value::Bool(active)),
            }));
        }
        glib::Propagation::Proceed
    });
    switch
}

/// Builds an always-visible single-select dropdown that saves a change immediately.
fn select_cell_for(
    item: &gtk::ListItem,
    column: &BaseColumn,
    options: &[String],
    dispatcher: &AppDispatcher,
) -> gtk::DropDown {
    let dropdown = build_select_cell(&FrontmatterValue::Null, options, "cell-select");
    let weak_item = item.downgrade();
    let column = column.clone();
    let options = options.to_vec();
    let dispatcher = dispatcher.clone();
    dropdown.connect_selected_notify(move |dropdown| {
        let Some(item) = weak_item.upgrade() else {
            return;
        };
        let Some(object) = item.item().and_downcast::<glib::BoxedAnyObject>() else {
            return;
        };
        let row = object.borrow::<BaseRow>();
        let current = cell_seed(&row, &column);
        // Include an authored value outside the configured options so it is not misrepresented.
        let effective = options_with_current(&options, &current);
        let Some(option) = effective.get(dropdown.selected() as usize) else {
            return;
        };
        if Some(option) != current_text(&current).as_ref() {
            let _ = dispatcher.dispatch(AppMsg::Bases(BasesMsg::CommitCellEdit {
                note_id: row.note_id,
                path: column_edit_path(&column),
                revision: row.revision,
                value: Some(serde_json::Value::String(option.clone())),
            }));
        }
    });
    dropdown
}

/// Seeds a single-select dropdown from the row, extending its options with an authored value.
fn bind_select_cell(
    dropdown: &gtk::DropDown,
    row: &BaseRow,
    column: &BaseColumn,
    options: &[String],
) {
    let current = cell_seed(row, column);
    let effective = options_with_current(options, &current);
    let labels: Vec<&str> = effective.iter().map(String::as_str).collect();
    dropdown.set_model(Some(&gtk::StringList::new(&labels)));
    let selected = current_text(&current)
        .and_then(|text| effective.iter().position(|option| option == &text))
        .unwrap_or(0);
    dropdown.set_selected(u32::try_from(selected).unwrap_or(0));
}

fn current_text(value: &FrontmatterValue) -> Option<String> {
    match value {
        FrontmatterValue::Text(text) => Some(text.clone()),
        _ => None,
    }
}

/// Builds the read-only presentation for a cell, including the Name open/edit controls.
fn build_display_cell(
    column: &BaseColumn,
    item: &gtk::ListItem,
    dispatcher: &AppDispatcher,
    open_editor: &Rc<dyn Fn()>,
) -> gtk::Widget {
    let label = value_label();
    match column {
        BaseColumn::Name => {
            label.add_css_class("link");
            let open = gtk::Button::new();
            open.add_css_class("flat");
            open.set_widget_name("cell-open-note");
            open.set_child(Some(&label));
            open.set_hexpand(true);
            open.set_halign(gtk::Align::Start);
            {
                let dispatcher = dispatcher.clone();
                let weak_item = item.downgrade();
                open.connect_clicked(move |_| {
                    let Some(item) = weak_item.upgrade() else {
                        return;
                    };
                    let Some(row) = item.item().and_downcast::<glib::BoxedAnyObject>() else {
                        return;
                    };
                    let note_id = { row.borrow::<BaseRow>().note_id };
                    let _ =
                        dispatcher.dispatch(AppMsg::Navigation(NavigationMsg::OpenNote(note_id)));
                });
            }
            let edit = gtk::Button::from_icon_name("document-edit-symbolic");
            edit.add_css_class("flat");
            edit.set_widget_name("cell-edit-title");
            edit.set_tooltip_text(Some(&gettext("Edit title")));
            edit.set_valign(gtk::Align::Center);
            {
                let open_editor = Rc::clone(open_editor);
                edit.connect_clicked(move |_| open_editor());
            }
            let row = gtk::Box::new(gtk::Orientation::Horizontal, 0);
            row.set_hexpand(true);
            row.append(&open);
            row.append(&edit);
            row.upcast()
        }
        BaseColumn::Category | BaseColumn::Updated | BaseColumn::Property(_) => label.upcast(),
    }
}

/// Opens the modal editor for a cell, seeded from the row's current value.
fn open_cell_editor(
    weak_item: &glib::WeakRef<gtk::ListItem>,
    column: &BaseColumn,
    editor: Option<&CellEditor>,
    dispatcher: &AppDispatcher,
) {
    let Some(editor) = editor else {
        return;
    };
    let Some(item) = weak_item.upgrade() else {
        return;
    };
    let Some(object) = item.item().and_downcast::<glib::BoxedAnyObject>() else {
        return;
    };
    let Some(anchor) = item.child() else {
        return;
    };
    let (note_id, revision, seed) = {
        let row = object.borrow::<BaseRow>();
        (row.note_id, row.revision, cell_seed(&row, column))
    };
    show_cell_editor(
        &anchor,
        dispatcher,
        CellEdit {
            note_id,
            revision,
            path: &column_edit_path(column),
            title: &cell_title(column),
            editor,
            seed: &seed,
        },
    );
}

/// Returns the dialog title for an editable cell.
fn cell_title(column: &BaseColumn) -> String {
    match column {
        BaseColumn::Name => gettext("Title"),
        BaseColumn::Property(path) => path.0.trim_start_matches('/').to_owned(),
        BaseColumn::Category => gettext("Category"),
        BaseColumn::Updated => gettext("Updated"),
    }
}

fn value_label() -> gtk::Label {
    let label = gtk::Label::new(None);
    label.set_widget_name("cell-value-label");
    label.set_hexpand(true);
    label.set_xalign(0.0);
    label.set_margin_start(10);
    label.set_margin_end(10);
    label.set_margin_top(7);
    label.set_margin_bottom(7);
    label.set_ellipsize(gtk::pango::EllipsizeMode::End);
    label
}

/// Returns the value a cell editor starts from.
fn cell_seed(row: &BaseRow, column: &BaseColumn) -> FrontmatterValue {
    match column {
        BaseColumn::Name => FrontmatterValue::Text(row.name.clone()),
        BaseColumn::Property(path) => row
            .properties
            .pointer(&path.0)
            .map_or(FrontmatterValue::Null, FrontmatterValue::from_json),
        BaseColumn::Category | BaseColumn::Updated => FrontmatterValue::Null,
    }
}

/// Returns the boolean a toggle cell shows for a row.
fn boolean_cell_value(row: &BaseRow, column: &BaseColumn) -> bool {
    let BaseColumn::Property(path) = column else {
        return false;
    };
    row.properties.pointer(&path.0) == Some(&serde_json::Value::Bool(true))
}

/// Returns the frontmatter path a cell edits.
fn column_edit_path(column: &BaseColumn) -> String {
    match column {
        BaseColumn::Name => "/title".to_owned(),
        BaseColumn::Property(path) => path.0.clone(),
        BaseColumn::Category | BaseColumn::Updated => String::new(),
    }
}

fn find_value_label(widget: &gtk::Widget) -> Option<gtk::Label> {
    if let Some(label) = widget.downcast_ref::<gtk::Label>()
        && label.widget_name() == "cell-value-label"
    {
        return Some(label.clone());
    }
    let mut child = widget.first_child();
    while let Some(current) = child {
        if let Some(found) = find_value_label(&current) {
            return Some(found);
        }
        child = current.next_sibling();
    }
    None
}

fn find_open_note_button(widget: &gtk::Widget) -> Option<gtk::Button> {
    if let Some(button) = widget.downcast_ref::<gtk::Button>() {
        let name = button.widget_name();
        if name == "cell-open-note" || name.starts_with("base-note:") {
            return Some(button.clone());
        }
    }
    let mut child = widget.first_child();
    while let Some(current) = child {
        if let Some(found) = find_open_note_button(&current) {
            return Some(found);
        }
        child = current.next_sibling();
    }
    None
}

fn connect_header_sorting(
    dispatcher: &AppDispatcher,
    grid: &gtk::ColumnView,
    syncing: std::rc::Rc<std::cell::Cell<bool>>,
) {
    let Some(sorter) = grid.sorter().and_downcast::<gtk::ColumnViewSorter>() else {
        return;
    };
    let dispatcher = dispatcher.clone();
    sorter.connect_changed(move |sorter, _| {
        if syncing.get() {
            return;
        }
        let sorts = header_sorts(sorter);
        let _ = dispatcher.dispatch(AppMsg::Bases(BasesMsg::SetSorts { sorts }));
    });
}

fn apply_header_sort(grid: &gtk::ColumnView, sorts: &[BaseSort]) {
    grid.sort_by_column(None, gtk::SortType::Ascending);
    // `sort_by_column` promotes the given field to primary, so restore ties first.
    for sort in sorts.iter().rev() {
        let column = grid
            .columns()
            .iter::<gtk::ColumnViewColumn>()
            .filter_map(Result::ok)
            .find(|column| {
                column
                    .id()
                    .as_deref()
                    .and_then(header_column_from_id)
                    .as_ref()
                    == Some(&sort.field)
            });
        let Some(column) = column else {
            continue;
        };
        let direction = match sort.direction {
            BaseSortDirection::Ascending => gtk::SortType::Ascending,
            BaseSortDirection::Descending => gtk::SortType::Descending,
        };
        grid.sort_by_column(Some(&column), direction);
    }
}

fn header_sorts(sorter: &gtk::ColumnViewSorter) -> Vec<BaseSort> {
    (0..sorter.n_sort_columns())
        .filter_map(|index| {
            let (column, direction) = sorter.nth_sort_column(index);
            let field = column?.id().as_deref().and_then(header_column_from_id)?;
            let direction = match direction {
                gtk::SortType::Ascending => BaseSortDirection::Ascending,
                gtk::SortType::Descending => BaseSortDirection::Descending,
                _ => return None,
            };
            Some(BaseSort { field, direction })
        })
        .collect()
}

fn header_column_id(column: &BaseColumn) -> String {
    match column {
        BaseColumn::Name => "name".to_owned(),
        BaseColumn::Category => "category".to_owned(),
        BaseColumn::Updated => "updated".to_owned(),
        BaseColumn::Property(path) => format!("property:{}", path.0),
    }
}

fn header_column_from_id(id: &str) -> Option<BaseColumn> {
    match id {
        "name" => Some(BaseColumn::Name),
        "category" => Some(BaseColumn::Category),
        "updated" => Some(BaseColumn::Updated),
        _ => id
            .strip_prefix("property:")
            .map(|path| BaseColumn::Property(carver_sdk::PropertyPath(path.to_owned()))),
    }
}

fn row_value(row: &BaseRow, column: &BaseColumn) -> String {
    match column {
        BaseColumn::Name => row.name.clone(),
        BaseColumn::Category => row.category.clone(),
        BaseColumn::Updated => row.updated.clone(),
        BaseColumn::Property(path) => row
            .properties
            .pointer(&path.0)
            .map(display_value)
            .unwrap_or_default(),
    }
}

fn display_value(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Null => String::new(),
        serde_json::Value::String(value) => value.clone(),
        serde_json::Value::Array(values) => values
            .iter()
            .map(display_value)
            .collect::<Vec<_>>()
            .join(", "),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use carver_sdk::{NoteId, Revision};

    use super::*;

    fn row(properties: serde_json::Value) -> BaseRow {
        BaseRow {
            note_id: NoteId::new(),
            revision: Revision(3),
            name: "Roadmap".to_owned(),
            category: "Projects".to_owned(),
            updated: "2026-09-09T12:00:00Z".to_owned(),
            properties,
        }
    }

    #[test]
    fn row_value_should_project_every_builtin_and_property_shape() {
        let row = row(serde_json::json!({
            "owner": {"name": "Ada"},
            "tags": ["rust", 2, null],
            "done": true,
            "empty": null
        }));

        let property = |path: &str| BaseColumn::Property(carver_sdk::PropertyPath(path.to_owned()));
        assert_eq!(row_value(&row, &BaseColumn::Name), "Roadmap");
        assert_eq!(row_value(&row, &BaseColumn::Category), "Projects");
        assert_eq!(
            row_value(&row, &BaseColumn::Updated),
            "2026-09-09T12:00:00Z"
        );
        assert_eq!(row_value(&row, &property("/owner/name")), "Ada");
        assert_eq!(row_value(&row, &property("/tags")), "rust, 2, ");
        assert_eq!(row_value(&row, &property("/done")), "true");
        assert_eq!(row_value(&row, &property("/empty")), "");
        assert_eq!(row_value(&row, &property("/missing")), "");
    }

    #[test]
    fn cell_seed_should_project_the_current_value() {
        let row = row(serde_json::json!({"status": "ready", "count": 3}));
        assert_eq!(
            cell_seed(&row, &BaseColumn::Name),
            FrontmatterValue::Text("Roadmap".to_owned())
        );
        assert_eq!(
            cell_seed(
                &row,
                &BaseColumn::Property(carver_sdk::PropertyPath("/status".to_owned()))
            ),
            FrontmatterValue::Text("ready".to_owned())
        );
        assert_eq!(
            cell_seed(
                &row,
                &BaseColumn::Property(carver_sdk::PropertyPath("/missing".to_owned()))
            ),
            FrontmatterValue::Null
        );
    }

    #[test]
    fn column_edit_path_should_map_name_to_the_reserved_title() {
        assert_eq!(column_edit_path(&BaseColumn::Name), "/title");
        assert_eq!(
            column_edit_path(&BaseColumn::Property(carver_sdk::PropertyPath(
                "/status".to_owned()
            ))),
            "/status"
        );
    }

    #[test]
    fn header_column_ids_should_round_trip_every_column_kind() {
        let columns = [
            BaseColumn::Name,
            BaseColumn::Category,
            BaseColumn::Updated,
            BaseColumn::Property(carver_sdk::PropertyPath("/project/status".to_owned())),
        ];

        for column in columns {
            let id = header_column_id(&column);
            assert_eq!(header_column_from_id(&id), Some(column));
        }
    }
}
