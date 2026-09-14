//! Display-backed unified creation tests, run on the shared GTK test thread.
use crate::mvu::{AppDispatcher, AppModel, AppRuntime};
use crate::ui::tests::support::{TestResult, run_main_context_until, test_state, widget_as};
use gtk::prelude::*;
use libadwaita::{self as adw, prelude::*};

#[expect(
    clippy::too_many_lines,
    reason = "One interaction scenario covers the complete shared creation flow"
)]
pub(super) fn add_dialog_should_create_category_and_configure_new_base() -> TestResult {
    let (_temporary, client) = test_state()?;
    let dispatcher = AppDispatcher::default();
    let routes = gtk::Stack::new();
    for name in ["browser", "base"] {
        routes.add_named(&gtk::Box::new(gtk::Orientation::Vertical, 0), Some(name));
    }
    routes.set_visible_child_name("browser");
    let runtime = AppRuntime::new(
        client.clone(),
        AppModel::new(&carver_config::Config::default()),
        crate::view::ViewRefs::new(
            routes.clone(),
            adw::StatusPage::new(),
            adw::StatusPage::new(),
        )
        .with_dispatcher(dispatcher.clone()),
    );
    runtime.bind_dispatcher(&dispatcher);
    let button = crate::ui::add::button(&dispatcher);
    button.set_halign(gtk::Align::Start);
    button.set_valign(gtk::Align::Start);
    let window = adw::Window::new();
    window.set_default_size(360, 640);
    let window_content = gtk::Box::new(gtk::Orientation::Vertical, 0);
    window_content.append(&routes);
    window_content.append(&button);
    window.set_content(Some(&window_content));
    window.present();
    assert!(run_main_context_until(|| button.is_mapped()));
    button.emit_clicked();
    let dialog = window.visible_dialog().ok_or("dialog")?;
    let root = dialog.upcast_ref();
    assert!(run_main_context_until(
        || dialog.width() > 0 && dialog.height() > 250
    ));
    assert!(dialog.width() <= 360, "dialog width: {}", dialog.width());
    capture_dialog(&dialog, "chooser")?;
    let pages = widget_as::<gtk::Stack>(root, "add-pages").ok_or("pages")?;
    assert_eq!(pages.visible_child_name().as_deref(), Some("choose"));
    widget_as::<gtk::Button>(root, "add-category-choice")
        .ok_or("category choice")?
        .emit_clicked();
    let entry = widget_as::<gtk::Entry>(root, "category-name-entry").ok_or("category name")?;
    let create = widget_as::<gtk::Button>(root, "add-category-create").ok_or("create category")?;
    entry.set_text("  ");
    assert!(!create.is_sensitive());
    entry.set_text("  Work  ");
    widget_as::<gtk::ToggleButton>(root, "category-icon-book")
        .ok_or("book icon")?
        .set_active(true);
    widget_as::<gtk::Button>(root, "add-category-back")
        .ok_or("back")?
        .emit_clicked();
    widget_as::<gtk::Button>(root, "add-category-choice")
        .ok_or("category choice")?
        .emit_clicked();
    assert_eq!(entry.text(), "  Work  ");
    entry.emit_activate();
    create.emit_clicked();
    assert!(run_main_context_until(|| client
        .categories()
        .is_ok_and(|items| items.len() == 1)));
    assert_eq!(client.categories()?[0].name, "Work");
    assert_eq!(
        client.categories()?[0].appearance.icon,
        carver_sdk::CategoryIcon::Book
    );
    button.emit_clicked();
    let dialog = window.visible_dialog().ok_or("dialog")?;
    let root = dialog.upcast_ref();
    let pages = widget_as::<gtk::Stack>(root, "add-pages").ok_or("fresh pages")?;
    assert_eq!(pages.visible_child_name().as_deref(), Some("choose"));
    widget_as::<gtk::Button>(root, "add-base-choice")
        .ok_or("base choice")?
        .emit_clicked();
    assert!(
        run_main_context_until(|| {
            window.visible_dialog().is_some_and(|dialog| {
                dialog.widget_name() == "base-configuration-dialog" && dialog.is_mapped()
            })
        }),
        "visible dialog: {:?}; model: {:?}",
        window.visible_dialog().map(|dialog| (
            dialog.widget_name().clone(),
            dialog.title().clone(),
            dialog.is_mapped(),
        )),
        runtime.model().notice,
    );
    let dialog = window.visible_dialog().ok_or("new Base configuration")?;
    let root = dialog.upcast_ref();
    assert!(widget_as::<gtk::Expander>(root, "base-visible-fields-section").is_some());
    assert!(widget_as::<gtk::Expander>(root, "base-filters-section").is_some());
    assert!(widget_as::<gtk::Expander>(root, "base-sort-section").is_some());
    let entry = widget_as::<gtk::Entry>(root, "base-configuration-name").ok_or("new Base name")?;
    assert!(entry.text().is_empty());
    entry.set_text("  Reading list  ");
    widget_as::<gtk::Button>(root, "base-configuration-save")
        .ok_or("create Base")?
        .emit_clicked();
    assert!(run_main_context_until(
        || matches!(&runtime.model().bases.definitions.state,
        crate::mvu::LoadState::Ready(items) if items.len() == 1 && items[0].name == "Reading list")
    ));
    button.emit_clicked();
    let dialog = window.visible_dialog().ok_or("dialog")?;
    let root = dialog.upcast_ref();
    widget_as::<gtk::Button>(root, "add-base-choice")
        .ok_or("base choice")?
        .emit_clicked();
    assert!(run_main_context_until(|| {
        window.visible_dialog().is_some_and(|dialog| {
            dialog.widget_name() == "base-configuration-dialog" && dialog.is_mapped()
        })
    }));
    let dialog = window.visible_dialog().ok_or("discard configuration")?;
    let root = dialog.upcast_ref();
    widget_as::<gtk::Entry>(root, "base-configuration-name")
        .ok_or("new Base name")?
        .set_text("Discard");
    dialog.close();
    button.emit_clicked();
    let dialog = window.visible_dialog().ok_or("dialog")?;
    widget_as::<gtk::Button>(dialog.upcast_ref(), "add-base-choice")
        .ok_or("base choice")?
        .emit_clicked();
    assert!(run_main_context_until(|| {
        window.visible_dialog().is_some_and(|dialog| {
            dialog.widget_name() == "base-configuration-dialog" && dialog.is_mapped()
        })
    }));
    let dialog = window.visible_dialog().ok_or("fresh Base configuration")?;
    assert!(
        widget_as::<gtk::Entry>(dialog.upcast_ref(), "base-configuration-name")
            .ok_or("reset new Base name")?
            .text()
            .is_empty()
    );
    dialog.close();
    window.close();
    Ok(())
}

