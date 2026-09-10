//! Saved Base header-action interaction coverage.
use crate::mvu::{AppDispatcher, AppModel, AppRuntime, LoadState, Route};
use crate::ui::tests::support::{
    TestResult, find_widget, run_main_context_until, test_state, widget_as,
};
use gtk::prelude::*;
use libadwaita::{self as adw, prelude::*};

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

#[expect(
    clippy::too_many_lines,
    reason = "The display-backed scenario exercises the complete configuration flow"
)]
pub(super) fn configure_base_should_keep_the_form_in_the_scroll_viewport() -> TestResult {
    let (_temp, client) = test_state()?;
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
        field: carver_sdk::BaseColumn::Updated,
        direction: carver_sdk::BaseSortDirection::Descending,
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
    let window = adw::Window::new();
    window.set_default_size(900, 700);
    window.set_content(Some(&routes));
    window.present();
    let button = widget_as::<gtk::Button>(&base_widget, "configure-base-button")
        .ok_or("configure button")?;
    assert!(run_main_context_until(|| button.is_mapped()));
    button.emit_clicked();
    let dialog = window
        .visible_dialog()
        .and_downcast::<adw::Dialog>()
        .ok_or("configuration dialog")?;
    let preview =
        find_widget(dialog.upcast_ref(), "base-configuration-preview").ok_or("preview label")?;
    let preview_label = preview
        .clone()
        .downcast::<gtk::Label>()
        .map_err(|_| "preview label type")?;
    assert_eq!(preview_label.text(), "Currently matches 0 notes");
    let visible_fields = widget_as::<gtk::Box>(dialog.upcast_ref(), "base-visible-fields-list")
        .ok_or("visible fields list")?;
    assert!(super::find_label(visible_fields.upcast_ref(), "Name").is_some());
    assert!(super::find_label(visible_fields.upcast_ref(), "priority").is_none());
    let add_field = widget_as::<gtk::Button>(dialog.upcast_ref(), "base-add-visible-field")
        .ok_or("add field button")?;
    add_field.emit_clicked();
    let search =
        widget_as::<gtk::SearchEntry>(dialog.upcast_ref(), "base-add-visible-field-picker-search")
            .ok_or("field search")?;
    search.set_text("priority");
    let priority = super::find_label(dialog.upcast_ref(), "priority").ok_or("priority option")?;
    assert!(super::find_label(dialog.upcast_ref(), "Text · high").is_some());
    let priority_button = priority
        .ancestor(gtk::Button::static_type())
        .and_downcast::<gtk::Button>()
        .ok_or("priority option button")?;
    priority_button.emit_clicked();
    assert!(super::find_label(visible_fields.upcast_ref(), "priority").is_some());
    search.set_text("owner");
    let owner = super::find_label(dialog.upcast_ref(), "owner").ok_or("owner option")?;
    owner
        .ancestor(gtk::Button::static_type())
        .and_downcast::<gtk::Button>()
        .ok_or("owner option button")?
        .emit_clicked();
    let priority_row =
        find_widget(dialog.upcast_ref(), "base-visible-field-3").ok_or("priority row")?;
    let controllers = priority_row.observe_controllers();
    let drop_target = (0..controllers.n_items())
        .find_map(|index| controllers.item(index)?.downcast::<gtk::DropTarget>().ok())
        .ok_or("visible field drop target")?;
    assert!(drop_target.emit_by_name::<bool>(
        "drop",
        &[
            &glib::BoxedValue("property:/owner".to_value()),
            &0.0_f64.to_value(),
            &0.0_f64.to_value(),
        ]
    ));
    let reordered_first =
        find_widget(dialog.upcast_ref(), "base-visible-field-3").ok_or("reordered first row")?;
    let reordered_second =
        find_widget(dialog.upcast_ref(), "base-visible-field-4").ok_or("reordered second row")?;
    assert!(super::find_label(&reordered_first, "owner").is_some());
    assert!(super::find_label(&reordered_second, "priority").is_some());
    let remove_filter = widget_as::<gtk::Button>(dialog.upcast_ref(), "base-rule-filter-remove-0")
        .ok_or("remove filter button")?;
    remove_filter.emit_clicked();
    assert_eq!(preview_label.text(), "Currently matches 1 note");
    let scroll = preview
        .ancestor(gtk::ScrolledWindow::static_type())
        .and_downcast::<gtk::ScrolledWindow>()
        .ok_or("configuration scroll view")?;
    assert!(scroll.vexpands());
    assert!(scroll.min_content_height() >= 560);
    dialog.close();
    window.close();
    Ok(())
}
