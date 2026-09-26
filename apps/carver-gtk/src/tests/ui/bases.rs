//! Saved Base header-action interaction coverage.
use crate::mvu::{AppDispatcher, AppModel, AppRuntime, LoadState, RequestId, Route};
use crate::ui::tests::support::{
    TestResult, find_widget, run_main_context_until, test_state, widget_as,
};
use gtk::prelude::*;
use libadwaita::{self as adw, prelude::*};
use std::{cell::Cell, rc::Rc};

use carver_config::Config;

pub(super) fn delete_base_should_require_confirmation_and_keep_notes() -> TestResult {
    let (_temp, client) = test_state()?;
    let category = client.create_category("Notes")?;
    let note = client.create_note(category.id)?;
    let base = glib::MainContext::default()
        .block_on(client.create_base_async("Projects".to_owned(), Vec::new()))?;
    let dispatcher = AppDispatcher::default();
    let sidebar = crate::ui::sidebar::build_sidebar(&dispatcher, &adw::NavigationSplitView::new());
    let sidebar_for_view = sidebar.clone();
    let mut model = AppModel::new(&carver_config::Config::default());
    model.sidebar.state = LoadState::Ready(Vec::new());
    model.bases.definitions.state = LoadState::Ready(vec![base.clone()]);
    model.bases.selected = Some(base.id);
    model.bases.rows.state = LoadState::Ready(Vec::new());
    model.route = Route::Base;
    sidebar.render(&model);
    let routes = gtk::Stack::new();
    let (base_widget, refs) = crate::ui::bases::build_base(
        &dispatcher,
        &adw::NavigationSplitView::new(),
        &std::rc::Rc::new(std::cell::Cell::new(false)),
    );
    routes.add_named(
        &gtk::Box::new(gtk::Orientation::Vertical, 0),
        Some("browser"),
    );
    routes.add_named(&base_widget, Some("base"));
    let view = crate::view::ViewRefs::new(
        routes.clone(),
        adw::StatusPage::new(),
        adw::StatusPage::new(),
    )
    .with_dispatcher(dispatcher.clone())
    .with_base(refs)
    .with_sidebar_renderer(move |model| sidebar_for_view.render(model));
    view.render(&model);
    let runtime = AppRuntime::new(client.clone(), model, view);
    runtime.bind_dispatcher(&dispatcher);
    let window = adw::Window::new();
    window.set_default_size(500, 700);
    window.set_content(Some(&routes));
    window.present();
    let button =
        widget_as::<gtk::Button>(&base_widget, "delete-base-button").ok_or("header delete")?;
    assert_eq!(button.icon_name().as_deref(), Some("user-trash-symbolic"));
    assert!(run_main_context_until(|| button.is_mapped()));
    for response in ["cancel", "delete"] {
        button.emit_clicked();
        let dialog = window
            .visible_dialog()
            .and_downcast::<adw::AlertDialog>()
            .ok_or("confirmation")?;
        assert!(dialog.body().contains("notes"));
        dialog.emit_by_name::<()>("response", &[&response]);
        dialog.close();
        if response == "cancel" {
            assert_eq!(runtime.model().route, Route::Base);
        }
    }
    assert!(run_main_context_until(|| runtime.model().route
        == Route::Browser
        && matches!(runtime.model().bases.definitions.state, LoadState::Ready(ref items) if items.is_empty())));
    let saved = client.note(note.id)?.ok_or("preserved note")?;
    assert_eq!(saved.revision, note.revision);
    assert_eq!(saved.updated_at, note.updated_at);
    assert_eq!(saved.source, note.source);
    window.close();
    Ok(())
}

