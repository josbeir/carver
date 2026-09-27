//! Saved Base header-action interaction coverage.
use crate::mvu::{AppDispatcher, AppModel, AppRuntime, LoadState, RequestId, Route};
use crate::ui::tests::support::{
    TestResult, find_widget, run_main_context_until, run_main_context_until_for, test_state,
    widget_as,
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
    let sidebar = crate::ui::sidebar::build_sidebar(
        &dispatcher,
        &adw::NavigationSplitView::new(),
        std::rc::Rc::new(std::cell::RefCell::new(None)),
    );
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
            property_type: carver_domain::PropertyType::Text,
            example: Some("high".to_owned()),
        },
        carver_domain::PropertyDescriptor {
            path: carver_domain::PropertyPath("/owner".to_owned()),
            kind: carver_domain::PropertyKind::Text,
            property_type: carver_domain::PropertyType::Text,
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
    assert_eq!(dialog.content_width(), 640);
    let content = dialog.child().ok_or("configuration content")?;
    assert!(run_main_context_until(|| content.height() > 200));
    let round = widget_as::<gtk::Button>(dialog.upcast_ref(), "base-visible-field-move-up-1")
        .ok_or("reorder button")?;
    assert!(
        run_main_context_until(|| round.width() > 0 && round.width() == round.height()),
        "row controls should be circular, not stretched: {}x{}",
        round.width(),
        round.height()
    );
    assert!(
        widget_as::<adw::PreferencesGroup>(dialog.upcast_ref(), "base-visible-fields-section")
            .is_some()
    );
    assert!(
        widget_as::<adw::PreferencesGroup>(dialog.upcast_ref(), "base-filters-section").is_some()
    );
    assert!(widget_as::<adw::PreferencesGroup>(dialog.upcast_ref(), "base-sort-section").is_some());
    let preview = widget_as::<adw::ComboRow>(dialog.upcast_ref(), "base-filter-mode")
        .ok_or("matching combo")?;
    assert!(run_main_context_until(|| {
        preview.subtitle().as_deref() == Some("Currently matches 0 notes")
    }));
    let visible_fields =
        widget_as::<adw::PreferencesGroup>(dialog.upcast_ref(), "base-visible-fields-section")
            .ok_or("visible fields group")?;
    assert!(super::find_label(visible_fields.upcast_ref(), "Title").is_some());
    assert!(super::find_label(visible_fields.upcast_ref(), "priority").is_none());
    let add_field = widget_as::<gtk::Button>(dialog.upcast_ref(), "base-add-visible-field")
        .ok_or("add field button")?;
    assert!(super::find_label(dialog.upcast_ref(), "Add field").is_some());
    let add_filter = widget_as::<adw::ButtonRow>(dialog.upcast_ref(), "base-add-filter")
        .ok_or("add filter button")?;
    assert!(super::find_label(add_filter.upcast_ref(), "Add filter").is_some());
    let add_sort = widget_as::<adw::ButtonRow>(dialog.upcast_ref(), "base-add-sort")
        .ok_or("add sort button")?;
    assert!(super::find_label(add_sort.upcast_ref(), "Add sort rule").is_some());
    add_filter.emit_by_name::<()>("activated", &[]);
    widget_as::<gtk::Button>(dialog.upcast_ref(), "base-rule-filter-up-1")
        .ok_or("move added filter up")?
        .emit_clicked();
    widget_as::<gtk::Button>(dialog.upcast_ref(), "base-rule-filter-down-1")
        .ok_or("move added filter down")?
        .emit_clicked();
    widget_as::<gtk::Button>(dialog.upcast_ref(), "base-rule-filter-remove-1")
        .ok_or("remove added filter")?
        .emit_clicked();
    add_sort.emit_by_name::<()>("activated", &[]);
    widget_as::<gtk::Button>(dialog.upcast_ref(), "base-rule-sort-rule-up-1")
        .ok_or("move added sort rule up")?
        .emit_clicked();
    assert!(
        !widget_as::<adw::ExpanderRow>(dialog.upcast_ref(), "base-filter-rule-0")
            .ok_or("filter rule")?
            .uses_markup(),
        "filter summaries carry user field labels and operators as plain text"
    );
    assert!(
        !widget_as::<adw::ExpanderRow>(dialog.upcast_ref(), "base-sort-rule-1")
            .ok_or("sort rule")?
            .uses_markup(),
        "sort summaries carry user field labels as plain text"
    );
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
        preview.subtitle().as_deref() == Some("Currently matches 1 note")
    }));
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
        widget_as::<adw::EntryRow>(failed_dialog.upcast_ref(), "base-configuration-name")
            .ok_or("preserved draft")?
            .is_sensitive()
    );
    assert!(save.is_sensitive());
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
    let (dialog, _form) = crate::ui::bases::actions::show_configuration_dialog(
        &gtk_parent,
        &AppDispatcher::default(),
        RequestId(1),
        &definition,
        &[],
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
    crate::ui::bases::render_base(&refs, &definition, &[row], &[], &[], &dispatcher);
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
        std::rc::Rc::new(std::cell::RefCell::new(None)),
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

/// Exercises the interactive filter, sort, and visible-field controls.
#[expect(
    clippy::too_many_lines,
    reason = "one scenario drives every Base rule control in sequence"
)]
pub(super) fn base_rule_controls_should_edit_rules_and_fields() -> TestResult {
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
        field: carver_sdk::BaseColumn::Category,
        operator: carver_sdk::BaseFilterOperator::Equals,
        value: Some(serde_json::Value::String("Notes".to_owned())),
    });
    base.sorts.push(carver_sdk::BaseSort {
        field: carver_sdk::BaseColumn::Updated,
        direction: carver_sdk::BaseSortDirection::Descending,
    });
    let dispatcher = AppDispatcher::default();
    let (base_widget, refs) = crate::ui::bases::build_base(
        &dispatcher,
        &adw::NavigationSplitView::new(),
        &Rc::new(Cell::new(false)),
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
    let mut model = AppModel::new(&Config::default());
    let base_id = base.id;
    model.bases.definitions.state = LoadState::Ready(vec![base]);
    model.bases.selected = Some(base_id);
    model.bases.property_descriptors.state = LoadState::Ready(vec![
        carver_domain::PropertyDescriptor {
            path: carver_domain::PropertyPath("/priority".to_owned()),
            kind: carver_domain::PropertyKind::Text,
            property_type: carver_domain::PropertyType::Text,
            example: Some("high".to_owned()),
        },
        carver_domain::PropertyDescriptor {
            path: carver_domain::PropertyPath("/owner".to_owned()),
            kind: carver_domain::PropertyKind::Text,
            property_type: carver_domain::PropertyType::Text,
            example: Some("Ada".to_owned()),
        },
    ]);
    model.bases.rows.state = LoadState::Ready(Vec::new());
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
    let dialog = window.visible_dialog().ok_or("configuration dialog")?;

    // Change the filter field through its picker, then its operator.
    let filter_rule = widget_as::<adw::ExpanderRow>(dialog.upcast_ref(), "base-filter-rule-0")
        .ok_or("filter rule")?;
    filter_rule.set_expanded(true);
    let filter_field = widget_as::<gtk::Button>(dialog.upcast_ref(), "base-rule-filter-field-0")
        .ok_or("filter field picker")?;
    assert!(run_main_context_until(|| filter_field.is_mapped()));
    filter_field.emit_clicked();
    let filter_search =
        widget_as::<gtk::SearchEntry>(dialog.upcast_ref(), "base-rule-filter-field-0-search")
            .ok_or("filter picker search")?;
    filter_search.set_text("priority");
    filter_search.emit_by_name::<()>("search-changed", &[]);
    let priority = super::find_label(dialog.upcast_ref(), "priority").ok_or("priority option")?;
    priority
        .ancestor(gtk::Button::static_type())
        .and_downcast::<gtk::Button>()
        .ok_or("priority option button")?
        .emit_clicked();
    widget_as::<adw::ComboRow>(dialog.upcast_ref(), "base-rule-filter-operator-0")
        .ok_or("filter operator")?
        .set_selected(1);

    // Change the sort field and direction.
    widget_as::<adw::ExpanderRow>(dialog.upcast_ref(), "base-sort-rule-0")
        .ok_or("sort rule")?
        .set_expanded(true);
    widget_as::<gtk::Button>(dialog.upcast_ref(), "base-rule-sort-field-0")
        .ok_or("sort field picker")?
        .emit_clicked();
    let sort_search =
        widget_as::<gtk::SearchEntry>(dialog.upcast_ref(), "base-rule-sort-field-0-search")
            .ok_or("sort picker search")?;
    sort_search.set_text("owner");
    sort_search.emit_by_name::<()>("search-changed", &[]);
    let owner = super::find_label(dialog.upcast_ref(), "owner").ok_or("owner option")?;
    owner
        .ancestor(gtk::Button::static_type())
        .and_downcast::<gtk::Button>()
        .ok_or("owner option button")?
        .emit_clicked();
    let direction = widget_as::<adw::ComboRow>(dialog.upcast_ref(), "base-rule-sort-direction-0")
        .ok_or("sort direction")?;
    direction.set_selected(0);
    direction.set_selected(1);

    // Reorder and remove a visible field.
    widget_as::<gtk::Button>(dialog.upcast_ref(), "base-visible-field-move-down-1")
        .ok_or("move field down")?
        .emit_clicked();
    widget_as::<gtk::Button>(dialog.upcast_ref(), "base-visible-field-remove-1")
        .ok_or("remove field")?
        .emit_clicked();
    assert!(run_main_context_until(|| {
        find_widget(dialog.upcast_ref(), "base-visible-field-2").is_none()
    }));

    // Dismiss the dialog without saving.
    dialog.close();
    assert!(run_main_context_until(|| window.visible_dialog().is_none()));
    window.close();
    Ok(())
}

