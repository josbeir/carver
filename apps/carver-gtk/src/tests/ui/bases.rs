//! Saved Base header-action interaction coverage.
use crate::mvu::{AppDispatcher, AppModel, AppRuntime, LoadState, Route};
use crate::ui::tests::support::{TestResult, run_main_context_until, test_state, widget_as};
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