pub(super) fn base_search_should_open_and_clear_from_native_controls() -> TestResult {
    let (_temp, client) = test_state()?;
    let category = client.create_category("Notes")?;
    let _note = client.create_note(category.id)?;
    let base = glib::MainContext::default()
        .block_on(client.create_base_async("Projects".to_owned(), Vec::new()))?;
    let dispatcher = AppDispatcher::default();
    let (base_widget, refs) = crate::ui::bases::build_base(
        &dispatcher,
        &adw::NavigationSplitView::new(),
        &std::rc::Rc::new(std::cell::Cell::new(false)),
    );
    let routes = gtk::Stack::new();
    routes.add_named(&base_widget, Some("base"));
    let view = crate::view::ViewRefs::new(
        routes.clone(),
        adw::StatusPage::new(),
        adw::StatusPage::new(),
    )
    .with_dispatcher(dispatcher.clone())
    .with_base(refs);
    let mut model = AppModel::new(&carver_config::Config::default());
    model.bases.definitions.state = LoadState::Ready(vec![base.clone()]);
    model.bases.selected = Some(base.id);
    model.bases.rows.state = LoadState::Ready(Vec::new());
    model.route = Route::Base;
    view.render(&model);
    let runtime = AppRuntime::new(client, model, view);
    runtime.bind_dispatcher(&dispatcher);
    let window = adw::Window::new();
    window.set_default_size(700, 500);
    window.set_content(Some(&routes));
    window.present();

    let search_bar =
        widget_as::<gtk::SearchBar>(&base_widget, "base-search-bar").ok_or("Base search bar")?;
    let search_entry = widget_as::<gtk::SearchEntry>(&base_widget, "base-search-entry")
        .ok_or("Base search entry")?;
    let search_toggle = widget_as::<gtk::ToggleButton>(&base_widget, "base-search-toggle")
        .ok_or("Base search toggle")?;
    let controllers = base_widget.observe_controllers();
    let shortcut = (0..controllers.n_items())
        .filter_map(|index| controllers.item(index))
        .filter_map(|controller| controller.downcast::<gtk::EventControllerKey>().ok())
        .find(|controller| controller.name().as_deref() == Some("base-search-shortcut"))
        .ok_or("Base search shortcut")?;

    assert!(run_main_context_until(|| search_toggle.is_mapped()));

    let handled = shortcut.emit_by_name::<bool>(
        "key-pressed",
        &[
            &gtk::gdk::Key::f,
            &0_u32,
            &gtk::gdk::ModifierType::CONTROL_MASK,
        ],
    );
    assert!(handled);
    assert!(run_main_context_until(|| {
        search_bar.is_search_mode() && search_toggle.is_active()
    }));

    search_entry.set_text("roadmap");
    assert!(run_main_context_until(|| {
        runtime.model().bases.search_query == "roadmap"
    }));
    search_entry.emit_stop_search();
    assert!(run_main_context_until(|| {
        !search_bar.is_search_mode()
            && !search_toggle.is_active()
            && runtime.model().bases.search_query.is_empty()
    }));
    window.close();
    Ok(())
}