/// Edits the reserved title cell through the shared editor and persists it to the note.
pub(super) fn base_grid_edits_should_persist_to_the_note() -> TestResult {
    let (_temp, client) = test_state()?;
    let category = client.create_category("Notes")?;
    let created = client.create_note(category.id)?;
    let note = client.save_note(created.id, created.revision, "# Heading\n\nBody")?;
    let dispatcher = AppDispatcher::default();
    let (base_widget, refs) = crate::ui::bases::build_base(
        &dispatcher,
        &adw::NavigationSplitView::new(),
        &Rc::new(Cell::new(false)),
    );
    let definition = carver_sdk::BaseDefinition {
        id: carver_sdk::BaseId::new(),
        name: "Projects".to_owned(),
        columns: vec![carver_sdk::BaseColumn::Property(carver_sdk::PropertyPath(
            "/status".to_owned(),
        ))],
        filter_mode: carver_sdk::BaseFilterMode::All,
        filters: Vec::new(),
        sorts: Vec::new(),
        revision: carver_sdk::Revision(1),
        row_count: 1,
    };
    let note_id = note.id;
    let row = carver_sdk::BaseRow {
        note_id,
        revision: note.revision,
        name: "Heading".to_owned(),
        category: "Notes".to_owned(),
        updated: String::new(),
        properties: serde_json::json!({}),
    };
    crate::ui::bases::render_base(
        &refs,
        &definition,
        std::slice::from_ref(&row),
        &[],
        &[],
        &dispatcher,
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
    let mut model = AppModel::new(&Config::default());
    model.route = Route::Base;
    model.bases.selected = Some(definition.id);
    model.bases.definitions.state = LoadState::Ready(vec![definition.clone()]);
    model.bases.rows.state = LoadState::Ready(vec![row.clone()]);
    let runtime = AppRuntime::new(client.clone(), model, view);
    runtime.bind_dispatcher(&dispatcher);
    let window = adw::Window::new();
    window.set_default_size(700, 500);
    window.set_content(Some(&routes));
    window.present();

    let title_edit =
        widget_as::<gtk::Button>(&base_widget, "cell-edit-title").ok_or("title edit")?;
    assert!(run_main_context_until(|| title_edit.is_mapped()));
    title_edit.emit_clicked();
    assert!(run_main_context_until(|| {
        find_widget(&base_widget, "base-cell-editor").is_some()
    }));
    let title_entry =
        widget_as::<gtk::Entry>(&base_widget, "base-cell-editor").ok_or("title entry")?;
    assert_eq!(title_entry.text(), "Heading");
    title_entry.set_text("Override");
    widget_as::<gtk::Button>(&base_widget, "base-cell-editor-done")
        .ok_or("done button")?
        .emit_clicked();

    let saved: Rc<std::cell::RefCell<Option<String>>> = Rc::new(std::cell::RefCell::new(None));
    assert!(run_main_context_until_for(
        std::time::Duration::from_secs(5),
        || {
            if let Ok(Some(note)) = client.note(note_id)
                && note.source.contains("title:")
            {
                *saved.borrow_mut() = Some(note.source);
                return true;
            }
            false
        }
    ));
    let saved_source = saved.borrow().clone().ok_or("title edit should persist")?;
    assert_eq!(
        carver_domain::derive_content(&saved_source).title,
        "Override"
    );

    window.close();
    Ok(())
}

/// Clears the reserved title through the grid so the derived heading takes over.
pub(super) fn base_grid_should_clear_the_title_override() -> TestResult {
    let (_temp, client) = test_state()?;
    let category = client.create_category("Notes")?;
    let created = client.create_note(category.id)?;
    let note = client.save_note(
        created.id,
        created.revision,
        "---\ntitle: Override\n---\n\n# Heading\n",
    )?;
    let dispatcher = AppDispatcher::default();
    let (base_widget, refs) = crate::ui::bases::build_base(
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
    let note_id = note.id;
    let row = carver_sdk::BaseRow {
        note_id,
        revision: note.revision,
        name: "Override".to_owned(),
        category: "Notes".to_owned(),
        updated: String::new(),
        properties: serde_json::json!({}),
    };
    crate::ui::bases::render_base(
        &refs,
        &definition,
        std::slice::from_ref(&row),
        &[],
        &[],
        &dispatcher,
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
    let mut model = AppModel::new(&Config::default());
    model.route = Route::Base;
    model.bases.selected = Some(definition.id);
    model.bases.definitions.state = LoadState::Ready(vec![definition.clone()]);
    model.bases.rows.state = LoadState::Ready(vec![row.clone()]);
    let runtime = AppRuntime::new(client.clone(), model, view);
    runtime.bind_dispatcher(&dispatcher);
    let window = adw::Window::new();
    window.set_default_size(700, 500);
    window.set_content(Some(&routes));
    window.present();

    let title_edit =
        widget_as::<gtk::Button>(&base_widget, "cell-edit-title").ok_or("title edit")?;
    assert!(run_main_context_until(|| title_edit.is_mapped()));
    title_edit.emit_clicked();
    assert!(run_main_context_until(|| {
        find_widget(&base_widget, "base-cell-editor").is_some()
    }));
    let title_entry =
        widget_as::<gtk::Entry>(&base_widget, "base-cell-editor").ok_or("title entry")?;
    title_entry.set_text("   ");
    widget_as::<gtk::Button>(&base_widget, "base-cell-editor-done")
        .ok_or("done button")?
        .emit_clicked();

    let cleared: Rc<std::cell::RefCell<Option<String>>> = Rc::new(std::cell::RefCell::new(None));
    assert!(run_main_context_until_for(
        std::time::Duration::from_secs(5),
        || {
            if let Ok(Some(note)) = client.note(note_id)
                && !note.source.contains("title:")
            {
                *cleared.borrow_mut() = Some(note.source);
                return true;
            }
            false
        }
    ));
    let cleared_source = cleared
        .borrow()
        .clone()
        .ok_or("title clear should persist")?;
    assert_eq!(
        carver_domain::derive_content(&cleared_source).title,
        "Heading"
    );
    window.close();
    Ok(())
}

/// Builds each grid editor and confirms it reads the typed value.
#[expect(
    clippy::too_many_lines,
    reason = "one scenario builds and reads every cell editor kind"
)]
pub(super) fn base_cell_editors_should_commit_typed_values() -> TestResult {
    use crate::ui::bases::editing::{CellEditor, CellEditorWidget, build_boolean_cell, seed_value};
    use carver_domain::FrontmatterValue;

    let text = CellEditorWidget::build(
        &CellEditor::Text,
        &FrontmatterValue::Text("seed".to_owned()),
        "base-cell-editor",
    )
    .ok_or("text editor")?;
    let text_entry = text
        .widget()
        .clone()
        .downcast::<gtk::Entry>()
        .map_err(|_| "text entry")?;
    text_entry.set_text("done");
    assert_eq!(text.value(), Ok(Some(serde_json::json!("done"))));
    text_entry.set_text("   ");
    assert_eq!(text.value(), Ok(None));

    let number = CellEditorWidget::build(
        &CellEditor::Number,
        &FrontmatterValue::Null,
        "base-cell-editor",
    )
    .ok_or("number editor")?;
    let number_entry = number
        .widget()
        .clone()
        .downcast::<gtk::Entry>()
        .map_err(|_| "number entry")?;
    number_entry.set_text("42");
    assert_eq!(number.value(), Ok(Some(serde_json::json!(42))));
    number_entry.set_text("not a number");
    assert_eq!(number.value(), Err(()));

    // A single-select list and a date picker are always-visible cells, so the popover has no
    // widget for them.
    for always_visible in [
        CellEditor::List {
            options: vec!["draft".to_owned(), "done".to_owned()],
            multiple: false,
        },
        CellEditor::Date,
    ] {
        assert!(
            CellEditorWidget::build(&always_visible, &FrontmatterValue::Null, "base-cell-editor")
                .is_none()
        );
    }

    // A multi-select list renders a checklist and reads the selected options.
    let multi = CellEditorWidget::build(
        &CellEditor::List {
            options: vec!["draft".to_owned(), "done".to_owned()],
            multiple: true,
        },
        &FrontmatterValue::List(vec![FrontmatterValue::Text("draft".to_owned())]),
        "base-cell-editor",
    )
    .ok_or("multi list editor")?;
    let container = multi
        .widget()
        .clone()
        .downcast::<gtk::Box>()
        .map_err(|_| "multi list box")?;
    assert_eq!(container.observe_children().n_items(), 2);
    assert_eq!(multi.value(), Ok(Some(serde_json::json!(["draft"]))));

    // A list without configured options is edited as comma-separated text.
    let list_text = CellEditorWidget::build(
        &CellEditor::ListText,
        &FrontmatterValue::List(vec![
            FrontmatterValue::Text("a".to_owned()),
            FrontmatterValue::Text("b".to_owned()),
        ]),
        "base-cell-editor",
    )
    .ok_or("list text editor")?;
    let list_entry = list_text
        .widget()
        .clone()
        .downcast::<gtk::Entry>()
        .map_err(|_| "list text entry")?;
    assert_eq!(list_entry.text(), "a, b");
    list_entry.set_text("x, y");
    assert_eq!(list_text.value(), Ok(Some(serde_json::json!(["x", "y"]))));
    assert_eq!(
        seed_value(
            &CellEditor::ListText,
            &FrontmatterValue::Text("solo".to_owned())
        ),
        Some(serde_json::json!(["solo"]))
    );
    assert_eq!(
        seed_value(&CellEditor::ListText, &FrontmatterValue::Null),
        None
    );

    // A boolean is always visible and reflects its seed.
    let switch = build_boolean_cell(true, "cell-boolean");
    assert!(switch.is_active());
    assert!(
        CellEditorWidget::build(
            &CellEditor::Boolean,
            &FrontmatterValue::Null,
            "base-cell-editor"
        )
        .is_none()
    );
    assert_eq!(
        seed_value(
            &CellEditor::Text,
            &FrontmatterValue::Text("seed".to_owned())
        ),
        Some(serde_json::json!("seed"))
    );
    Ok(())
}

/// Toggles an always-visible boolean cell and persists it to the note.
pub(super) fn base_grid_should_toggle_a_boolean_property() -> TestResult {
    let (_temp, client) = test_state()?;
    let category = client.create_category("Notes")?;
    let created = client.create_note(category.id)?;
    let note = client.save_note(
        created.id,
        created.revision,
        "---\ndone: false\n---\n\n# Task\n",
    )?;
    let dispatcher = AppDispatcher::default();
    let (base_widget, refs) = crate::ui::bases::build_base(
        &dispatcher,
        &adw::NavigationSplitView::new(),
        &Rc::new(Cell::new(false)),
    );
    let definition = carver_sdk::BaseDefinition {
        id: carver_sdk::BaseId::new(),
        name: "Tasks".to_owned(),
        columns: vec![carver_sdk::BaseColumn::Property(carver_sdk::PropertyPath(
            "/done".to_owned(),
        ))],
        filter_mode: carver_sdk::BaseFilterMode::All,
        filters: Vec::new(),
        sorts: Vec::new(),
        revision: carver_sdk::Revision(1),
        row_count: 1,
    };
    let note_id = note.id;
    let row = carver_sdk::BaseRow {
        note_id,
        revision: note.revision,
        name: "Task".to_owned(),
        category: "Notes".to_owned(),
        updated: String::new(),
        properties: serde_json::json!({"done": false}),
    };
    let descriptors = [carver_domain::PropertyDescriptor {
        path: carver_domain::PropertyPath("/done".to_owned()),
        kind: carver_domain::PropertyKind::Boolean,
        property_type: carver_domain::PropertyType::Boolean,
        example: Some("false".to_owned()),
    }];
    crate::ui::bases::render_base(
        &refs,
        &definition,
        std::slice::from_ref(&row),
        &descriptors,
        &[],
        &dispatcher,
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
    let mut model = AppModel::new(&Config::default());
    model.route = Route::Base;
    model.bases.selected = Some(definition.id);
    model.bases.definitions.state = LoadState::Ready(vec![definition.clone()]);
    model.bases.rows.state = LoadState::Ready(vec![row.clone()]);
    let runtime = AppRuntime::new(client.clone(), model, view);
    runtime.bind_dispatcher(&dispatcher);
    let window = adw::Window::new();
    window.set_default_size(700, 500);
    window.set_content(Some(&routes));
    window.present();

    let switch = widget_as::<gtk::Switch>(&base_widget, "cell-boolean").ok_or("boolean cell")?;
    assert!(run_main_context_until(|| switch.is_mapped()));
    assert!(!switch.is_active());
    switch.set_active(true);

    let saved: Rc<std::cell::RefCell<Option<String>>> = Rc::new(std::cell::RefCell::new(None));
    assert!(run_main_context_until_for(
        std::time::Duration::from_secs(5),
        || {
            if let Ok(Some(note)) = client.note(note_id)
                && note.source.contains("done: true")
            {
                *saved.borrow_mut() = Some(note.source);
                return true;
            }
            false
        }
    ));
    window.close();
    Ok(())
}

/// Single-clicking an editable property cell reveals its editor.
pub(super) fn clicking_a_cell_should_reveal_the_editor() -> TestResult {
    let dispatcher = AppDispatcher::default();
    let (base_widget, refs) = crate::ui::bases::build_base(
        &dispatcher,
        &adw::NavigationSplitView::new(),
        &Rc::new(Cell::new(false)),
    );
    let definition = carver_sdk::BaseDefinition {
        id: carver_sdk::BaseId::new(),
        name: "Projects".to_owned(),
        columns: vec![carver_sdk::BaseColumn::Property(carver_sdk::PropertyPath(
            "/status".to_owned(),
        ))],
        filter_mode: carver_sdk::BaseFilterMode::All,
        filters: Vec::new(),
        sorts: Vec::new(),
        revision: carver_sdk::Revision(1),
        row_count: 1,
    };
    let row = carver_sdk::BaseRow {
        note_id: carver_sdk::NoteId::new(),
        revision: carver_sdk::Revision(1),
        name: "Note".to_owned(),
        category: "Notes".to_owned(),
        updated: String::new(),
        properties: serde_json::json!({"status": "ready"}),
    };
    let descriptors = [carver_domain::PropertyDescriptor {
        path: carver_domain::PropertyPath("/status".to_owned()),
        kind: carver_domain::PropertyKind::Text,
        property_type: carver_domain::PropertyType::Text,
        example: Some("ready".to_owned()),
    }];
    crate::ui::bases::render_base(
        &refs,
        &definition,
        std::slice::from_ref(&row),
        &descriptors,
        &[],
        &dispatcher,
    );
    let window = adw::Window::new();
    window.set_default_size(700, 500);
    window.set_content(Some(&base_widget));
    window.present();

    let display =
        find_widget(&base_widget, "base-cell-display:property:/status").ok_or("cell display")?;
    assert!(run_main_context_until(|| display.is_mapped()));
    let label = find_widget(&display, "cell-value-label").ok_or("cell label")?;
    assert_eq!(
        label
            .downcast::<gtk::Label>()
            .map_err(|_| "cell label")?
            .text(),
        "ready"
    );
    let gesture = display
        .observe_controllers()
        .iter::<glib::Object>()
        .filter_map(Result::ok)
        .find_map(|object| object.downcast::<gtk::GestureClick>().ok())
        .ok_or("cell gesture")?;
    gesture.emit_by_name::<()>("pressed", &[&1i32, &0.0f64, &0.0f64]);

    assert!(run_main_context_until(|| {
        find_widget(&base_widget, "base-cell-editor").is_some()
    }));
    let entry = widget_as::<gtk::Entry>(&base_widget, "base-cell-editor").ok_or("cell editor")?;
    assert_eq!(entry.text(), "ready");
    // Enter submits the popover like the Done button.
    entry.emit_by_name::<()>("activate", &[]);
    window.close();
    Ok(())
}

/// A configured list property renders an inline dropdown and persists the selection.
pub(super) fn base_grid_list_should_offer_a_dropdown() -> TestResult {
    let (_temp, client) = test_state()?;
    let category = client.create_category("Notes")?;
    let created = client.create_note(category.id)?;
    let note = client.save_note(
        created.id,
        created.revision,
        "---\nstatus: draft\n---\n\n# Task\n",
    )?;
    let dispatcher = AppDispatcher::default();
    let (base_widget, refs) = crate::ui::bases::build_base(
        &dispatcher,
        &adw::NavigationSplitView::new(),
        &Rc::new(Cell::new(false)),
    );
    let definition = carver_sdk::BaseDefinition {
        id: carver_sdk::BaseId::new(),
        name: "Tasks".to_owned(),
        columns: vec![carver_sdk::BaseColumn::Property(carver_sdk::PropertyPath(
            "/status".to_owned(),
        ))],
        filter_mode: carver_sdk::BaseFilterMode::All,
        filters: Vec::new(),
        sorts: Vec::new(),
        revision: carver_sdk::Revision(1),
        row_count: 1,
    };
    let note_id = note.id;
    let row = carver_sdk::BaseRow {
        note_id,
        revision: note.revision,
        name: "Task".to_owned(),
        category: "Notes".to_owned(),
        updated: String::new(),
        properties: serde_json::json!({"status": "draft"}),
    };
    // A single-select list is stored as text, so the observation reports `Text`; the configured
    // options must still drive the cell editor.
    let descriptors = [carver_domain::PropertyDescriptor {
        path: carver_domain::PropertyPath("/status".to_owned()),
        kind: carver_domain::PropertyKind::Text,
        property_type: carver_domain::PropertyType::Text,
        example: Some("draft".to_owned()),
    }];
    let defaults = [carver_config::DocumentProperty {
        key: "status".to_owned(),
        field_type: carver_config::DocumentPropertyType::List,
        multiple: false,
        value: serde_json::json!(["draft", "done"]),
    }];
    crate::ui::bases::render_base(
        &refs,
        &definition,
        std::slice::from_ref(&row),
        &descriptors,
        &defaults,
        &dispatcher,
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
    let mut model = AppModel::new(&Config::default());
    model.route = Route::Base;
    model.bases.selected = Some(definition.id);
    model.bases.definitions.state = LoadState::Ready(vec![definition.clone()]);
    model.bases.rows.state = LoadState::Ready(vec![row.clone()]);
    let runtime = AppRuntime::new(client.clone(), model, view);
    runtime.bind_dispatcher(&dispatcher);
    let window = adw::Window::new();
    window.set_default_size(700, 500);
    window.set_content(Some(&routes));
    window.present();

    // A single-select list is always visible as a dropdown: an explicit "not set" entry plus the
    // configured options. The authored `draft` value selects its option, not the first entry.
    let dropdown =
        widget_as::<gtk::DropDown>(&base_widget, "cell-select").ok_or("list dropdown")?;
    assert!(run_main_context_until(|| dropdown.is_mapped()));
    assert_eq!(dropdown.model().map(|model| model.n_items()), Some(3));
    assert_eq!(dropdown.selected(), 1);
    dropdown.set_selected(2);

    let saved: Rc<std::cell::RefCell<Option<String>>> = Rc::new(std::cell::RefCell::new(None));
    assert!(run_main_context_until_for(
        std::time::Duration::from_secs(5),
        || {
            if let Ok(Some(note)) = client.note(note_id)
                && note.source.contains("status")
                && note.source.contains("done")
            {
                *saved.borrow_mut() = Some(note.source);
                return true;
            }
            false
        }
    ));
    window.close();
    Ok(())
}

/// The Base properties dialog moves a note to the chosen category without leaving the Base.
pub(super) fn base_properties_should_set_the_category() -> TestResult {
    let (_temp, client) = test_state()?;
    let source = client.create_category("Notes")?;
    let destination = client.create_category("Archive")?;
    let created = client.create_note(source.id)?;
    let note = client.save_note(created.id, created.revision, "# Note\n")?;

    let dispatcher = AppDispatcher::default();
    let (base_widget, refs) = crate::ui::bases::build_base(
        &dispatcher,
        &adw::NavigationSplitView::new(),
        &Rc::new(Cell::new(false)),
    );
    let definition = carver_sdk::BaseDefinition {
        id: carver_sdk::BaseId::new(),
        name: "Notes".to_owned(),
        columns: Vec::new(),
        filter_mode: carver_sdk::BaseFilterMode::All,
        filters: Vec::new(),
        sorts: Vec::new(),
        revision: carver_sdk::Revision(1),
        row_count: 1,
    };
    let note_id = note.id;
    let row = carver_sdk::BaseRow {
        note_id,
        revision: note.revision,
        name: "Note".to_owned(),
        category: "Notes".to_owned(),
        updated: String::new(),
        properties: serde_json::json!({}),
    };
    crate::ui::bases::render_base(
        &refs,
        &definition,
        std::slice::from_ref(&row),
        &[],
        &[],
        &dispatcher,
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
    let mut model = AppModel::new(&Config::default());
    model.route = Route::Base;
    model.bases.selected = Some(definition.id);
    model.bases.definitions.state = LoadState::Ready(vec![definition.clone()]);
    model.bases.rows.state = LoadState::Ready(vec![row.clone()]);
    model.sidebar.state = LoadState::Ready(vec![
        carver_sdk::CategorySummary {
            category: source,
            note_count: 1,
        },
        carver_sdk::CategorySummary {
            category: destination.clone(),
            note_count: 0,
        },
    ]);
    let runtime = AppRuntime::new(client.clone(), model, view);
    runtime.bind_dispatcher(&dispatcher);
    let window = adw::Window::new();
    window.set_default_size(700, 500);
    window.set_content(Some(&routes));
    window.present();

    let properties =
        widget_as::<gtk::Button>(&base_widget, "base-row-properties").ok_or("properties button")?;
    assert!(run_main_context_until(|| properties.is_mapped()));
    properties.emit_clicked();
    assert!(run_main_context_until(|| window.visible_dialog().is_some()));
    let dialog = window.visible_dialog().ok_or("properties dialog")?;
    let dialog: gtk::Widget = dialog.upcast();
    let combo =
        widget_as::<adw::ComboRow>(&dialog, "document-properties-category").ok_or("category")?;
    assert_eq!(combo.model().map(|model| model.n_items()), Some(2));
    assert_eq!(combo.selected(), 0);
    combo.set_selected(1);
    widget_as::<gtk::Button>(&dialog, "document-properties-save")
        .ok_or("save button")?
        .emit_clicked();

    assert!(run_main_context_until_for(
        std::time::Duration::from_secs(5),
        || {
            client
                .note(note_id)
                .ok()
                .flatten()
                .is_some_and(|note| note.category_id == destination.id)
        }
    ));
    assert_eq!(runtime.model().route, Route::Base);
    window.close();
    Ok(())
}

/// A date property cell shows a picker icon instead of a two-step popover.
pub(super) fn base_grid_date_should_expose_a_picker_icon() -> TestResult {
    let dispatcher = AppDispatcher::default();
    let (base_widget, refs) = crate::ui::bases::build_base(
        &dispatcher,
        &adw::NavigationSplitView::new(),
        &Rc::new(Cell::new(false)),
    );
    let definition = carver_sdk::BaseDefinition {
        id: carver_sdk::BaseId::new(),
        name: "Projects".to_owned(),
        columns: vec![carver_sdk::BaseColumn::Property(carver_sdk::PropertyPath(
            "/due".to_owned(),
        ))],
        filter_mode: carver_sdk::BaseFilterMode::All,
        filters: Vec::new(),
        sorts: Vec::new(),
        revision: carver_sdk::Revision(1),
        row_count: 1,
    };
    let row = carver_sdk::BaseRow {
        note_id: carver_sdk::NoteId::new(),
        revision: carver_sdk::Revision(1),
        name: "Note".to_owned(),
        category: "Notes".to_owned(),
        updated: String::new(),
        properties: serde_json::json!({"due": "2026-09-27"}),
    };
    let descriptors = [carver_domain::PropertyDescriptor {
        path: carver_domain::PropertyPath("/due".to_owned()),
        kind: carver_domain::PropertyKind::Text,
        property_type: carver_domain::PropertyType::Date,
        example: Some("2026-09-27".to_owned()),
    }];
    crate::ui::bases::render_base(
        &refs,
        &definition,
        std::slice::from_ref(&row),
        &descriptors,
        &[],
        &dispatcher,
    );
    // The derived-title column is labelled Title, matching the properties dialog.
    let header = refs
        .grid
        .columns()
        .item(0)
        .and_downcast::<gtk::ColumnViewColumn>()
        .ok_or("name column")?;
    let expected = gettextrs::gettext("Title");
    assert_eq!(header.title().as_deref(), Some(expected.as_str()));

    let window = adw::Window::new();
    window.set_default_size(700, 500);
    window.set_content(Some(&base_widget));
    window.present();

    let display = find_widget(&base_widget, "base-cell-display:property:/due").ok_or("cell")?;
    assert!(run_main_context_until(|| display.is_mapped()));
    let label = find_widget(&display, "cell-value-label").ok_or("value label")?;
    assert!(
        !label
            .downcast::<gtk::Label>()
            .map_err(|_| "label")?
            .text()
            .is_empty()
    );
    // Opening the picker reseeds it from the row, which must not re-enter while applying.
    let picker = find_widget(&display, "cell-date-picker").ok_or("date picker")?;
    let picker = picker
        .downcast::<gtk::MenuButton>()
        .map_err(|_| "picker button")?;
    picker.popup();
    assert!(run_main_context_until(|| picker.is_visible()));
    window.close();
    Ok(())
}

/// Opens the popover editor for a property cell by synthesizing its click.
fn open_property_cell_editor(base: &gtk::Widget, cell: &str) -> TestResult {
    let display =
        find_widget(base, &format!("base-cell-display:property:{cell}")).ok_or("cell display")?;
    assert!(
        run_main_context_until(|| display.is_mapped()),
        "cell {cell} should be mapped"
    );
    let gesture = display
        .observe_controllers()
        .iter::<glib::Object>()
        .filter_map(Result::ok)
        .find_map(|object| object.downcast::<gtk::GestureClick>().ok())
        .ok_or("cell gesture")?;
    gesture.emit_by_name::<()>("pressed", &[&1i32, &0.0f64, &0.0f64]);
    assert!(run_main_context_until(|| {
        find_widget(base, "base-cell-editor").is_some()
    }));
    Ok(())
}

/// A Base over one real note and one property column, kept alive for interaction tests.
struct GridCellFixture {
    _temp: tempfile::TempDir,
    client: crate::ui::tests::support::TestLibraryClient,
    base_widget: gtk::Widget,
    window: adw::Window,
    note_id: carver_sdk::NoteId,
    _runtime: AppRuntime<carver_storage_sqlite::SqliteLibrary>,
}

/// Builds a Base with a single editable property column over a real note.
fn grid_cell_fixture(
    source: &str,
    column: &str,
    properties: serde_json::Value,
    descriptor: carver_domain::PropertyDescriptor,
) -> Result<GridCellFixture, Box<dyn std::error::Error>> {
    let (temp, client) = test_state()?;
    let category = client.create_category("Notes")?;
    let created = client.create_note(category.id)?;
    let note = client.save_note(created.id, created.revision, source)?;
    let dispatcher = AppDispatcher::default();
    let (base_widget, refs) = crate::ui::bases::build_base(
        &dispatcher,
        &adw::NavigationSplitView::new(),
        &Rc::new(Cell::new(false)),
    );
    let definition = carver_sdk::BaseDefinition {
        id: carver_sdk::BaseId::new(),
        name: "Tasks".to_owned(),
        columns: vec![carver_sdk::BaseColumn::Property(carver_sdk::PropertyPath(
            column.to_owned(),
        ))],
        filter_mode: carver_sdk::BaseFilterMode::All,
        filters: Vec::new(),
        sorts: Vec::new(),
        revision: carver_sdk::Revision(1),
        row_count: 1,
    };
    let note_id = note.id;
    let row = carver_sdk::BaseRow {
        note_id,
        revision: note.revision,
        name: "Task".to_owned(),
        category: "Notes".to_owned(),
        updated: String::new(),
        properties,
    };
    let descriptors = [descriptor];
    crate::ui::bases::render_base(
        &refs,
        &definition,
        std::slice::from_ref(&row),
        &descriptors,
        &[],
        &dispatcher,
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
    let mut model = AppModel::new(&Config::default());
    model.route = Route::Base;
    model.bases.selected = Some(definition.id);
    model.bases.definitions.state = LoadState::Ready(vec![definition.clone()]);
    model.bases.rows.state = LoadState::Ready(vec![row.clone()]);
    let runtime = AppRuntime::new(client.clone(), model, view);
    runtime.bind_dispatcher(&dispatcher);
    let window = adw::Window::new();
    window.set_default_size(700, 500);
    window.set_content(Some(&routes));
    window.present();
    Ok(GridCellFixture {
        _temp: temp,
        client,
        base_widget,
        window,
        note_id,
        _runtime: runtime,
    })
}

/// Invalid input keeps the editor open, and Escape cancels without committing.
pub(super) fn base_cell_editor_should_reject_invalid_input_and_escape() -> TestResult {
    let fixture = grid_cell_fixture(
        "---\npoints: 3\n---\n\n# Task\n",
        "/points",
        serde_json::json!({"points": 3}),
        carver_domain::PropertyDescriptor {
            path: carver_domain::PropertyPath("/points".to_owned()),
            kind: carver_domain::PropertyKind::Number,
            property_type: carver_domain::PropertyType::Number,
            example: Some("3".to_owned()),
        },
    )?;

    open_property_cell_editor(&fixture.base_widget, "/points")?;
    widget_as::<gtk::Entry>(&fixture.base_widget, "base-cell-editor")
        .ok_or("points entry")?
        .set_text("not a number");
    widget_as::<gtk::Button>(&fixture.base_widget, "base-cell-editor-done")
        .ok_or("done")?
        .emit_clicked();
    assert!(
        find_widget(&fixture.base_widget, "base-cell-editor").is_some(),
        "invalid input should not dismiss the editor"
    );

    let popover = widget_as::<gtk::Popover>(&fixture.base_widget, "base-cell-editor-popover")
        .ok_or("popover")?;
    let key = popover
        .observe_controllers()
        .iter::<glib::Object>()
        .filter_map(Result::ok)
        .find_map(|controller| controller.downcast::<gtk::EventControllerKey>().ok())
        .ok_or("key controller")?;
    assert!(key.emit_by_name::<bool>(
        "key-pressed",
        &[
            &gtk::gdk::Key::Escape,
            &0_u32,
            &gtk::gdk::ModifierType::empty(),
        ],
    ));
    assert!(run_main_context_until(|| {
        find_widget(&fixture.base_widget, "base-cell-editor").is_none()
    }));
    assert!(
        fixture
            .client
            .note(fixture.note_id)?
            .is_some_and(|note| note.source.contains("points: 3"))
    );

    // Cancel dismisses a changed value without persisting it.
    open_property_cell_editor(&fixture.base_widget, "/points")?;
    widget_as::<gtk::Entry>(&fixture.base_widget, "base-cell-editor")
        .ok_or("points entry")?
        .set_text("5");
    widget_as::<gtk::Button>(&fixture.base_widget, "base-cell-editor-cancel")
        .ok_or("cancel")?
        .emit_clicked();
    assert!(run_main_context_until(|| {
        find_widget(&fixture.base_widget, "base-cell-editor").is_none()
    }));
    assert!(
        fixture
            .client
            .note(fixture.note_id)?
            .is_some_and(|note| note.source.contains("points: 3"))
    );

    fixture.window.close();
    Ok(())
}

/// The editor's Clear action removes the property key.
pub(super) fn base_cell_editor_should_clear_a_value() -> TestResult {
    let fixture = grid_cell_fixture(
        "---\nstatus: ready\n---\n\n# Task\n",
        "/status",
        serde_json::json!({"status": "ready"}),
        carver_domain::PropertyDescriptor {
            path: carver_domain::PropertyPath("/status".to_owned()),
            kind: carver_domain::PropertyKind::Text,
            property_type: carver_domain::PropertyType::Text,
            example: Some("ready".to_owned()),
        },
    )?;

    open_property_cell_editor(&fixture.base_widget, "/status")?;
    widget_as::<gtk::Button>(&fixture.base_widget, "base-cell-editor-clear")
        .ok_or("clear")?
        .emit_clicked();
    assert!(run_main_context_until_for(
        std::time::Duration::from_secs(5),
        || fixture
            .client
            .note(fixture.note_id)
            .ok()
            .flatten()
            .is_some_and(|note| !note.source.contains("status:"))
    ));

    fixture.window.close();
    Ok(())
}

/// A list cell whose authored value cannot round-trip stays read-only.
pub(super) fn base_grid_should_keep_a_lossy_list_cell_read_only() -> TestResult {
    let fixture = grid_cell_fixture(
        "---\ntags: [1, ready]\n---\n\n# Task\n",
        "/tags",
        serde_json::json!({"tags": [1, "ready"]}),
        carver_domain::PropertyDescriptor {
            path: carver_domain::PropertyPath("/tags".to_owned()),
            kind: carver_domain::PropertyKind::List,
            property_type: carver_domain::PropertyType::List,
            example: Some("ready".to_owned()),
        },
    )?;

    let display = find_widget(&fixture.base_widget, "base-cell-display:property:/tags")
        .ok_or("cell display")?;
    assert!(run_main_context_until(|| display.is_mapped()));
    assert!(
        !display.is_sensitive(),
        "a list cell that cannot round-trip its value must be read-only"
    );

    let gesture = display
        .observe_controllers()
        .iter::<glib::Object>()
        .filter_map(Result::ok)
        .find_map(|object| object.downcast::<gtk::GestureClick>().ok())
        .ok_or("cell gesture")?;
    gesture.emit_by_name::<()>("pressed", &[&1i32, &0.0f64, &0.0f64]);
    assert!(
        find_widget(&fixture.base_widget, "base-cell-editor").is_none(),
        "a read-only cell must not open an editor"
    );

    fixture.window.close();
    Ok(())
}

/// Clicking away from a changed cell commits its value.
pub(super) fn base_cell_editor_should_commit_on_click_away() -> TestResult {
    let fixture = grid_cell_fixture(
        "---\nstatus: ready\n---\n\n# Task\n",
        "/status",
        serde_json::json!({"status": "ready"}),
        carver_domain::PropertyDescriptor {
            path: carver_domain::PropertyPath("/status".to_owned()),
            kind: carver_domain::PropertyKind::Text,
            property_type: carver_domain::PropertyType::Text,
            example: Some("ready".to_owned()),
        },
    )?;

    open_property_cell_editor(&fixture.base_widget, "/status")?;
    widget_as::<gtk::Entry>(&fixture.base_widget, "base-cell-editor")
        .ok_or("status entry")?
        .set_text("ready2");
    widget_as::<gtk::Popover>(&fixture.base_widget, "base-cell-editor-popover")
        .ok_or("popover")?
        .popdown();
    assert!(run_main_context_until_for(
        std::time::Duration::from_secs(5),
        || fixture
            .client
            .note(fixture.note_id)
            .ok()
            .flatten()
            .is_some_and(|note| note.source.contains("ready2"))
    ));

    fixture.window.close();
    Ok(())
}

/// Choosing a day and closing the grid date picker commits the chosen value.
pub(super) fn base_grid_date_picker_should_commit_on_close() -> TestResult {
    let (_temp, client) = test_state()?;
    let category = client.create_category("Notes")?;
    let created = client.create_note(category.id)?;
    let note = client.save_note(
        created.id,
        created.revision,
        "---\ndue: 2026-09-01\n---\n\n# Task\n",
    )?;
    let dispatcher = AppDispatcher::default();
    let (base_widget, refs) = crate::ui::bases::build_base(
        &dispatcher,
        &adw::NavigationSplitView::new(),
        &Rc::new(Cell::new(false)),
    );
    let definition = carver_sdk::BaseDefinition {
        id: carver_sdk::BaseId::new(),
        name: "Tasks".to_owned(),
        columns: vec![carver_sdk::BaseColumn::Property(carver_sdk::PropertyPath(
            "/due".to_owned(),
        ))],
        filter_mode: carver_sdk::BaseFilterMode::All,
        filters: Vec::new(),
        sorts: Vec::new(),
        revision: carver_sdk::Revision(1),
        row_count: 1,
    };
    let note_id = note.id;
    let row = carver_sdk::BaseRow {
        note_id,
        revision: note.revision,
        name: "Task".to_owned(),
        category: "Notes".to_owned(),
        updated: String::new(),
        properties: serde_json::json!({"due": "2026-09-01"}),
    };
    let descriptors = [carver_domain::PropertyDescriptor {
        path: carver_domain::PropertyPath("/due".to_owned()),
        kind: carver_domain::PropertyKind::Text,
        property_type: carver_domain::PropertyType::Date,
        example: Some("2026-09-01".to_owned()),
    }];
    crate::ui::bases::render_base(
        &refs,
        &definition,
        std::slice::from_ref(&row),
        &descriptors,
        &[],
        &dispatcher,
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
    let mut model = AppModel::new(&Config::default());
    model.route = Route::Base;
    model.bases.selected = Some(definition.id);
    model.bases.definitions.state = LoadState::Ready(vec![definition.clone()]);
    model.bases.rows.state = LoadState::Ready(vec![row.clone()]);
    let runtime = AppRuntime::new(client.clone(), model, view);
    runtime.bind_dispatcher(&dispatcher);
    let window = adw::Window::new();
    window.set_default_size(700, 500);
    window.set_content(Some(&routes));
    window.present();

    let display =
        find_widget(&base_widget, "base-cell-display:property:/due").ok_or("date cell")?;
    assert!(run_main_context_until(|| display.is_mapped()));
    let picker = widget_as::<gtk::MenuButton>(&base_widget, "cell-date-picker").ok_or("picker")?;
    picker.popup();
    assert!(run_main_context_until(|| picker.is_visible()));
    let calendar =
        widget_as::<gtk::Calendar>(&base_widget, "cell-date-calendar").ok_or("calendar")?;
    calendar.set_day(15);
    calendar.emit_by_name::<()>("day-selected", &[]);
    widget_as::<gtk::Popover>(&base_widget, "cell-date-popover")
        .ok_or("popover")?
        .popdown();
    assert!(run_main_context_until_for(
        std::time::Duration::from_secs(5),
        || client
            .note(note_id)
            .ok()
            .flatten()
            .is_some_and(|note| note.source.contains("2026-09-15"))
    ));

    window.close();
    Ok(())
}

/// Confirming an unset date-time picker commits the displayed default instead of dropping it.
pub(super) fn base_grid_date_picker_should_commit_an_unset_datetime() -> TestResult {
    let fixture = grid_cell_fixture(
        "---\nstatus: ready\n---\n\n# Task\n",
        "/at",
        serde_json::json!({}),
        carver_domain::PropertyDescriptor {
            path: carver_domain::PropertyPath("/at".to_owned()),
            kind: carver_domain::PropertyKind::Text,
            property_type: carver_domain::PropertyType::DateTime,
            example: Some("2026-09-27T00:00:00Z".to_owned()),
        },
    )?;

    let picker =
        widget_as::<gtk::MenuButton>(&fixture.base_widget, "cell-date-picker").ok_or("picker")?;
    assert!(run_main_context_until(|| picker.is_mapped()));
    picker.popup();
    assert!(run_main_context_until(|| picker.is_visible()));
    // Close without moving the calendar or spinner: the displayed date must still persist, because
    // the property was absent and the picker shows a selection.
    widget_as::<gtk::Popover>(&fixture.base_widget, "cell-date-popover")
        .ok_or("popover")?
        .popdown();

    let now = glib::DateTime::now_local()?;
    let expected = format!(
        "{:04}-{:02}-{:02}",
        now.year(),
        now.month(),
        now.day_of_month()
    );
    assert!(
        run_main_context_until_for(std::time::Duration::from_secs(5), || fixture
            .client
            .note(fixture.note_id)
            .ok()
            .flatten()
            .is_some_and(|note| note.source.contains(&expected))),
        "an unset date-time should commit the displayed value when confirmed"
    );

    fixture.window.close();
    Ok(())
}

/// Clearing an unset date-time picker keeps the property absent.
pub(super) fn base_grid_cleared_unset_datetime_should_not_commit() -> TestResult {
    let fixture = grid_cell_fixture(
        "---\nstatus: ready\n---\n\n# Task\n",
        "/at",
        serde_json::json!({}),
        carver_domain::PropertyDescriptor {
            path: carver_domain::PropertyPath("/at".to_owned()),
            kind: carver_domain::PropertyKind::Text,
            property_type: carver_domain::PropertyType::DateTime,
            example: Some("2026-09-27T00:00:00Z".to_owned()),
        },
    )?;

    let picker =
        widget_as::<gtk::MenuButton>(&fixture.base_widget, "cell-date-picker").ok_or("picker")?;
    assert!(run_main_context_until(|| picker.is_mapped()));
    picker.popup();
    assert!(run_main_context_until(|| picker.is_visible()));
    widget_as::<gtk::Button>(&fixture.base_widget, "cell-date-clear")
        .ok_or("clear")?
        .emit_clicked();
    widget_as::<gtk::Popover>(&fixture.base_widget, "cell-date-popover")
        .ok_or("popover")?
        .popdown();
    // Give any commit a chance to reach storage; a cleared picker must not write the displayed day.
    let _ = run_main_context_until_for(std::time::Duration::from_millis(300), || false);
    assert!(
        fixture
            .client
            .note(fixture.note_id)?
            .is_some_and(|note| !note.source.contains("at:")),
        "clearing an unset date-time must leave the property absent"
    );

    fixture.window.close();
    Ok(())
}

/// Clear submits the removal immediately, without a separate Done press.
pub(super) fn base_grid_date_picker_clear_should_commit_immediately() -> TestResult {
    let fixture = grid_cell_fixture(
        "---\nat: 2026-09-01T00:00:00Z\n---\n\n# Task\n",
        "/at",
        serde_json::json!({"at": "2026-09-01T00:00:00Z"}),
        carver_domain::PropertyDescriptor {
            path: carver_domain::PropertyPath("/at".to_owned()),
            kind: carver_domain::PropertyKind::Text,
            property_type: carver_domain::PropertyType::DateTime,
            example: Some("2026-09-01T00:00:00Z".to_owned()),
        },
    )?;

    let picker =
        widget_as::<gtk::MenuButton>(&fixture.base_widget, "cell-date-picker").ok_or("picker")?;
    assert!(run_main_context_until(|| picker.is_mapped()));
    picker.popup();
    assert!(run_main_context_until(|| picker.is_visible()));
    // Clear alone must submit; do not press Done or close the popover by hand.
    widget_as::<gtk::Button>(&fixture.base_widget, "cell-date-clear")
        .ok_or("clear")?
        .emit_clicked();
    assert!(
        run_main_context_until_for(std::time::Duration::from_secs(5), || fixture
            .client
            .note(fixture.note_id)
            .ok()
            .flatten()
            .is_some_and(|note| !note.source.contains("at:"))),
        "pressing Clear should remove the property without pressing Done"
    );

    fixture.window.close();
    Ok(())
}
