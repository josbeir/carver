//! Display-backed unified creation tests, run on the shared GTK test thread.
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use crate::mvu::{AppDispatcher, AppModel, AppRuntime};
use crate::ui::tests::support::{
    TestResult, run_main_context_until, run_main_context_until_for, test_state, widget_as,
};
use gtk::prelude::*;
use libadwaita::{self as adw, prelude::*};

#[expect(
    clippy::too_many_lines,
    reason = "One interaction scenario covers the complete shared creation flow"
)]
pub(super) fn add_dialog_should_create_category_and_configure_new_base() -> TestResult {
    let (_temporary, client) = test_state()?;
    let template = glib::MainContext::default()
        .block_on(client.create_template_async("Meeting".into(), "# Agenda".into()))?;
    let dispatcher = AppDispatcher::default();
    let routes = gtk::Stack::new();
    routes.add_named(
        &gtk::Box::new(gtk::Orientation::Vertical, 0),
        Some("browser"),
    );
    let split_view = adw::NavigationSplitView::new();
    let compact_navigation = Rc::new(Cell::new(false));
    let (base, base_refs) =
        crate::ui::bases::build_base(&dispatcher, &split_view, &compact_navigation);
    routes.add_named(&base, Some("base"));
    routes.set_visible_child_name("browser");
    let add_dialog: crate::ui::add::AddDialogSlot = Rc::new(RefCell::new(None));
    let runtime = AppRuntime::new(
        client.clone(),
        AppModel::new(&carver_config::Config::default()),
        crate::view::ViewRefs::new(
            routes.clone(),
            adw::StatusPage::new(),
            adw::StatusPage::new(),
        )
        .with_dispatcher(dispatcher.clone())
        .with_add_dialog(Rc::clone(&add_dialog))
        .with_base(base_refs),
    );
    runtime.bind_dispatcher(&dispatcher);
    let button = crate::ui::add::button(&dispatcher, Rc::clone(&add_dialog));
    button.set_halign(gtk::Align::Start);
    button.set_valign(gtk::Align::Start);
    let window = adw::Window::new();
    window.set_default_size(400, 900);
    let window_content = gtk::Box::new(gtk::Orientation::Vertical, 0);
    window_content.append(&routes);
    window_content.append(&button);
    window.set_content(Some(&window_content));
    window.present();
    assert!(run_main_context_until(|| button.is_mapped()));
    button.emit_clicked();
    let dialog = window.visible_dialog().ok_or("dialog")?;
    let root = dialog.upcast_ref::<gtk::Widget>();
    assert!(run_main_context_until(
        || dialog.width() > 0 && dialog.height() > 250
    ));
    capture_dialog(&dialog, "tabs-category")?;
    let stack = widget_as::<adw::ViewStack>(root, "add-stack").ok_or("tabs")?;
    assert_eq!(stack.visible_child_name().as_deref(), Some("category"));
    let switcher = widget_as::<adw::ViewSwitcher>(root, "add-switcher").ok_or("switcher")?;
    assert_eq!(switcher.policy(), adw::ViewSwitcherPolicy::Wide);
    let category_page = stack.child_by_name("category").ok_or("category tab")?;
    assert_eq!(
        stack.page(&category_page).icon_name().as_deref(),
        Some("folder-symbolic")
    );
    let base_page = stack.child_by_name("base").ok_or("base tab")?;
    assert_eq!(
        stack.page(&base_page).icon_name().as_deref(),
        Some("carver-database-symbolic")
    );

    let entry = widget_as::<adw::EntryRow>(root, "category-name-entry").ok_or("category name")?;
    let create = widget_as::<gtk::Button>(root, "add-category-create").ok_or("create category")?;
    entry.set_text("  ");
    assert!(!create.is_sensitive());
    entry.set_text("  Work  ");
    assert!(create.ancestor(adw::HeaderBar::static_type()).is_some());
    let selector = widget_as::<adw::ComboRow>(root, "category-default-template")
        .ok_or("template selector in creation")?;
    assert!(run_main_context_until(|| selector.is_sensitive()));
    assert_eq!(selector.selected(), 0);
    selector.set_selected(1);
    let icon_picker =
        widget_as::<gtk::MenuButton>(root, "category-icon-picker").ok_or("compact icon picker")?;
    icon_picker.popup();
    widget_as::<gtk::ToggleButton>(root, "category-icon-book")
        .ok_or("book icon")?
        .set_active(true);
    assert_eq!(
        icon_picker.icon_name().as_deref(),
        Some("x-office-document-symbolic")
    );
    let color_picker = widget_as::<gtk::MenuButton>(root, "category-color-picker")
        .ok_or("compact colour picker")?;
    color_picker.popup();
    widget_as::<gtk::ToggleButton>(root, "category-color-teal")
        .ok_or("teal colour")?
        .set_active(true);
    assert!(
        widget_as::<gtk::Box>(root, "category-color-swatch")
            .ok_or("colour swatch")?
            .has_css_class("category-color-teal")
    );
    // Pressing Enter must create the category without touching the Create button.
    entry.emit_by_name::<()>("entry-activated", &[]);
    assert!(run_main_context_until(|| client
        .categories()
        .is_ok_and(|items| items.len() == 1)));
    assert_eq!(client.categories()?[0].name, "Work");
    assert_eq!(
        client.categories()?[0].default_template_id,
        Some(template.id)
    );
    assert_eq!(
        client.categories()?[0].appearance.color,
        carver_sdk::CategoryColor::Teal
    );
    assert_eq!(
        client.categories()?[0].appearance.icon,
        carver_sdk::CategoryIcon::Book
    );

    // Cancel discards form changes, and reopening resets the template choice.
    button.emit_clicked();
    let canceled = window
        .visible_dialog()
        .ok_or("cancelable category dialog")?;
    widget_as::<adw::EntryRow>(canceled.upcast_ref(), "category-name-entry")
        .ok_or("cancelable name")?
        .set_text("Discard me");
    widget_as::<gtk::Button>(canceled.upcast_ref(), "category-cancel")
        .ok_or("header Cancel")?
        .emit_clicked();
    assert!(run_main_context_until(|| window.visible_dialog().is_none()));
    assert_eq!(client.categories()?.len(), 1);

    // The Base tab prepares its form lazily, only once it has been selected.
    button.emit_clicked();
    let dialog = window.visible_dialog().ok_or("dialog")?;
    let root = dialog.upcast_ref::<gtk::Widget>();
    let stack = widget_as::<adw::ViewStack>(root, "add-stack").ok_or("fresh tabs")?;
    assert_eq!(stack.visible_child_name().as_deref(), Some("category"));
    let selector =
        widget_as::<adw::ComboRow>(root, "category-default-template").ok_or("reopened selector")?;
    assert!(run_main_context_until(|| selector.is_sensitive()));
    assert_eq!(selector.selected(), 0);

    assert!(run_main_context_until(|| dialog.height() > 250));
    let content = dialog.child().ok_or("dialog content")?;
    assert!(run_main_context_until(
        || content.width() > 0 && content.height() > 0
    ));
    let category_height = content.height();
    let category_width = content.width();
    stack.set_visible_child_name("base");
    assert!(
        run_main_context_until(|| {
            widget_as::<adw::EntryRow>(dialog.upcast_ref(), "base-configuration-name").is_some()
        }),
        "model: {:?}",
        runtime.model().notice,
    );
    capture_dialog(&dialog, "tabs-base")?;
    assert!(
        run_main_context_until(|| content.width() == category_width),
        "both tabs should share the dialog width: {category_width} -> {}",
        content.width()
    );
    assert!(
        run_main_context_until(|| content.height() > category_height + 50),
        "the Base tab should grow the dialog: {category_height} -> {}",
        content.height()
    );
    let base_height = content.height();
    // Returning to Category must restore the compact size instead of keeping the
    // largest page's dimensions.
    stack.set_visible_child_name("category");
    assert!(
        run_main_context_until(|| content.height() <= category_height + 20),
        "returning to Category should shrink the dialog back: {category_height} -> {}",
        content.height()
    );
    stack.set_visible_child_name("base");
    assert!(run_main_context_until(
        || content.height() >= base_height - 20
    ));
    let entry =
        widget_as::<adw::EntryRow>(root, "base-configuration-name").ok_or("new Base name")?;
    assert!(entry.text().is_empty());
    // A new Base starts with only the implicit Title field, not Category or Updated.
    assert!(
        widget_as::<adw::ActionRow>(root, "base-visible-field-0").is_some(),
        "a new Base should list the Title field"
    );
    assert!(
        widget_as::<adw::ActionRow>(root, "base-visible-field-1").is_none(),
        "a new Base should not seed Category or Updated fields"
    );
    entry.set_text("  Reading list  ");
    widget_as::<gtk::Button>(root, "base-configuration-save")
        .ok_or("create Base")?
        .emit_clicked();
    assert!(run_main_context_until(
        || matches!(&runtime.model().bases.definitions.state,
        crate::mvu::LoadState::Ready(items) if items.len() == 1 && items[0].name == "Reading list")
    ));
    assert!(run_main_context_until(|| window.visible_dialog().is_none()));

    // A duplicate name keeps the Add dialog open and preserves the draft.
    button.emit_clicked();
    let dialog = window.visible_dialog().ok_or("dialog")?;
    let root = dialog.upcast_ref::<gtk::Widget>();
    widget_as::<adw::ViewStack>(root, "add-stack")
        .ok_or("tabs")?
        .set_visible_child_name("base");
    assert!(run_main_context_until(|| {
        widget_as::<adw::EntryRow>(dialog.upcast_ref(), "base-configuration-name").is_some()
    }));
    let duplicate_name =
        widget_as::<adw::EntryRow>(root, "base-configuration-name").ok_or("new Base name")?;
    duplicate_name.set_text("Reading list");
    widget_as::<gtk::Button>(root, "base-configuration-save")
        .ok_or("create duplicate Base")?
        .emit_clicked();
    assert!(run_main_context_until(|| runtime.model().notice.is_some()));
    assert!(
        widget_as::<adw::EntryRow>(root, "base-configuration-name")
            .ok_or("preserved duplicate Base draft")?
            .is_sensitive()
    );
    assert_eq!(duplicate_name.text(), "Reading list");
    dialog.close();

    // Reopening starts from a fresh Base draft.
    button.emit_clicked();
    let dialog = window.visible_dialog().ok_or("dialog")?;
    let root = dialog.upcast_ref::<gtk::Widget>();
    widget_as::<adw::ViewStack>(root, "add-stack")
        .ok_or("tabs")?
        .set_visible_child_name("base");
    assert!(run_main_context_until(|| {
        widget_as::<adw::EntryRow>(dialog.upcast_ref(), "base-configuration-name").is_some()
    }));
    assert!(
        widget_as::<adw::EntryRow>(root, "base-configuration-name")
            .ok_or("reset new Base name")?
            .text()
            .is_empty()
    );
    dialog.close();
    window.close();
    Ok(())
}