pub(super) fn base_header_sort_should_persist_from_native_controls() -> TestResult {
    let (_temp, client) = test_state()?;
    let base = glib::MainContext::default().block_on(client.create_base_async(
        "Projects".to_owned(),
        vec![
            carver_sdk::BaseColumn::Name,
            carver_sdk::BaseColumn::Category,
        ],
    ))?;
    let dispatcher = AppDispatcher::default();
    let (base_widget, refs) = crate::ui::bases::build_base(
        &dispatcher,
        &adw::NavigationSplitView::new(),
        &std::rc::Rc::new(std::cell::Cell::new(false)),
    );
    let grid = refs.grid.clone();
    let routes = gtk::Stack::new();
    routes.add_named(&base_widget, Some("base"));
    let view = crate::view::ViewRefs::new(
        routes.clone(),
        adw::StatusPage::new(),
        adw::StatusPage::new(),
    )
    .with_dispatcher(dispatcher.clone())
    .with_base(refs);
    let mut model = AppModel::new(&carver_config::Config::default());
    model.bases.definitions.state = LoadState::Ready(vec![base.clone()]);
    model.bases.selected = Some(base.id);
    model.bases.rows.state = LoadState::Ready(vec![carver_sdk::BaseRow {
        note_id: carver_sdk::NoteId::new(),
        revision: carver_sdk::Revision(1),
        name: "Roadmap".to_owned(),
        category: "Notes".to_owned(),
        updated: "2026-09-14T12:00:00Z".to_owned(),
        properties: serde_json::Value::Null,
    }]);
    model.route = Route::Base;
    view.render(&model);
    let runtime = AppRuntime::new(client.clone(), model, view);
    runtime.bind_dispatcher(&dispatcher);
    let window = adw::Window::new();
    window.set_default_size(700, 500);
    window.set_content(Some(&routes));
    window.present();

    let category = grid
        .columns()
        .iter::<gtk::ColumnViewColumn>()
        .filter_map(Result::ok)
        .find(|column| column.id().as_deref() == Some("category"))
        .ok_or("Category column")?;
    grid.sort_by_column(Some(&category), gtk::SortType::Ascending);
    assert!(run_main_context_until(|| {
        glib::MainContext::default()
            .block_on(client.bases_async())
            .is_ok_and(|bases| {
                bases.iter().any(|definition| {
                    definition.id == base.id
                        && definition.sorts
                            == vec![carver_sdk::BaseSort {
                                field: carver_sdk::BaseColumn::Category,
                                direction: carver_sdk::BaseSortDirection::Ascending,
                            }]
                })
            })
    }));
    assert!(
        grid.sorter()
            .and_downcast::<gtk::ColumnViewSorter>()
            .and_then(|sorter| sorter.primary_sort_column())
            .is_some_and(|column| column.id().as_deref() == Some("category"))
    );
    window.close();
    Ok(())
}