pub(super) fn add_dialog_should_resize_for_the_active_form() -> TestResult {
    let window = adw::Window::new();
    window.set_default_size(1120, 900);
    let button = crate::ui::add::button(&AppDispatcher::default());
    button.set_halign(gtk::Align::Start);
    button.set_valign(gtk::Align::Start);
    window.set_content(Some(&button));
    window.present();
    assert!(run_main_context_until(|| button.is_mapped()));
    button.emit_clicked();
    let dialog = window.visible_dialog().ok_or("dialog")?;
    assert!(dialog.follows_content_size());
    let root = dialog.upcast_ref();
    let pages = widget_as::<gtk::Stack>(root, "add-pages").ok_or("pages")?;
    assert!(!pages.is_hhomogeneous());
    assert!(!pages.is_vhomogeneous());
    assert!(run_main_context_until(|| dialog.height() > 250));
    capture_dialog(&dialog, "chooser-desktop")?;
    widget_as::<gtk::Button>(root, "add-category-choice")
        .ok_or("category choice")?
        .emit_clicked();
    let content = dialog.child().ok_or("dialog content")?;
    assert!(run_main_context_until(
        || !pages.is_transition_running() && content.height() > 300
    ));
    let scroll = pages
        .parent()
        .and_then(|parent| parent.parent())
        .and_downcast::<gtk::ScrolledWindow>()
        .ok_or("scroll")?;
    assert!(run_main_context_until(
        || scroll.vadjustment().upper() <= scroll.vadjustment().page_size() + 1.0
    ));
    capture_dialog(&dialog, "category-desktop")?;
    dialog.close();
    window.close();
    Ok(())
}

fn capture_dialog(dialog: &adw::Dialog, name: &str) -> TestResult {
    let Some(directory) = std::env::var_os("CARVER_ADD_SCREENSHOTS") else {
        return Ok(());
    };
    let widget = dialog.child().ok_or("dialog content")?;
    let paintable = gtk::WidgetPaintable::new(Some(&widget));
    let captured = std::cell::RefCell::new(None);
    assert!(run_main_context_until(|| {
        let snapshot = gtk::Snapshot::new();
        paintable.snapshot(
            &snapshot,
            f64::from(widget.width()),
            f64::from(widget.height()),
        );
        let node = snapshot.to_node();
        let ready = node.is_some();
        *captured.borrow_mut() = node;
        ready
    }));
    let node = captured.into_inner().ok_or("snapshot node")?;
    let renderer = dialog
        .native()
        .and_then(|native| native.renderer())
        .ok_or("renderer")?;
    renderer
        .render_texture(&node, None)
        .save_to_png(std::path::PathBuf::from(directory).join(format!("{name}.png")))?;
    Ok(())
}