pub(super) fn add_dialog_should_balance_page_sizes() -> TestResult {
    let window = adw::Window::new();
    window.set_default_size(1120, 900);
    let add_dialog: crate::ui::add::AddDialogSlot = Rc::new(RefCell::new(None));
    let button = crate::ui::add::button(&AppDispatcher::default(), add_dialog);
    button.set_halign(gtk::Align::Start);
    button.set_valign(gtk::Align::Start);
    window.set_content(Some(&button));
    window.present();
    assert!(run_main_context_until(|| button.is_mapped()));
    button.emit_clicked();
    let dialog = window.visible_dialog().ok_or("dialog")?;
    assert_eq!(dialog.content_width(), 560);
    let root = dialog.upcast_ref::<gtk::Widget>();
    let stack = widget_as::<adw::ViewStack>(root, "add-stack").ok_or("tabs")?;
    assert!(run_main_context_until(|| dialog.height() > 250));
    let content = dialog.child().ok_or("dialog content")?;
    assert!(run_main_context_until(
        || content.width() > 0 && content.height() > 0
    ));
    let category_height = content.height();
    let category_width = content.width();
    let form = widget_as::<gtk::Box>(root, "category-dialog-content").ok_or("shared form")?;
    let scroll = form
        .ancestor(gtk::ScrolledWindow::static_type())
        .and_downcast::<gtk::ScrolledWindow>()
        .ok_or("category scroller")?;
    assert!(
        run_main_context_until(|| {
            let adjustment = scroll.vadjustment();
            adjustment.page_size() > 0.0 && adjustment.upper() <= adjustment.page_size() + 1.0
        }),
        "all category rows should fit on a desktop without scrolling"
    );
    capture_dialog(&dialog, "tabs-category-desktop")?;
    stack.set_visible_child_name("base");
    assert!(run_main_context_until(|| {
        widget_as::<gtk::Spinner>(root, "add-base-spinner").is_some()
    }));
    assert!(
        run_main_context_until(|| content.width() == category_width),
        "the Base tab should keep the dialog width: {category_width} -> {}",
        content.width()
    );
    capture_dialog(&dialog, "tabs-base-loading-desktop")?;
    stack.set_visible_child_name("category");
    assert!(
        run_main_context_until(|| {
            (content.height() - category_height).abs() <= 4
                && (content.width() - category_width).abs() <= 4
        }),
        "the dialog should return to the category size: {}x{} -> {}x{}",
        category_width,
        category_height,
        content.width(),
        content.height()
    );
    dialog.close();
    window.close();
    Ok(())
}