#[expect(
    clippy::too_many_lines,
    reason = "The display-backed scenario exercises the complete configuration flow"
)]
pub(super) fn configure_base_should_keep_the_form_in_the_scroll_viewport() -> TestResult {
    let (_temp, client) = test_state()?;
    let category = client.create_category("Notes")?;
    let _note = client.create_note(category.id)?;
    let mut base = glib::MainContext::default().block_on(client.create_base_async(
        "Projects".to_owned(),
        vec![
            carver_sdk::BaseColumn::Name,
            carver_sdk::BaseColumn::Category,
            carver_sdk::BaseColumn::Updated,
        ],
    ))?;
    base.filters.push(carver_sdk::BaseFilter {
        field: carver_sdk::BaseColumn::Property(carver_domain::PropertyPath("/status".to_owned())),
        operator: carver_sdk::BaseFilterOperator::Equals,
        value: Some(serde_json::Value::String("ready".to_owned())),
    });
    base.sorts.push(carver_sdk::BaseSort {
        field: carver_sdk::BaseColumn::Category,
        direction: carver_sdk::BaseSortDirection::Ascending,
    });
    let dispatcher = AppDispatcher::default();
    let (base_widget, refs) = crate::ui::bases::build_base(
        &dispatcher,
        &adw::NavigationSplitView::new(),
        &std::rc::Rc::new(std::cell::Cell::new(false)),
    );
    let routes = gtk::Stack::new();
    routes.add_named(&base_widget, Some("base"));
    let view = crate::view::ViewRefs::new(
        routes.clone(),
        adw::StatusPage::new(),
        adw::StatusPage::new(),
    )
    .with_dispatcher(dispatcher.clone())
    .with_base(refs);
    let mut model = AppModel::new(&carver_config::Config::default());
    let base_id = base.id;
    model.bases.definitions.state = LoadState::Ready(vec![base]);
    model.bases.selected = Some(base_id);
    model.bases.property_descriptors.state = LoadState::Ready(vec![
        carver_domain::PropertyDescriptor {
            path: carver_domain::PropertyPath("/priority".to_owned()),
            kind: carver_domain::PropertyKind::Text,
            example: Some("high".to_owned()),
        },
        carver_domain::PropertyDescriptor {
            path: carver_domain::PropertyPath("/owner".to_owned()),
            kind: carver_domain::PropertyKind::Text,
            example: Some("Ada".to_owned()),
        },
    ]);
    model.bases.rows.state = LoadState::Ready(vec![carver_sdk::BaseRow {
        note_id: carver_sdk::NoteId::new(),
        revision: carver_sdk::Revision(1),
        name: "Roadmap".to_owned(),
        category: "Notes".to_owned(),
        updated: "2026-09-10T12:00:00Z".to_owned(),
        properties: serde_json::json!({"status": "blocked"}),
    }]);
    model.route = Route::Base;
    view.render(&model);
    let runtime = AppRuntime::new(client.clone(), model, view);
    runtime.bind_dispatcher(&dispatcher);
    let window = adw::Window::new();
    window.set_default_size(900, 700);
    window.set_content(Some(&routes));
    window.present();
    let button = widget_as::<gtk::Button>(&base_widget, "configure-base-button")
        .ok_or("configure button")?;
    assert!(run_main_context_until(|| button.is_mapped()));
    button.emit_clicked();
    assert!(run_main_context_until(|| window.visible_dialog().is_some()));
    let dialog = window
        .visible_dialog()
        .and_downcast::<adw::Dialog>()
        .ok_or("configuration dialog")?;
    assert!(dialog.follows_content_size());
    assert!(dialog.content_width() >= 720);
    let visible_section =
        widget_as::<gtk::Expander>(dialog.upcast_ref(), "base-visible-fields-section")
            .ok_or("visible fields section")?;
    assert!(visible_section.is_expanded());
    assert!(visible_section.resizes_toplevel());
    let filters_section = widget_as::<gtk::Expander>(dialog.upcast_ref(), "base-filters-section")
        .ok_or("filters section")?;
    assert!(filters_section.is_expanded());
    let sort_section = widget_as::<gtk::Expander>(dialog.upcast_ref(), "base-sort-section")
        .ok_or("sort section")?;
    assert!(sort_section.is_expanded());
    sort_section.emit_activate();
    assert!(!sort_section.is_expanded());
    visible_section.emit_activate();
    filters_section.emit_activate();
    assert!(
        run_main_context_until(|| dialog.content_height() < 500),
        "collapsed dialog height was {}",
        dialog.content_height()
    );
    visible_section.emit_activate();
    filters_section.emit_activate();
    sort_section.emit_activate();
    assert!(visible_section.is_expanded());
    assert!(filters_section.is_expanded());
    assert!(sort_section.is_expanded());
    let preview =
        find_widget(dialog.upcast_ref(), "base-configuration-preview").ok_or("preview label")?;
    let preview_label = preview
        .clone()
        .downcast::<gtk::Label>()
        .map_err(|_| "preview label type")?;
    assert!(run_main_context_until(|| {
        preview_label.text() == "Currently matches 0 notes"
    }));
    let visible_fields = widget_as::<gtk::Box>(dialog.upcast_ref(), "base-visible-fields-list")
        .ok_or("visible fields list")?;
    assert!(super::find_label(visible_fields.upcast_ref(), "Name").is_some());
    assert!(super::find_label(visible_fields.upcast_ref(), "priority").is_none());
    let visible_fields_parent = visible_fields.parent().ok_or("visible fields parent")?;
    assert!(
        visible_fields_parent
            .downcast::<gtk::ScrolledWindow>()
            .is_err()
    );
    let add_field = widget_as::<gtk::Button>(dialog.upcast_ref(), "base-add-visible-field")
        .ok_or("add field button")?;
    assert!(super::find_label(add_field.upcast_ref(), "Add field").is_some());
    let add_filter = widget_as::<gtk::Button>(dialog.upcast_ref(), "base-add-filter")
        .ok_or("add filter button")?;
    assert!(super::find_label(add_filter.upcast_ref(), "Add filter").is_some());
    let add_sort =
        widget_as::<gtk::Button>(dialog.upcast_ref(), "base-add-sort").ok_or("add sort button")?;
    assert!(super::find_label(add_sort.upcast_ref(), "Add sort rule").is_some());
    add_filter.emit_clicked();
    widget_as::<gtk::Button>(dialog.upcast_ref(), "base-rule-filter-up-1")
        .ok_or("move added filter up")?
        .emit_clicked();
    widget_as::<gtk::Button>(dialog.upcast_ref(), "base-rule-filter-down-1")
        .ok_or("move added filter down")?
        .emit_clicked();
    widget_as::<gtk::Button>(dialog.upcast_ref(), "base-rule-filter-remove-1")
        .ok_or("remove added filter")?
        .emit_clicked();
    add_sort.emit_clicked();
    widget_as::<gtk::Button>(dialog.upcast_ref(), "base-rule-sort-rule-up-1")
        .ok_or("move added sort rule up")?
        .emit_clicked();
    add_field.emit_clicked();
    assert!(super::find_label(dialog.upcast_ref(), "Category").is_some());
    assert!(super::find_label(dialog.upcast_ref(), "priority").is_none());
    let search =
        widget_as::<gtk::SearchEntry>(dialog.upcast_ref(), "base-add-visible-field-picker-search")
            .ok_or("field search")?;
    search.set_text("priority");
    search.emit_by_name::<()>("search-changed", &[]);
    let picker_scroll = widget_as::<gtk::ScrolledWindow>(
        dialog.upcast_ref(),
        "base-add-visible-field-picker-scroll",
    )
    .ok_or("field picker scroll view")?;
    assert!(picker_scroll.min_content_width() >= 520);
    assert!(picker_scroll.max_content_width() >= 680);
    let priority = super::find_label(dialog.upcast_ref(), "priority").ok_or("priority option")?;
    assert!(super::find_label(dialog.upcast_ref(), "Text · high").is_some());
    let priority_button = priority
        .ancestor(gtk::Button::static_type())
        .and_downcast::<gtk::Button>()
        .ok_or("priority option button")?;
    priority_button.emit_clicked();
    assert!(super::find_label(visible_fields.upcast_ref(), "priority").is_some());
    search.set_text("owner");
    search.emit_by_name::<()>("search-changed", &[]);
    let owner = super::find_label(dialog.upcast_ref(), "owner").ok_or("owner option")?;
    owner
        .ancestor(gtk::Button::static_type())
        .and_downcast::<gtk::Button>()
        .ok_or("owner option button")?
        .emit_clicked();
    widget_as::<gtk::Button>(dialog.upcast_ref(), "base-visible-field-move-up-4")
        .ok_or("move custom field up")?
        .emit_clicked();
    widget_as::<gtk::Button>(dialog.upcast_ref(), "base-visible-field-move-up-3")
        .ok_or("move custom field up")?
        .emit_clicked();
    widget_as::<gtk::Button>(dialog.upcast_ref(), "base-visible-field-move-up-2")
        .ok_or("move custom field before built-in field")?
        .emit_clicked();
    let moved_up =
        find_widget(dialog.upcast_ref(), "base-visible-field-1").ok_or("moved-up custom row")?;
    assert!(super::find_label(&moved_up, "owner").is_some());
    let remove_filter = widget_as::<gtk::Button>(dialog.upcast_ref(), "base-rule-filter-remove-0")
        .ok_or("remove filter button")?;
    remove_filter.emit_clicked();
    assert!(run_main_context_until(|| {
        preview_label.text() == "Currently matches 1 note"
    }));
    let scroll = preview
        .ancestor(gtk::ScrolledWindow::static_type())
        .and_downcast::<gtk::ScrolledWindow>()
        .ok_or("configuration scroll view")?;
    assert!(!scroll.vexpands());
    assert!(scroll.propagates_natural_height());
    assert!(scroll.propagates_natural_width());
    assert!(scroll.min_content_width() >= 720);
    assert!(scroll.min_content_height() < 0);
    assert!(scroll.max_content_height() < 0);
    let footer = widget_as::<gtk::Box>(dialog.upcast_ref(), "base-configuration-footer")
        .ok_or("configuration footer")?;
    assert!(footer.margin_top() >= 12);
    let save =
        widget_as::<gtk::Button>(dialog.upcast_ref(), "base-configuration-save").ok_or("save")?;
    save.emit_clicked();
    assert!(!dialog.can_close());
    assert!(run_main_context_until(|| !runtime
        .model()
        .bases
        .saving_configuration));
    let saved = glib::MainContext::default()
        .block_on(client.bases_async())?
        .into_iter()
        .find(|base| base.id == base_id)
        .ok_or("saved base")?;
    assert!(saved.filters.is_empty());
    assert_eq!(saved.columns.len(), 5);
    assert_eq!(
        saved.sorts,
        vec![
            carver_sdk::BaseSort {
                field: carver_sdk::BaseColumn::Updated,
                direction: carver_sdk::BaseSortDirection::Descending,
            },
            carver_sdk::BaseSort {
                field: carver_sdk::BaseColumn::Category,
                direction: carver_sdk::BaseSortDirection::Ascending,
            },
        ]
    );
    assert!(run_main_context_until(|| window.visible_dialog().is_none()));
    assert!(run_main_context_until(|| matches!(
        runtime.model().bases.rows.state,
        LoadState::Ready(_)
    )));
    button.emit_clicked();
    assert!(run_main_context_until(|| window.visible_dialog().is_some()));
    let failed_dialog = window.visible_dialog().ok_or("second configuration")?;
    // A second client changes the revision while this draft is open.
    glib::MainContext::default().block_on(client.update_base_async(
        saved.id,
        saved.revision,
        "External rename".into(),
        saved.columns.clone(),
        saved.filter_mode,
        saved.filters.clone(),
        saved.sorts.clone(),
    ))?;
    let save = widget_as::<gtk::Button>(failed_dialog.upcast_ref(), "base-configuration-save")
        .ok_or("second save")?;
    save.emit_clicked();
    assert!(run_main_context_until(|| !runtime
        .model()
        .bases
        .saving_configuration));
    assert!(runtime.model().notice.is_some());
    assert!(failed_dialog.can_close());
    assert!(
        failed_dialog
            .child()
            .ok_or("preserved draft")?
            .is_sensitive()
    );
    assert!(window.visible_dialog().is_some());
    failed_dialog.close();
    assert!(run_main_context_until(|| runtime
        .model()
        .bases
        .configuration_dialog
        .is_none()));
    window.close();
    Ok(())
}

