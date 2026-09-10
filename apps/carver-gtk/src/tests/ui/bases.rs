//! Saved Base context-action interaction coverage.
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
    model.route = Route::Base;
    sidebar.render(&model);
    let routes = gtk::Stack::new();
    for name in ["browser", "base"] {
        routes.add_named(&gtk::Box::new(gtk::Orientation::Vertical, 0), Some(name));
    }
    let runtime = AppRuntime::new(
        client.clone(),
        model,
        crate::view::ViewRefs::new(routes, adw::StatusPage::new(), adw::StatusPage::new())
            .with_sidebar_renderer(move |model| sidebar_for_view.render(model)),
    );
    runtime.bind_dispatcher(&dispatcher);
    let window = adw::Window::new();
    window.set_default_size(500, 700);
    window.set_content(Some(&sidebar.widget));
    window.present();
    let button = widget_as::<gtk::Button>(&sidebar.widget, &format!("base:{}", base.id))
        .ok_or("base button")?;
    assert!(run_main_context_until(|| button.is_mapped()));
    let controllers = button.observe_controllers();
    let keys = (0..controllers.n_items())
        .find_map(|i| {
            controllers
                .item(i)
                .and_downcast::<gtk::EventControllerKey>()
        })
        .ok_or("context keys")?;
    for response in ["cancel", "delete"] {
        assert!(keys.emit_by_name::<bool>(
            "key-pressed",
            &[
                &gtk::gdk::Key::Menu,
                &0_u32,
                &gtk::gdk::ModifierType::empty()
            ]
        ));
        let delete = widget_as::<gtk::Button>(button.upcast_ref(), "delete-base-action")
            .ok_or("delete action")?;
        delete.emit_clicked();
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