pub(super) fn add_dialog_should_cancel_base_setup_when_closed_while_loading() -> TestResult {
    let (_temporary, client) = test_state()?;
    let dispatcher = AppDispatcher::default();
    let routes = gtk::Stack::new();
    routes.add_named(
        &gtk::Box::new(gtk::Orientation::Vertical, 0),
        Some("browser"),
    );
    let add_dialog: crate::ui::add::AddDialogSlot = Rc::new(RefCell::new(None));
    let runtime = AppRuntime::new(
        client,
        AppModel::new(&carver_config::Config::default()),
        crate::view::ViewRefs::new(
            routes.clone(),
            adw::StatusPage::new(),
            adw::StatusPage::new(),
        )
        .with_dispatcher(dispatcher.clone())
        .with_add_dialog(Rc::clone(&add_dialog)),
    );
    runtime.bind_dispatcher(&dispatcher);
    let button = crate::ui::add::button(&dispatcher, Rc::clone(&add_dialog));
    let window = adw::Window::new();
    window.set_default_size(400, 900);
    window.set_content(Some(&button));
    window.present();
    assert!(run_main_context_until(|| button.is_mapped()));
    button.emit_clicked();
    let dialog = window.visible_dialog().ok_or("dialog")?;
    let stack = widget_as::<adw::ViewStack>(dialog.upcast_ref(), "add-stack").ok_or("tabs")?;
    stack.set_visible_child_name("base");
    // The descriptor load is still pending, so dismiss the workflow now.
    dialog.close();
    assert!(run_main_context_until(|| window.visible_dialog().is_none()));
    assert!(runtime.model().bases.configuration_request.is_none());
    // A late descriptor reply must not resurrect the standalone dialog.
    let _ = run_main_context_until_for(std::time::Duration::from_millis(300), || false);
    assert!(window.visible_dialog().is_none());
    assert!(runtime.model().bases.configuration_dialog.is_none());
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