pub(super) fn base_field_picker_should_add_a_valid_custom_path() -> TestResult {
    let parent = adw::Window::new();
    parent.set_default_size(900, 700);
    parent.present();
    let gtk_parent = parent.clone().upcast::<gtk::Window>();
    let definition = carver_sdk::BaseDefinition::defaults(
        carver_sdk::BaseId::new(),
        "Projects".to_owned(),
        vec![carver_sdk::BaseColumn::Category],
        carver_sdk::Revision(1),
    );
    let dialog = crate::ui::bases::actions::show_configuration_dialog(
        &gtk_parent,
        &AppDispatcher::default(),
        RequestId(1),
        &definition,
        &[],
    );
    assert!(run_main_context_until(|| dialog.is_mapped()));
    widget_as::<gtk::Button>(dialog.upcast_ref(), "base-add-visible-field")
        .ok_or("add field button")?
        .emit_clicked();
    let custom = super::find_label(dialog.upcast_ref(), "Add custom field…")
        .ok_or("custom field action")?
        .ancestor(gtk::Button::static_type())
        .and_downcast::<gtk::Button>()
        .ok_or("custom field button")?;
    custom.emit_clicked();
    let custom_dialog = parent
        .visible_dialog()
        .and_downcast::<adw::AlertDialog>()
        .ok_or("custom field dialog")?;
    let entry = widget_as::<gtk::Entry>(custom_dialog.upcast_ref(), "base-custom-field-entry")
        .ok_or("custom field entry")?;
    entry.set_text("project/status");
    let validation = super::find_label(
        custom_dialog.upcast_ref(),
        "Enter a valid path such as /project/status.",
    )
    .ok_or("custom path validation")?;
    assert!(validation.is_visible());
    entry.set_text("/project/status");
    assert!(!validation.is_visible());
    custom_dialog.emit_by_name::<()>("response", &[&"add"]);
    custom_dialog.close();
    assert!(run_main_context_until(|| {
        super::find_label(dialog.upcast_ref(), "project → status").is_some()
    }));
    dialog.close();
    parent.close();
    Ok(())
}
pub(super) fn assert_base_loading_delay() -> TestResult {
    use crate::mvu::{
        AppDispatcher, AppModel, AppMsg, BasesMsg, LoadState, RequestId, Route, update,
    };
    let dispatcher = AppDispatcher::default();
    let (widget, refs) = crate::ui::bases::build_base(
        &dispatcher,
        &adw::NavigationSplitView::new(),
        &Rc::new(Cell::new(false)),
    );
    let pages = refs.pages.clone();
    let routes = gtk::Stack::new();
    routes.add_named(&widget, Some("base"));
    let view = crate::view::ViewRefs::new(routes, adw::StatusPage::new(), adw::StatusPage::new())
        .with_dispatcher(dispatcher)
        .with_base(refs);
    let mut model = AppModel::new(&Config::default());
    model.route = Route::Base;
    model.bases.selected = Some(carver_sdk::BaseId::new());
    model.bases.definitions.state = LoadState::Loading(RequestId(1));
    view.render(&model);
    assert_eq!(pages.visible_child_name().as_deref(), Some("grid"));
    let _ = update(
        &mut model,
        AppMsg::Bases(BasesMsg::LoadingIndicatorElapsed(RequestId(1))),
    );
    view.render(&model);
    assert_eq!(pages.visible_child_name().as_deref(), Some("status"));
    model.bases.definitions.state = LoadState::Ready(vec![carver_sdk::BaseDefinition {
        id: model.bases.selected.ok_or("selected base")?,
        name: "Projects".to_owned(),
        columns: Vec::new(),
        filter_mode: carver_sdk::BaseFilterMode::All,
        filters: Vec::new(),
        sorts: Vec::new(),
        revision: carver_sdk::Revision(1),
        row_count: 0,
    }]);
    pages.set_visible_child_name("grid");
    model.bases.rows.state = LoadState::Loading(RequestId(2));
    view.render(&model);
    assert_eq!(pages.visible_child_name().as_deref(), Some("grid"));
    let _ = update(
        &mut model,
        AppMsg::Bases(BasesMsg::LoadingIndicatorElapsed(RequestId(2))),
    );
    view.render(&model);
    assert_eq!(pages.visible_child_name().as_deref(), Some("status"));
    Ok(())
}
pub(super) fn assert_base_note_keyboard_activation() -> TestResult {
    use crate::mvu::{AppDispatcher, AppModel, AppRuntime, Route};
    let (_temporary, client) = test_state()?;
    let category = client.create_category("Notes")?;
    let note = client.create_note(category.id)?;
    let dispatcher = AppDispatcher::default();
    let routes = gtk::Stack::new();
    for route in ["browser", "editor"] {
        routes.add_named(&gtk::Box::new(gtk::Orientation::Vertical, 0), Some(route));
    }
    let runtime = AppRuntime::new(
        client,
        AppModel::new(&Config::default()),
        crate::view::ViewRefs::new(routes, adw::StatusPage::new(), adw::StatusPage::new()),
    );
    runtime.bind_dispatcher(&dispatcher);
    let (widget, refs) = crate::ui::bases::build_base(
        &dispatcher,
        &adw::NavigationSplitView::new(),
        &Rc::new(Cell::new(false)),
    );
    let definition = carver_sdk::BaseDefinition {
        id: carver_sdk::BaseId::new(),
        name: "Projects".to_owned(),
        columns: Vec::new(),
        filter_mode: carver_sdk::BaseFilterMode::All,
        filters: Vec::new(),
        sorts: Vec::new(),
        revision: carver_sdk::Revision(1),
        row_count: 1,
    };
    let row = carver_sdk::BaseRow {
        note_id: note.id,
        revision: note.revision,
        name: "Keyboard note".to_owned(),
        category: "Notes".to_owned(),
        updated: String::new(),
        properties: serde_json::json!({}),
    };
    crate::ui::bases::render_base(&refs, &definition, &[row], &dispatcher);
    let window = gtk::Window::new();
    window.set_child(Some(&widget));
    window.present();
    let name = format!("base-note:{}", note.id);
    assert!(run_main_context_until(|| widget_as::<gtk::Button>(
        &widget, &name
    )
    .is_some_and(|button| button.grab_focus())));
    let button = widget_as::<gtk::Button>(&widget, &name).ok_or("base note button")?;
    assert!(button.activate());
    assert!(run_main_context_until(
        || runtime.model().route == Route::Editor
    ));
    window.close();
    Ok(())
}
pub(super) fn assert_base_reload_preserves_buttons() -> TestResult {
    let sidebar = crate::ui::sidebar::build_sidebar(
        &crate::mvu::AppDispatcher::default(),
        &adw::NavigationSplitView::new(),
    );
    let base = carver_sdk::BaseDefinition {
        id: carver_sdk::BaseId::new(),
        name: "Projects".to_owned(),
        columns: Vec::new(),
        filter_mode: carver_sdk::BaseFilterMode::All,
        filters: Vec::new(),
        sorts: Vec::new(),
        revision: carver_sdk::Revision(1),
        row_count: 7,
    };
    let mut model = crate::mvu::AppModel::new(&Config::default());
    model.bases.definitions.state = crate::mvu::LoadState::Ready(vec![base.clone()]);
    model.sidebar.state = crate::mvu::LoadState::Ready(Vec::new());
    sidebar.render(&model);
    let badge = format!("base-count:{}", base.id);
    let index = super::window::sidebar_item_index(&sidebar.sidebar, &badge).ok_or("base item")?;
    let item = sidebar.sidebar.item(index).ok_or("base item instance")?;
    model.route = crate::mvu::Route::Base;
    model.bases.selected = Some(base.id);
    sidebar.render(&model);
    assert_eq!(sidebar.sidebar.selected_item(), Some(item.clone()));
    model.route = crate::mvu::Route::Editor;
    model.editor_return_route = crate::mvu::Route::Base;
    sidebar.render(&model);
    assert_eq!(sidebar.sidebar.selected_item(), Some(item.clone()));
    model.route = crate::mvu::Route::Browser;
    sidebar.render(&model);
    assert_eq!(
        super::window::sidebar_selected_badge(&sidebar.sidebar).as_deref(),
        Some("all-notes-count")
    );
    for state in [
        crate::mvu::LoadState::Loading(crate::mvu::RequestId(1)),
        crate::mvu::LoadState::Failed(crate::mvu::UiError::new("offline")),
    ] {
        model.bases.definitions.state = state;
        sidebar.render(&model);
        assert_eq!(sidebar.sidebar.item(index), Some(item.clone()));
    }
    model.bases.definitions.state = crate::mvu::LoadState::Ready(Vec::new());
    sidebar.render(&model);
    assert!(super::window::sidebar_item_index(&sidebar.sidebar, &badge).is_none());
    Ok(())
}
